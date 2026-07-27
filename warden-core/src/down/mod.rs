use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Context;
use http::StatusCode;
use hyper::server::conn::http2;
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{error, trace};
use tokio::{net::TcpStream, sync::Mutex, time::Instant};
use tokio_rustls::TlsAcceptor;

use crate::{Warden, up::PinnedFuture, utils::http_error};

const MAX_REQUESTS_PER_SECOND: u64 = 1;
const WINDOW: Duration = Duration::new(1, 0);

#[derive(Debug, Clone)]
pub struct Downstream {
    inner: Arc<Mutex<DownstreamInner>>,
}

#[derive(Debug)]
struct DownstreamInner {
    warden: Warden,
    last: Instant,
    window_requests: u64,
}

impl hyper::service::Service<crate::Request> for Downstream {
    type Response = crate::FullResponse;
    type Error = anyhow::Error;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;

    fn call(&self, req: crate::Request) -> Self::Future {
        let cloned = self.clone();
        Box::pin(async move {
            let mut inner = cloned.inner.lock().await;
            if inner.last.elapsed() > WINDOW {
                inner.last = Instant::now();
                inner.window_requests = 0;
            }

            if inner.window_requests > MAX_REQUESTS_PER_SECOND {
                Ok(http_error(StatusCode::TOO_MANY_REQUESTS))
            } else {
                inner.window_requests += 1;

                inner.warden.serve_request(req).await
            }
        })
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
            inner: Arc::new(Mutex::new(DownstreamInner {
                warden,
                last: Instant::now(),
                window_requests: 0,
            })),
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

            if let Err(e) = http2::Builder::new(TokioExecutor::new())
                .serve_connection(io, downstream_task)
                .await
            {
                error!(
                    "{:#}",
                    anyhow::Error::from(e).context("failed to serve request")
                );
            }
        });

        Ok(downstream)
    }
}
