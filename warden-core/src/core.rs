pub mod config;

use anyhow::Context;
use http_body_util::Full;
use hyper::{body::Bytes, server::conn::http2, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{error, info, trace};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs::File,
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    select,
    sync::{Mutex, RwLock},
};
use tokio_rustls::TlsAcceptor;

use crate::{
    auth::{AuthProvider, Authorization, DefaultAuthProvider},
    core::config::Configuration,
    utils::{path, r_401, r_404, r_500},
};

// Tasks:
//   - Accept connections and spawn handler
//   - Perform health checks
//   - Wait for termination signal
struct WardenState {
    connections: Vec<ConnectionInfo>,
}

struct WardenRouter {
    upstream: HashMap<String, Upstream>,
}

impl Default for WardenRouter {
    fn default() -> Self {
        let upstream = HashMap::new();

        Self { upstream }
    }
}

#[derive(Clone)]
pub struct Warden {
    inner: Arc<WardenInner>,
}

pub struct WardenInner {
    host: SocketAddr,
    listener: TcpListener,
    tls_acceptor: TlsAcceptor,

    state: Mutex<WardenState>,
    router: Arc<WardenRouter>,
    config: Arc<Configuration>,
}

impl Warden {
    pub async fn bind(host: SocketAddr, config_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let config_path = config_path.as_ref();

        // Setup TLS
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let certs = CertificateDer::pem_file_iter("temp/server.crt")?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| "failed to read cert file")?;

        let key = PrivateKeyDer::from_pem_file("temp/server.key")
            .with_context(|| "failed to read private key file")?;

        let mut server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .with_context(|| "failed to create TLS server config")?;

        server_config.alpn_protocols =
            vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
        let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

        let listener: TcpListener = TcpListener::bind(host).await?;

        info!("server started @ {}", host);

        let state = Mutex::new(WardenState {
            connections: vec![],
        });

        let router = Arc::new(WardenRouter::default());

        Ok(Self {
            inner: Arc::new(WardenInner {
                tls_acceptor,
                host,
                listener,
                state,
                router,
                config: Arc::new(Configuration::from_path_or_default(config_path).await),
            }),
        })
    }

    pub async fn serve_next(&self) -> anyhow::Result<()> {
        select! {
            conn = self.inner.listener.accept() => {
                if let Err(e) = self.handle_new_connection(conn).await {
                    error!("{}", e.context("failed to handle new connection"));
                }
                Ok(())
            }
        }
    }

    pub fn host(&self) -> &SocketAddr {
        &self.inner.host
    }

    async fn serve_request(
        &self,
        request: hyper::Request<hyper::body::Incoming>,
    ) -> anyhow::Result<crate::Response> {
        let path = path(&request);

        let verified = DefaultAuthProvider::verify_request(&request);

        match verified {
            Ok(v) => {
                if let Authorization::Blocked = v {
                    return Ok(r_401());
                }
            }
            Err(e) => {
                error!("{}", e.context("error verifying request"));
                return Ok(r_500());
            }
        }

        if let Some(upstream) = self.inner.config.handlers.get(path) {
            upstream.call(request).await
        } else {
            Ok(r_404())
        }
    }

    async fn handle_new_connection(
        &self,
        conn: std::io::Result<(TcpStream, SocketAddr)>,
    ) -> anyhow::Result<()> {
        let (stream, addr) = conn.with_context(|| "failed to open connection")?;
        trace!("new connection: {}", addr);

        let mut state = self.inner.state.lock().await;

        state.connections.push(ConnectionInfo {
            host: addr,
            user_agent: None,
        });
        let acceptor = self.inner.tls_acceptor.clone();

        let warden = self.clone();

        tokio::spawn(async move {
            let tls_stream = match acceptor
                .accept(stream)
                .await
                .with_context(|| "failed to perform tls handshake")
            {
                Ok(tls_stream) => tls_stream,
                Err(e) => {
                    error!("{e:#}");
                    return;
                }
            };
            let io = TokioIo::new(tls_stream);
            if let Err(e) = http2::Builder::new(TokioExecutor::new())
                .serve_connection(
                    io,
                    service_fn(move |r| {
                        let warden = warden.clone();
                        async move { warden.serve_request(r).await }
                    }),
                )
                .await
            {
                error!(
                    "{:#}",
                    anyhow::Error::from(e).context("failed to serve request")
                );
            }
        });

        Ok(())
    }

    async fn connections_snapshot(&self) -> Vec<ConnectionInfo> {
        self.inner.state.lock().await.connections.clone()
    }

    pub async fn close(&self) -> anyhow::Result<()> {
        Ok(self.inner.config.save_if_missing().await?)
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub host: SocketAddr,
    pub user_agent: Option<String>,
}

pub struct Upstream {
    source: Source,
}

#[derive(Debug, Default)]
pub enum Source {
    StaticHtml(Vec<u8>),

    /// This type reads the HTML file each time the page is requested.
    /// Should not be used for high traffic routes since it's more computationally
    /// expensive.
    DynamicHtml(PathBuf),
    Http,
    Https,

    #[default]
    Unknown,
}

impl Upstream {
    /// Prepare source for serving connections.
    async fn new_html(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let display_path = path.as_ref().as_os_str();

        let mut content = Vec::new();
        let mut file = File::open(path.as_ref())
            .await
            .with_context(|| format!("unable to open html file at {display_path:?}"))?;

        let meta = file
            .metadata()
            .await
            .with_context(|| format!("unable to read metadata of html file at {display_path:?}"))?;

        if meta.len() > crate::MAX_STATIC_HTML_FILE_SIZE {
            return Err(anyhow::Error::msg(format!(
                "html file at {display_path:?} exceeds max size {}",
                crate::MAX_STATIC_HTML_FILE_SIZE
            )));
        }

        file.read_to_end(&mut content)
            .await
            .with_context(|| format!("unable to read html file at {display_path:?}"))?;

        Ok(Self {
            source: Source::StaticHtml(content),
        })
    }

    async fn call(&self, _request: crate::Request) -> anyhow::Result<crate::Response> {
        match &self.source {
            Source::StaticHtml(d) => Ok(crate::Response::new(Full::new(Bytes::from(d.clone())))),
            Source::DynamicHtml(p) => {
                let mut buf = Vec::new();
                File::open(p)
                    .await
                    .with_context(|| "could not open dynamic page")?
                    .read_to_end(&mut buf)
                    .await
                    .with_context(|| "could not read dynamic page")?;

                Ok(crate::Response::new(Full::new(Bytes::from(buf))))
            }
            _ => unimplemented!(),
        }
    }
}

pub struct Downstream {
    stream: TokioIo<TcpStream>,
}

#[derive(Debug, Clone)]
pub struct Role {
    id: u64,
    metadata: Arc<RwLock<RoleMetadata>>,
}

#[derive(Debug)]
pub struct RoleMetadata {
    name: String,
    keys: HashSet<String>,
}

pub enum Ruleset {
    AllowList(HashSet<String>),
    BlockList(HashSet<String>),
}

impl Ruleset {
    fn is_allowed(&self, key: &str) -> bool {
        match self {
            Ruleset::AllowList(l) => l.contains(key),
            Ruleset::BlockList(l) => !l.contains(key),
        }
    }
}
