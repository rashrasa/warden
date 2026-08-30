pub mod config;
pub mod jwt;
pub mod route;
pub mod tcp;

use anyhow::Context;
use http::{StatusCode, Uri, uri::PathAndQuery};
use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, service::Service};
use log::{error, info};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use static_assertions::assert_impl_all;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tokio::{fs::File, io::AsyncReadExt, net::TcpListener, time::Instant};
use tokio_rustls::TlsAcceptor;

use crate::{
    PinnedFuture,
    core::config::ConfigurationDesc,
    down::ConnectionService,
    services::{AuthService, RouterService, ThrottleService, route::Routes},
    up::Upstream,
    utils::{self, http_error},
};

pub struct Warden {
    host: SocketAddr,

    connection_service: ConnectionService,

    config: Arc<ConfigurationDesc>,

    routes: Arc<Routes>,
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

pub struct RequestService;

impl RequestService {
    pub async fn handle_request(
        config: impl AsRef<ConfigurationDesc>,
        routes: impl AsRef<Routes>,
        request: crate::Request,
    ) -> crate::FullResponse {
        let request = match AuthService::handle_request(&config, request).await {
            Ok(r) => r,
            Err(e) => return e,
        };

        let request = match ThrottleService::handle_request(&config, request).await {
            Ok(r) => r,
            Err(e) => return e,
        };

        match RouterService::route(&config, routes, request).await {
            Ok(res) => res,
            Err(e) => {
                error!("{:#}", e.context("failed to handle request"));
                http_error(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
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
    Http(Uri, Upstream),
    Https(Uri, Upstream),

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

assert_impl_all!(Source: Send);
assert_impl_all!(SourceInner: Send);

pub struct Meter {
    limit: u64,
    window: Duration,

    last: Instant,
    count: u64,
}

impl Meter {
    pub fn new(limit: u64, window: Duration) -> Self {
        Self {
            limit,
            window,
            last: Instant::now(),
            count: 0,
        }
    }

    pub fn tick(&mut self, amt: u64) -> MeterTickResult {
        let now = Instant::now();
        self.count += amt;
        if now - self.last > self.window {
            self.count -= self.limit;
            self.last = now;
        }

        if self.count > self.limit {
            MeterTickResult::Exceeds
        } else {
            MeterTickResult::Within
        }
    }
}

pub enum MeterTickResult {
    Within,
    Exceeds,
}
