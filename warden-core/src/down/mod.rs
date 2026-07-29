use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use hyper::{
    server::conn::http2,
    service::{Service, service_fn},
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{error, trace};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

use crate::{PinnedFuture, Warden};

#[derive(Debug, Clone)]
pub struct Downstream {
    inner: Arc<DownstreamInner>,
}

#[derive(Debug)]
struct DownstreamInner {
    warden: Warden,
}

impl hyper::service::Service<crate::Request> for Downstream {
    type Response = crate::FullResponse;
    type Error = anyhow::Error;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;

    fn call(&self, req: crate::Request) -> Self::Future {
        let cloned = self.clone();
        Box::pin(async move { cloned.inner.warden.serve_request(req).await })
    }
}

impl Downstream {
    pub async fn handle_new_connection(
        warden: Warden,
        acceptor: TlsAcceptor,
        conn: std::io::Result<(TcpStream, SocketAddr)>,
    ) -> anyhow::Result<Self> {
        let (stream, addr) = conn.with_context(|| "failed to open connection")?;
        trace!("new connection: {}", addr);
        let downstream = Downstream {
            inner: Arc::new(DownstreamInner { warden }),
        };
        let downstream_task = downstream.clone();
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

            let service = service_fn(|req: crate::RawRequest| {
                let downstream = downstream_task.clone();

                downstream.call(crate::Request {
                    source: addr,
                    inner: req,
                })
            });

            if let Err(e) = http2::Builder::new(TokioExecutor::new())
                .serve_connection(io, service)
                .await
            {
                error!("{:#}", anyhow::Error::from(e).context("connection failed"));
            }
        });

        Ok(downstream)
    }
}
