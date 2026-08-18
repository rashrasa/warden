use std::time::Duration;

use http::{HeaderMap, HeaderValue, StatusCode};
use tokio::time::Instant;

use crate::{UnwrapLog, core::config::ConfigurationDesc, utils::http_error_with_headers};

const MAX_REQUESTS_PER_SECOND: u64 = 5;
const WINDOW: Duration = Duration::new(1, 0);

#[derive(Debug)]
pub struct ThrottleService;

#[derive(Debug)]
struct Metadata {
    last: Instant,
    window_requests: f64,
}

impl ThrottleService {
    pub async fn handle_request(
        config: impl AsRef<ConfigurationDesc>,
        request: crate::Request,
    ) -> Result<crate::Request, crate::FullResponse> {
        // TODO
        Ok(request)

        // let window_requests;
        // {
        //     let mut meta = Metadata {
        //         last: Instant::now(),
        //         window_requests: 0.0,
        //     };
        //     meta.window_requests += 1.0;
        //     let elapsed = meta.last.elapsed().as_secs_f64();

        //     if elapsed > WINDOW.as_secs_f64() {
        //         meta.last = Instant::now();
        //         meta.window_requests -=
        //             MAX_REQUESTS_PER_SECOND as f64 * (elapsed / WINDOW.as_secs_f64());
        //         meta.window_requests = meta.window_requests.max(0.0);
        //     }
        //     window_requests = meta.window_requests;
        // }

        // if window_requests > MAX_REQUESTS_PER_SECOND as f64 {
        //     let mut headers = HeaderMap::with_capacity(1);

        //     let retry_after = (((window_requests / MAX_REQUESTS_PER_SECOND as f64)
        //         * WINDOW.as_secs_f64()) as i64)
        //         .max(1);
        //     let retry_after = HeaderValue::from_str(&format!("{}", retry_after))
        //         .unwrap_or(HeaderValue::from_static("1"));

        //     headers.insert(hyper::header::RETRY_AFTER, retry_after.clone());
        //     headers.insert(hyper::header::REFRESH, retry_after);

        //     Err(http_error_with_headers(
        //         StatusCode::TOO_MANY_REQUESTS,
        //         headers,
        //     ))
        // } else {
        //     Ok(request)
        // }
    }
}
