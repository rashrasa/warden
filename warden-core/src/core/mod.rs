pub mod config;
pub mod route;

use anyhow::Context;
use http::{StatusCode, Uri};
use log::{error, info};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};
use tokio_rustls::TlsAcceptor;

use crate::{
    auth::{AuthProvider, Authorization},
    core::config::Configuration,
    down::Downstream,
    up::http1::Http1Upstream,
    utils::{http_error, path},
};

#[derive(Clone, Debug)]
pub struct Warden {
    inner: Arc<WardenInner>,
}

pub struct WardenInner {
    host: SocketAddr,
    listener: TcpListener,
    tls_acceptor: TlsAcceptor,

    auth: AuthProvider,
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

        let config = Arc::new(Configuration::from_path_or_default(config_path).await);
        let auth = AuthProvider {
            config: config.clone(),
        };

        Ok(Self {
            inner: Arc::new(WardenInner {
                tls_acceptor,
                host,
                listener,
                auth,
                config,
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

    pub async fn serve_request(
        &mut self,
        request: hyper::Request<hyper::body::Incoming>,
    ) -> anyhow::Result<crate::FullResponse> {
        let path = path(&request);

        let verified = self.inner.auth.verify_request(&request);

        match verified {
            Ok(v) => {
                if let Authorization::Blocked = v {
                    return Ok(http_error(StatusCode::UNAUTHORIZED));
                }
            }
            Err(e) => {
                error!("{}", e.context("error verifying request"));
                return Ok(http_error(StatusCode::INTERNAL_SERVER_ERROR));
            }
        }

        if let Some(upstream) = self.inner.config.handlers.get(path) {
            upstream.call(request).await
        } else {
            Ok(http_error(StatusCode::NOT_FOUND))
        }
    }

    async fn handle_new_connection(
        &self,
        conn: std::io::Result<(TcpStream, SocketAddr)>,
    ) -> anyhow::Result<()> {
        Downstream::handle_new_connection(self.clone(), self.inner.tls_acceptor.clone(), conn)
            .await?;
        Ok(())
    }

    pub async fn close(&self) -> anyhow::Result<()> {
        Ok(self.inner.config.save_if_missing().await?)
    }
}

impl std::fmt::Debug for WardenInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Address: {:?}\nConfiguration:{:?}",
            self.host, self.config
        )
    }
}

#[derive(Debug, Default)]
pub enum Source {
    StaticHtml(Vec<u8>),

    /// This type reads the HTML file each time the page is requested.
    /// Should not be used for high traffic routes since it's more computationally
    /// expensive.
    DynamicHtml(PathBuf),
    Http(Uri, Http1Upstream),
    Https,

    #[default]
    Unknown,
}
