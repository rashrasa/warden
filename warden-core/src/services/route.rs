use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Error};
use http::StatusCode;
use hyper::service::Service;
use static_assertions::assert_impl_all;

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

    fn call(&self, mut req: crate::Request) -> Self::Future {
        let routes = self.routes.clone();
        Box::pin(async move {
            let path = utils::path(&req);
            let path = Path::new(path).with_context(|| "failed to parse path")?;
            if let Some((upstream, excess)) = routes.find_match(&path) {
                req.path_extension = excess.to_vec().join("/");
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
    fn find_match<'a>(&'a self, path: &'a Path) -> Option<(&'a Source, &'a [String])> {
        for (k, v) in self.inner.iter() {
            if let RouteMatch::Match { excess } = k.matches(path) {
                return Some((v, excess));
            }
        }

        None
    }
}

assert_impl_all!(RouterService: Send);

#[cfg(test)]
mod test {
    use http::Uri;

    use crate::{core::SourceInner, up::Upstream};

    use super::*;

    #[test]
    fn basic_matches() {
        let mut routes = Routes {
            inner: HashMap::new(),
        };

        routes.inner.insert(
            Route::new("const/path").unwrap(),
            Source::new(SourceInner::StaticHtml(vec![90])),
        );

        let path = Path::new("const/path").unwrap();

        let (source, excess) = routes.find_match(&path).unwrap();
        match source.inner() {
            SourceInner::StaticHtml(bytes) => {
                assert_eq!(1, bytes.len());
                assert_eq!(90, bytes[0]);
            }
            _ => {
                panic!("SourceInner is not StaticHtml")
            }
        }

        assert!(excess.is_empty())
    }

    #[test]
    fn wildcard_matches() {
        let mut routes = Routes {
            inner: HashMap::new(),
        };
        routes.inner.insert(
            Route::new("const/*").unwrap(),
            Source::new(SourceInner::StaticHtml(vec![91])),
        );
        let path0 = Path::new("const/path").unwrap();
        let path1 = Path::new("const/path1/path2/").unwrap();
        let path3 = Path::new("").unwrap();

        let (source, excess) = routes.find_match(&path0).unwrap();
        match source.inner() {
            SourceInner::StaticHtml(bytes) => {
                assert_eq!(1, bytes.len());
                assert_eq!(91, bytes[0]);
            }
            _ => {
                panic!("SourceInner is not StaticHtml")
            }
        }

        assert_eq!(1, excess.len());
        assert_eq!("path", excess[0]);

        let (source, excess) = routes.find_match(&path1).unwrap();
        match source.inner() {
            SourceInner::StaticHtml(bytes) => {
                assert_eq!(1, bytes.len());
                assert_eq!(91, bytes[0]);
            }
            _ => {
                panic!("SourceInner is not StaticHtml")
            }
        }

        assert_eq!(2, excess.len());
        assert_eq!("path1", excess[0]);
        assert_eq!("path2", excess[1]);

        assert!(routes.find_match(&path3).is_none());
    }

    #[tokio::test]
    async fn mixed_matches() {
        let mut routes = Routes {
            inner: HashMap::new(),
        };
        routes.inner.insert(
            Route::new("/").unwrap(),
            Source::new(SourceInner::StaticHtml(vec![91])),
        );
        routes.inner.insert(
            Route::new("/dyn").unwrap(),
            Source::new(SourceInner::StaticHtml(vec![92])),
        );
        routes.inner.insert(
            Route::new("/test/*").unwrap(),
            Source::new(SourceInner::StaticHtml(vec![93])),
        );
        routes.inner.insert(
            Route::new("/nginx/*").unwrap(),
            Source::new(SourceInner::StaticHtml(vec![94])),
        );
        routes.inner.insert(
            Route::new("/secure/*").unwrap(),
            Source::new(SourceInner::Https(
                Uri::from_static("https://localhost"),
                Upstream::test().await.unwrap(),
            )),
        );
        let path = Path::new("/secure").unwrap();
        let (source, excess) = routes.find_match(&path).unwrap();

        assert!(matches!(source.inner(), SourceInner::Https(..)));
        assert!(excess.is_empty());
    }
}
