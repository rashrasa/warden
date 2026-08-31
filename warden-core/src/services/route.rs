use std::collections::HashMap;

use anyhow::{Context, Error};
use http::StatusCode;
use hyper::service::Service;
use static_assertions::assert_impl_all;

use crate::{
    core::{
        Source,
        config::ConfigurationDesc,
        route::{Path, Route, RouteMatch},
    },
    utils,
};

#[derive(Debug)]
pub struct RouterService;

impl RouterService {
    pub fn parse_routes(config: impl AsRef<ConfigurationDesc>) -> (Routes, Vec<Error>) {
        let config = config.as_ref();
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
        (Routes { inner: routes }, errors)
    }

    pub async fn route(
        config: impl AsRef<ConfigurationDesc>,
        routes: impl AsRef<Routes>,
        mut request: crate::Request,
    ) -> anyhow::Result<crate::FullResponse> {
        let config = config.as_ref();
        let routes = routes.as_ref();
        let path: &str = utils::path(&request);

        let path = Path::new(path).with_context(|| "failed to parse path")?;
        if let Some((upstream, excess)) = routes.find_match(&path) {
            request.path_extension = excess.to_vec().join("/");
            upstream.call(request).await
        } else {
            Ok(utils::http_error(StatusCode::NOT_FOUND))
        }
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
            Source::new(SourceInner::StaticHtml(String::from("a"))),
        );

        let path = Path::new("const/path").unwrap();

        let (source, excess) = routes.find_match(&path).unwrap();
        match source.inner() {
            SourceInner::StaticHtml(text) => {
                assert_eq!(1, text.len());
                assert_eq!('a', text.chars().next().unwrap());
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
            Source::new(SourceInner::StaticHtml(String::from("b"))),
        );
        let path0 = Path::new("const/path").unwrap();
        let path1 = Path::new("const/path1/path2/").unwrap();
        let path3 = Path::new("").unwrap();

        let (source, excess) = routes.find_match(&path0).unwrap();
        match source.inner() {
            SourceInner::StaticHtml(text) => {
                assert_eq!(1, text.len());
                assert_eq!('b', text.chars().next().unwrap());
            }
            _ => {
                panic!("SourceInner is not StaticHtml")
            }
        }

        assert_eq!(1, excess.len());
        assert_eq!("path", excess[0]);

        let (source, excess) = routes.find_match(&path1).unwrap();
        match source.inner() {
            SourceInner::StaticHtml(text) => {
                assert_eq!(1, text.len());
                assert_eq!('b', text.chars().next().unwrap());
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
            Source::new(SourceInner::StaticHtml(String::from("b"))),
        );
        routes.inner.insert(
            Route::new("/dyn").unwrap(),
            Source::new(SourceInner::StaticHtml(String::from("c"))),
        );
        routes.inner.insert(
            Route::new("/test/*").unwrap(),
            Source::new(SourceInner::StaticHtml(String::from("d"))),
        );
        routes.inner.insert(
            Route::new("/nginx/*").unwrap(),
            Source::new(SourceInner::StaticHtml(String::from("e"))),
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
