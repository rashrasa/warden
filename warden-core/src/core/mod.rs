pub mod config;
pub mod route;

use anyhow::Context;
use http::{StatusCode, Uri, uri::PathAndQuery};
use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, service::Service};
use log::{error, info};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};
use tokio::{fs::File, io::AsyncReadExt, net::TcpListener};
use tokio_rustls::TlsAcceptor;

use crate::{
    PinnedFuture,
    core::config::ConfigurationDesc,
    down::ConnectionService,
    services::{AuthService, RouterService, ThrottleService},
    up::http1::Http1Upstream,
    utils,
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

        let (request_service, errors) = RequestService::new(&config);

        if !errors.is_empty() {
            error!("route parsing failed for some routes\n{errors:?}");
        }

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
    pub fn new(config: &Arc<ConfigurationDesc>) -> (Self, Vec<anyhow::Error>) {
        let (router, errors) = RouterService::new(Arc::clone(config));
        (
            Self {
                inner: ThrottleService::new(AuthService::new(Arc::clone(config), router)),
            },
            errors,
        )
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
pub struct Source {
    inner: Arc<SourceInner>,
}

impl Source {
    pub fn new(inner: SourceInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn inner(&self) -> &SourceInner {
        &self.inner
    }
}

impl Clone for Source {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Service<crate::Request> for Source {
    type Response = crate::FullResponse;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;
    type Error = anyhow::Error;
    fn call(&self, mut req: crate::Request) -> Self::Future {
        let source = self.clone();
        Box::pin(async move {
            match &*source.inner {
                SourceInner::StaticHtml(d) => {
                    Ok(crate::FullResponse::new(Full::new(Bytes::from(d.clone()))))
                }
                SourceInner::DynamicHtml(p) => {
                    let mut buf = Vec::new();
                    let mut file = match File::open(p)
                        .await
                        .with_context(|| "could not open dynamic page")
                    {
                        Ok(f) => f,
                        Err(e) => return Err(e),
                    };

                    file.read_to_end(&mut buf)
                        .await
                        .with_context(|| "could not read dynamic page")?;

                    Ok(crate::FullResponse::new(Full::new(Bytes::from(buf))))
                }
                SourceInner::Http(uri, sender) => {
                    let host = match uri.host() {
                        None => return Ok(utils::http_error(StatusCode::INTERNAL_SERVER_ERROR)),
                        Some(host) => host,
                    };

                    let uri = extend_path(uri, &req.path_extension)?;

                    let request = match hyper::Request::builder()
                        .header(http::header::HOST, host)
                        .uri(uri)
                        .body(req.inner.into_body())
                    {
                        Ok(req) => req,
                        Err(err) => {
                            error!("error building downstream response: {err}");
                            return Ok(utils::http_error(StatusCode::INTERNAL_SERVER_ERROR));
                        }
                    };
                    req.inner = request;

                    // TODO: Find better way to share HTTP client
                    match sender.call(req).await {
                        Ok(res) => {
                            let (parts, body) = res.into_parts();
                            let body = match body.collect().await {
                                Ok(bytes) => bytes.to_bytes(),
                                Err(err) => {
                                    error!("error collecting upstream response: {err}");
                                    return Ok(utils::http_error(
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                    ));
                                }
                            };
                            Ok(crate::FullResponse::from_parts(parts, body.into()))
                        }
                        Err(err) => {
                            error!("failed to get response from upstream: {err}");
                            Ok(utils::http_error(StatusCode::BAD_GATEWAY))
                        }
                    }
                }
                _ => Ok(utils::http_error(StatusCode::INTERNAL_SERVER_ERROR)),
            }
        })
    }
}

#[derive(Debug, Default)]
pub enum SourceInner {
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

fn extend_path(uri: &Uri, ext: &str) -> anyhow::Result<Uri> {
    let mut extended = String::new();

    let path = uri.path().trim_end_matches("/").trim_start_matches("/");
    let path_extension = (ext).trim_end_matches("/").trim_start_matches("/");

    extended += &format!("{path}/{path_extension}");

    if let Some(query) = uri.query() {
        extended += &format!("?{query}");
    }

    let p_q = PathAndQuery::from_str(&extended).with_context(|| "failed to build extended path")?;

    Uri::builder()
        .path_and_query(p_q)
        .build()
        .with_context(|| "failed to extend uri path")
}
