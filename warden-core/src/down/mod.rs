use std::sync::Arc;

use anyhow::Context;
use hyper::{server::conn::http2, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{debug, error};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::{
    core::{RequestService, config::ConfigurationDesc},
    services::route::Routes,
};

pub struct ConnectionService {
    tcp: TcpListener,
    tls: TlsAcceptor,
    config: Arc<ConfigurationDesc>,
    routes: Arc<Routes>,
}

impl ConnectionService {
    pub fn new(
        tcp: TcpListener,
        tls: TlsAcceptor,
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

        tokio::spawn(async move {
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
            let io = TokioIo::new(tls_stream);

            let service = service_fn(|req: crate::RawRequest| {
                let config = Arc::clone(&config);
                let routes = Arc::clone(&routes);
                async move {
                    Ok::<_, anyhow::Error>(
                        RequestService::handle_request(
                            config,
                            routes,
                            crate::Request {
                                source: addr,
                                inner: req,
                                path_extension: String::new(),
                            },
                        )
                        .await,
                    )
                }
            });

            if let Err(e) = http2::Builder::new(TokioExecutor::new())
                .serve_connection(io, service)
                .await
            {
                error!("{:#}", anyhow::Error::from(e).context("connection failed"));
            }
        });
        Ok(())
    }
}
