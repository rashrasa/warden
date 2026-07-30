use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Error};
use http::StatusCode;
use hyper::service::Service;

use crate::{
    PinnedFuture,
    core::{
        Source,
        config::ConfigurationDesc,
        route::{Path, Route, RouteMatch},
    },
    utils,
};

#[derive(Debug)]
pub struct RouterService {
    routes: Arc<Routes>,
}

impl RouterService {
    pub fn new(config: Arc<ConfigurationDesc>) -> (Self, Vec<Error>) {
        let mut routes = HashMap::new();
        let mut errors = vec![];

        for (path, desc) in config.handlers.iter() {
            match Route::new(path) {
                Ok(r) => {
                    routes.insert(r, desc.source.clone());
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        (
            Self {
                routes: Arc::new(Routes { inner: routes }),
            },
            errors,
        )
    }
}

impl Clone for RouterService {
    fn clone(&self) -> Self {
        Self {
            routes: Arc::clone(&self.routes),
        }
    }
}

impl Service<crate::Request> for RouterService {
    type Response = crate::FullResponse;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;
    type Error = anyhow::Error;

    fn call(&self, req: crate::Request) -> Self::Future {
        let routes = self.routes.clone();
        Box::pin(async move {
            let path = utils::path(&req);
            let path = Path::new(path).with_context(|| "failed to parse path")?;
            if let Some(upstream) = routes.find_match(&path) {
                upstream.call(req).await
            } else {
                Ok(utils::http_error(StatusCode::NOT_FOUND))
            }
        })
    }
}

#[derive(Debug)]
pub struct Routes {
    inner: HashMap<Route, Source>,
}

impl Routes {
    fn find_match(&self, path: &Path) -> Option<&Source> {
        for (k, v) in self.inner.iter() {
            if let RouteMatch::Match { .. } = k.matches(path) {
                return Some(v);
            }
        }

        None
    }
}
