use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Context;
use http::{HeaderMap, HeaderValue, StatusCode};
use hyper::server::conn::http2;
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{error, trace};
use tokio::{net::TcpStream, sync::Mutex, time::Instant};
use tokio_rustls::TlsAcceptor;

use crate::{Warden, up::PinnedFuture, utils::http_error_with_headers};

const MAX_REQUESTS_PER_SECOND: u64 = 50;

const WINDOW: Duration = Duration::new(1, 0);

#[derive(Debug, Clone)]
pub struct Downstream {
    inner: Arc<Mutex<DownstreamInner>>,
}

#[derive(Debug)]
struct DownstreamInner {
    warden: Warden,
    last: Instant,
    window_requests: f64,
}

impl hyper::service::Service<crate::Request> for Downstream {
    type Response = crate::FullResponse;
    type Error = anyhow::Error;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;

    fn call(&self, req: crate::Request) -> Self::Future {
        let cloned = self.clone();
        Box::pin(async move {
            let mut inner = cloned.inner.lock().await;
            inner.window_requests += 1.0;
            let elapsed = inner.last.elapsed().as_secs_f64();

            if elapsed > WINDOW.as_secs_f64() {
                inner.last = Instant::now();
                inner.window_requests -=
                    MAX_REQUESTS_PER_SECOND as f64 * (elapsed / WINDOW.as_secs_f64());
            }

            if inner.window_requests > MAX_REQUESTS_PER_SECOND as f64 {
                let mut headers = HeaderMap::with_capacity(1);

                let retry_after = (((inner.window_requests as f64 / MAX_REQUESTS_PER_SECOND as f64)
                    * WINDOW.as_secs_f64()) as i64)
                    .max(1);
                let retry_after = HeaderValue::from_str(&format!("{}", retry_after))
                    .unwrap_or(HeaderValue::from_static("1"));

                headers.insert(hyper::header::RETRY_AFTER, retry_after);
                Ok(http_error_with_headers(
                    StatusCode::TOO_MANY_REQUESTS,
                    headers,
                ))
            } else {
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
                window_requests: 0.0,
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
