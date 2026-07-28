use hyper::client::conn::http2::*;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use std::time::Duration;
use tokio::net::TcpStream;
use tower::Service;

use crate::up::PinnedFuture;

async fn make_http2_connection(
    io: TokioIo<TcpStream>,
) -> Result<
    (
        SendRequest<hyper::body::Incoming>,
        Connection<TokioIo<TcpStream>, hyper::body::Incoming, TokioExecutor>,
    ),
    hyper::Error,
> {
    Builder::new(TokioExecutor::new())
        .keep_alive_while_idle(true)
        .keep_alive_interval(Duration::from_millis(5000))
        .timer(TokioTimer::new())
        .handshake(io)
        .await
}

pub struct Http2Upstream {
    inner: SendRequest<hyper::body::Incoming>,
}

impl Http2Upstream {
    pub fn connect(uri: hyper::Uri) -> Self {
        todo!()
    }
}

impl Service<crate::Request> for Http2Upstream {
    type Response = crate::IncomingResponse;
    type Error = hyper::Error;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;

    fn call(&mut self, req: crate::Request) -> Self::Future {
        Box::pin(self.inner.send_request(req.inner))
    }

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
}
