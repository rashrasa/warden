use std::sync::Arc;

use http::StatusCode;
use hyper::service::Service;

use crate::{
    PinnedFuture,
    core::config::ConfigurationDesc,
    utils::{http_error, path},
};

pub struct RouterService {
    config: Arc<ConfigurationDesc>,
}

impl RouterService {
    pub fn new(config: Arc<ConfigurationDesc>) -> Self {
        Self { config }
    }
}

impl Clone for RouterService {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
        }
    }
}

impl Service<crate::Request> for RouterService {
    type Response = crate::FullResponse;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;
    type Error = anyhow::Error;

    fn call(&self, req: crate::Request) -> Self::Future {
        let config = self.config.clone();
        Box::pin(async move {
            let path = path(&req);
            if let Some(upstream) = config.handlers.get(path) {
                upstream.call(req).await
            } else {
                Ok(http_error(StatusCode::NOT_FOUND))
            }
        })
    }
}
