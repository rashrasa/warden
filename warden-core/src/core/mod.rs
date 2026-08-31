pub mod config;
pub mod jwt;
pub mod route;
pub mod tcp;

use anyhow::Context;
use http::{StatusCode, Uri, uri::PathAndQuery};
use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, service::Service};
use log::error;
use static_assertions::assert_impl_all;
use std::{path::PathBuf, str::FromStr, sync::Arc, time::Duration};
use tokio::{fs::File, io::AsyncReadExt, time::Instant};

use crate::{
    PinnedFuture,
    up::Upstream,
    utils::{self},
};

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
    StaticHtml(String),

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
