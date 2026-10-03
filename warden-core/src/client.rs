use std::{str::FromStr, sync::Arc};

use http::{Response, Uri};
use http_body_util::{BodyExt, Full};
use hyper::service::Service;
use static_assertions::assert_impl_all;

use crate::{PinnedFuture, client::connection::Connection};

mod connection;
mod protocols;

/// HTTP Client managing connections without caching.
///
/// A new client should be created for each individual upstream service
/// as connection state and pending request futures will be driven in one
/// task.
pub struct Client {
    host: Host,
    conns: Vec<Connection>,
}

impl Client {
    pub fn new(host: Host) -> Self {
        Self {
            host,
            conns: vec![],
        }
    }

    pub fn send<Fut>(&self, req: crate::Request) -> Fut
    where
        Fut: Future<Output = crate::IncomingResponse>,
    {
        todo!()
    }
}

pub struct Host {
    addr: String,
}

impl FromStr for Host {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        todo!()
    }
}

trait UpstreamService:
    Service<
        crate::Request,
        Response = crate::FullResponse,
        Error = anyhow::Error,
        Future = PinnedFuture<Result<crate::FullResponse, anyhow::Error>>,
    >
{
}
impl UpstreamService for protocols::Http1Upstream {}
impl UpstreamService for protocols::Http2Upstream {}

pub struct Upstream {
    inner: Arc<dyn UpstreamService + Send + Sync + 'static>,
}

impl std::fmt::Debug for Upstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Opaque Upstream type")
    }
}

impl Upstream {
    pub async fn http1(uri: &Uri) -> anyhow::Result<Self> {
        Ok(Self {
            inner: Arc::new(protocols::Http1Upstream::new(uri).await?),
        })
    }

    pub async fn http2(uri: &Uri) -> anyhow::Result<Self> {
        Ok(Self {
            inner: Arc::new(protocols::Http2Upstream::connect(uri).await?),
        })
    }

    #[cfg(test)]
    pub async fn test() -> anyhow::Result<Self> {
        Ok(Upstream {
            inner: Arc::new(TestUpstream {}),
        })
    }
}

impl Service<crate::Request> for Upstream {
    type Response = crate::FullResponse;
    type Error = anyhow::Error;
    type Future = PinnedFuture<Result<crate::FullResponse, anyhow::Error>>;

    fn call(&self, req: crate::Request) -> Self::Future {
        self.inner.call(req)
    }
}

#[cfg(test)]
struct TestUpstream {}

#[cfg(test)]
impl UpstreamService for TestUpstream {}

#[cfg(test)]
impl Service<crate::Request> for TestUpstream {
    type Response = crate::FullResponse;
    type Error = anyhow::Error;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;

    fn call(&self, _: crate::Request) -> Self::Future {
        Box::pin(async move {
            Ok(crate::FullResponse::new(Full::new(
                hyper::body::Bytes::from("response"),
            )))
        })
    }
}

pub async fn collect_body(
    incoming: crate::IncomingResponse,
) -> anyhow::Result<crate::FullResponse> {
    let (parts, body) = incoming.into_parts();

    let body = Full::new(body.collect().await?.to_bytes());

    Ok(Response::from_parts(parts, body))
}

assert_impl_all!(Upstream: Send);
