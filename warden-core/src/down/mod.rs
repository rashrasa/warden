use std::sync::Arc;

use anyhow::Context;
use hyper::{server::conn::http2, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{debug, error};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
};
use tokio_rustls::TlsAcceptor;

use crate::{
    PinnedFuture,
    core::{RequestService, config::ConfigurationDesc, tcp::AsyncRateLimiter},
    services::route::Routes,
};

trait AsyncIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncIo for T {}

type PinnedFutureFactory<Arg, Ret> = Box<dyn Fn(Arg) -> PinnedFuture<Ret> + Send + 'static>;

pub struct ConnectionService {
    tcp: TcpListener,
    tls: Option<TlsAcceptor>,
    config: Arc<ConfigurationDesc>,
    routes: Arc<Routes>,
}

impl ConnectionService {
    pub fn new(
        tcp: TcpListener,
        tls: Option<TlsAcceptor>,
        config: Arc<ConfigurationDesc>,
        routes: Arc<Routes>,
    ) -> Self {
        Self {
            tcp,
            tls,
            config,
            routes,
        }
    }

    pub async fn serve_next_connection(&mut self) -> anyhow::Result<()> {
        let (stream, addr) = self
            .tcp
            .accept()
            .await
            .with_context(|| "failed to create tcp connection")?;

        debug!("new connection: {}", addr);
        let tls = self.tls.clone();
        let config = Arc::clone(&self.config);
        let routes = Arc::clone(&self.routes);

        let stream: Box<dyn AsyncIo> = match &config.global.throttle {
            Some(thr) => Box::new(AsyncRateLimiter::new(stream, thr.bandwidth_limit)),
            None => Box::new(stream),
        };

        // Connection Task
        //
        // Executes all requests from a single source.
        tokio::spawn(async move {
            let io: TokioIo<Box<dyn AsyncIo>> = match tls {
                Some(tls) => {
                    let tls_stream = match tls
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

                    TokioIo::new(Box::new(tls_stream))
                }
                None => TokioIo::new(Box::new(stream)),
            };
            let config_fn = Arc::clone(&config);
            let routes_fn = Arc::clone(&routes);

            let clsr: PinnedFutureFactory<crate::RawRequest, anyhow::Result<crate::FullResponse>> = {
                let config = config_fn;
                let routes = routes_fn;
                Box::new(move |req: crate::RawRequest| {
                    let config = Arc::clone(&config);
                    let routes = Arc::clone(&routes);
                    Box::pin({
                        async move {
                            Ok(RequestService::handle_request(
                                config,
                                routes,
                                crate::Request {
                                    inner: req,
                                    path_extension: String::new(),
                                },
                            )
                            .await)
                        }
                    })
                })
            };

            let service = service_fn(clsr);

            let mut builder = http2::Builder::new(TokioExecutor::new());

            if let Some(size) = config.global.header_size_max {
                builder.max_header_list_size(size);
            } else {
                builder.max_header_list_size(crate::DEFAULT_HEADER_SIZE_MAX);
            }

            if let Some(n) = config.global.connection_concurrent_requests_max {
                builder.max_concurrent_streams(Some(n));
            } else {
                builder.max_concurrent_streams(Some(
                    crate::DEFAULT_CONNECTION_CONCURRENT_REQUESTS_MAX,
                ));
            }

            if let Err(e) = builder.serve_connection(io, service).await {
                error!("{:#}", anyhow::Error::from(e).context("connection failed"));
            }
        });
        Ok(())
    }
}
