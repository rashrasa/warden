use std::collections::HashMap;

use anyhow::{Context, Error};
use http::StatusCode;
use hyper::service::Service;
use static_assertions::assert_impl_all;

use crate::{
    core::{
        Source,
        config::ConfigurationDesc,
        route::{Path, Route, RouteMatch, Routes},
    },
    utils,
};

#[derive(Debug)]
pub struct RouterService;

impl RouterService {
    pub fn parse_routes(config: impl AsRef<ConfigurationDesc>) -> (Routes<Source>, Vec<Error>) {
        let config = config.as_ref();
        let mut errors = vec![];
        let mut routes = Routes::new();

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
        (routes, errors)
    }

    pub async fn route(
        config: impl AsRef<ConfigurationDesc>,
        routes: impl AsRef<Routes<Source>>,
        request: crate::Request,
    ) -> anyhow::Result<crate::FullResponse> {
        let config = config.as_ref();
        let routes = routes.as_ref();
        let path: &str = utils::path(&request);

        let path = Path::new(path).with_context(|| "failed to parse path")?;
        if let Some(source) = routes.match_path(&path) {
            source.call(request).await
        } else {
            Ok(utils::http_error(StatusCode::NOT_FOUND))
        }
    }
}

assert_impl_all!(RouterService: Send);
