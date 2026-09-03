pub mod core;
pub mod down;
pub mod services;
pub mod up;
pub mod utils;

use std::{
    net::SocketAddr,
    path::Path,
    pin::Pin,
    str::FromStr,
    sync::{Arc, PoisonError},
};

use anyhow::Context;
use log::{debug, error, info};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::{
    core::{Source, config::ConfigurationDesc, route::Routes},
    down::ConnectionService,
    services::RouterService,
};

pub type PinnedFuture<T> = Pin<Box<dyn Future<Output = T> + 'static + Send>>;

pub const DEFAULT_HEADER_SIZE_MAX: u32 = 8 * 1024;

/// At least 100 as recommended in the [HTTP/2 RFC](https://httpwg.org/specs/rfc9113.html#SETTINGS_MAX_CONCURRENT_STREAMS)
pub const DEFAULT_CONNECTION_CONCURRENT_REQUESTS_MAX: u32 = 200;

pub struct Request {
    pub inner: RawRequest,
    pub path_extension: String,
}

pub type RawRequest = hyper::Request<hyper::body::Incoming>;

pub type IncomingResponse = hyper::Response<hyper::body::Incoming>;
pub type FullResponse = hyper::Response<http_body_util::Full<hyper::body::Bytes>>;

pub struct Warden {
    host: SocketAddr,

    connection_service: ConnectionService,

    config: Arc<ConfigurationDesc>,

    routes: Arc<Routes<Source>>,
}

impl Warden {
    pub async fn start(config_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let config_path = config_path.as_ref();

        let config = Arc::new(ConfigurationDesc::from_path_or_default(config_path).await);
        let tls_acceptor = match &config.tls {
            Some(tls) => {
                // Setup TLS
                let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

                let certs = CertificateDer::pem_file_iter(&tls.certs)?
                    .collect::<Result<Vec<_>, _>>()
                    .with_context(|| "failed to read cert file")?;

                let key = PrivateKeyDer::from_pem_file(&tls.key)
                    .with_context(|| "failed to read private key file")?;

                let mut server_config = ServerConfig::builder()
                    .with_no_client_auth()
                    .with_single_cert(certs, key)
                    .with_context(|| "failed to create TLS server config")?;

                server_config.alpn_protocols =
                    vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
                Some(TlsAcceptor::from(Arc::new(server_config)))
            }
            None => None,
        };
        let host: SocketAddr = SocketAddr::from_str(&format!("{}:{}", config.host, config.port))
            .with_context(|| "failed to parse host")?;
        let listener: TcpListener = TcpListener::bind(host).await?;

        info!("server started @ {}", host);

        debug!("config: {config:#?}");

        let (routes, errors) = RouterService::parse_routes(&config);
        let routes = Arc::new(routes);

        if !errors.is_empty() {
            return Err(anyhow::anyhow!("failed to parse routes: {:#?}", errors));
        }

        let connection_service = ConnectionService::new(
            listener,
            tls_acceptor,
            Arc::clone(&config),
            Arc::clone(&routes),
        );

        Ok(Self {
            host,
            connection_service,

            config,

            routes,
        })
    }

    pub fn host(&self) -> &SocketAddr {
        &self.host
    }

    pub async fn serve_next(&mut self) -> anyhow::Result<()> {
        self.connection_service.serve_next_connection().await
    }

    pub async fn close(&self) -> anyhow::Result<()> {
        Ok(self.config.save_if_missing().await?)
    }
}

pub trait UnwrapLog<T> {
    fn unwrap_log(self) -> T;
}

impl<T> UnwrapLog<T> for Result<T, PoisonError<T>> {
    fn unwrap_log(self) -> T {
        match self {
            Ok(v) => v,
            Err(err) => {
                error!("{err:?}",);
                err.into_inner()
            }
        }
    }
}
