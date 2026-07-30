pub mod config;
pub mod route;

use anyhow::Context;
use http::Uri;
use hyper::service::Service;
use log::info;
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::{
    PinnedFuture,
    core::config::ConfigurationDesc,
    down::ConnectionService,
    services::{AuthService, RouterService, ThrottleService},
    up::http1::Http1Upstream,
};

pub struct Warden {
    host: SocketAddr,

    request_service: RequestService,
    connection_service: ConnectionService,

    config: Arc<ConfigurationDesc>,
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

        let config = Arc::new(ConfigurationDesc::from_path_or_default(config_path).await);

        let request_service = RequestService::new(&config);
        let connection_service =
            ConnectionService::new(listener, tls_acceptor, request_service.clone());

        Ok(Self {
            host,
            connection_service,
            request_service,

            config,
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

#[derive(Clone)]
pub struct RequestService {
    inner: ThrottleService<AuthService<RouterService>>,
}

impl RequestService {
    pub fn new(config: &Arc<ConfigurationDesc>) -> Self {
        Self {
            inner: ThrottleService::new(AuthService::new(
                Arc::clone(config),
                RouterService::new(Arc::clone(config)),
            )),
        }
    }
}

impl Service<crate::Request> for RequestService {
    type Response = crate::FullResponse;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;
    type Error = anyhow::Error;

    fn call(&self, req: crate::Request) -> Self::Future {
        self.inner.call(req)
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
