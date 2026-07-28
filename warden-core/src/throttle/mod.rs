use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use http::{HeaderMap, HeaderValue, StatusCode};
use hyper::service::Service;
use tokio::{sync::Mutex, time::Instant};

use crate::{up::PinnedFuture, utils::http_error_with_headers};

const MAX_REQUESTS_PER_SECOND: u64 = 5;
const WINDOW: Duration = Duration::new(1, 0);

#[derive(Debug)]
pub struct ThrottleService<S> {
    inner: Arc<ThrottleServiceInner<S>>,
}

impl<S> Clone for ThrottleService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[derive(Debug)]
struct ThrottleServiceInner<S> {
    inner: S,
    clients: Mutex<HashMap<SocketAddr, Metadata>>,
}

#[derive(Debug)]
struct Metadata {
    last: Instant,
    window_requests: f64,
}

impl<S> ThrottleService<S> {
    pub fn new(inner: S) -> Self {
        let inner = Arc::new(ThrottleServiceInner {
            inner,
            clients: Mutex::new(HashMap::new()),
        });
        Self { inner }
    }
}

impl<S> Service<crate::Request> for ThrottleService<S>
where
    S: Service<
            crate::Request,
            Response = crate::FullResponse,
            Error = anyhow::Error,
            Future = PinnedFuture<Result<crate::FullResponse, anyhow::Error>>,
        > + Send
        + Sync
        + 'static,
{
    type Response = crate::FullResponse;
    type Error = anyhow::Error;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;

    fn call(&self, req: crate::Request) -> Self::Future {
        let cloned = self.clone();
        Box::pin(async move {
            let mut map = cloned.inner.clients.lock().await;
            let meta = map.entry(req.source).or_insert(Metadata {
                last: Instant::now(),
                window_requests: 0.0,
            });
            meta.window_requests += 1.0;
            let elapsed = meta.last.elapsed().as_secs_f64();

            if elapsed > WINDOW.as_secs_f64() {
                meta.last = Instant::now();
                meta.window_requests -=
                    MAX_REQUESTS_PER_SECOND as f64 * (elapsed / WINDOW.as_secs_f64());
                meta.window_requests = meta.window_requests.max(0.0);
            }

            if meta.window_requests > MAX_REQUESTS_PER_SECOND as f64 {
                let mut headers = HeaderMap::with_capacity(1);

                let retry_after = (((meta.window_requests as f64 / MAX_REQUESTS_PER_SECOND as f64)
                    * WINDOW.as_secs_f64()) as i64)
                    .max(1);
                let retry_after = HeaderValue::from_str(&format!("{}", retry_after))
                    .unwrap_or(HeaderValue::from_static("1"));

                headers.insert(hyper::header::RETRY_AFTER, retry_after.clone());
                headers.insert(hyper::header::REFRESH, retry_after);

                Ok(http_error_with_headers(
                    StatusCode::TOO_MANY_REQUESTS,
                    headers,
                ))
            } else {
                cloned.inner.inner.call(req).await
            }
        })
    }
}
