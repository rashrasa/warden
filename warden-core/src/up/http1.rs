use std::sync::Arc;

use hyper::client::conn::http1::*;
use hyper_util::rt::TokioIo;
use log::error;
use tokio::{net::TcpStream, sync::Mutex};
use tower::Service;

use crate::up::PinnedFuture;

async fn make_http1_connection(
    io: TokioIo<TcpStream>,
) -> Result<
    (
        SendRequest<hyper::body::Incoming>,
        Connection<TokioIo<TcpStream>, hyper::body::Incoming>,
    ),
    hyper::Error,
> {
    Builder::new().handshake(io).await
}

#[derive(Debug, Clone)]
pub struct Http1Upstream {
    inner: Arc<Mutex<Http1UpstreamInner>>,
}

impl Http1Upstream {
    pub async fn connect(uri: &hyper::Uri) -> anyhow::Result<Self> {
        let host = uri.host().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                anyhow::anyhow!("invalid uri: {uri}"),
            )
        })?;
        let address = format!("{host}:80");
        let stream = TcpStream::connect(address).await?;
        let io = TokioIo::new(stream);
        let (sender, conn) = make_http1_connection(io)
            .await
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionRefused, e))?;

        tokio::spawn(async move {
            if let Err(err) = conn.await {
                error!("connection failed: {err:?}");
            }
        });

        Ok(Self {
            inner: Arc::new(Mutex::new(Http1UpstreamInner { sender })),
        })
    }
}

#[derive(Debug)]
struct Http1UpstreamInner {
    sender: SendRequest<hyper::body::Incoming>,
}

impl Service<crate::Request> for Http1UpstreamInner {
    type Response = crate::IncomingResponse;
    type Error = hyper::Error;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;

    fn call(&mut self, req: crate::Request) -> Self::Future {
        Box::pin(self.sender.send_request(req))
    }

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
}

impl Http1Upstream {
    pub fn call(
        &self,
        req: crate::Request,
    ) -> PinnedFuture<Result<crate::IncomingResponse, hyper::Error>> {
        let inner = self.inner.clone();
        Box::pin(async move {
            let mut guard = inner.lock().await;
            guard.call(req).await
        })
    }

    fn poll_ready(
        &self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), hyper::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
}
