/*
    Note: Likely, a separate, optimized data structure will be used for
    storing routes and matching paths against them and current APIs will be replaced.
*/

use std::collections::HashMap;

/// Route specifier which allows wildcards and valid URI characters
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Route {
    parts: Vec<RoutePart>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum RoutePart {
    Wildcard,
    Part(String),
}

impl Route {
    /// Creates a new route.
    pub fn new(mut path: &str) -> anyhow::Result<Self> {
        let mut parts = vec![];
        if let Some(p) = path.strip_suffix("/") {
            path = p;
        }
        if let Some(p) = path.strip_prefix("/") {
            path = p;
        }
        // TODO: Only allow valid URI characters.
        for part in path.split("/") {
            let part = part.trim();
            parts.push(RoutePart::Part(part.to_owned()));
        }
        Ok(Self { parts })
    }

    pub fn matches<'a>(&self, path: &'a Path) -> RouteMatch<'a> {
        let mut route_parts = self.parts.iter().enumerate();
        let mut path_parts = path.parts.iter();
        loop {
            // Invariant: matched so far
            let route_part = route_parts.next();
            let path_part = path_parts.next();

            match path_part {
                Some(p) => match route_part {
                    Some((i, r)) => match r {
                        RoutePart::Wildcard => {
                            return RouteMatch::Match {
                                excess: &path.parts[i..],
                            };
                        }
                        RoutePart::Part(r_p) => {
                            if p != r_p {
                                return RouteMatch::NotMatch;
                            }
                        }
                    },
                    None => {
                        return RouteMatch::NotMatch;
                    }
                },
                None => match route_part {
                    Some((.., r)) => match r {
                        RoutePart::Wildcard => {
                            return RouteMatch::Match { excess: &[] };
                        }
                        RoutePart::Part(_) => {
                            return RouteMatch::NotMatch;
                        }
                    },
                    None => {
                        return RouteMatch::Match { excess: &[] };
                    }
                },
            }
        }
    }
}

#[derive(Debug)]
pub enum RouteMatch<'a> {
    NotMatch,
    Match { excess: &'a [String] },
}

/// Specific path type which does not allow wildcards.
#[derive(Debug)]
pub struct Path {
    parts: Vec<String>,
}

impl Path {
    pub fn new(mut path: &str) -> anyhow::Result<Self> {
        if let Some(p) = path.strip_suffix("/") {
            path = p;
        }
        if let Some(p) = path.strip_prefix("/") {
            path = p;
        }
        let parts = path.split("/").map(|s| s.to_owned()).collect();
        Ok(Self { parts })
    }
}

// Invariant: No given [`Path`] value matches more than one route.
/// Generic [`Route`] container for storing values against URI routes. Mainly
/// used to store HTTP route handlers.
pub struct Routes<T> {
    root: Node<T>,
}

impl<T> Routes<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts value at the provided route. Returns the old value if it existed.
    pub fn insert(&mut self, route: Route, value: T) -> Option<T> {
        todo!()
    }

    /// Gets the value at the provided route if it exists.
    pub fn get<'a>(&'a self, route: &Route) -> Option<&'a T> {
        todo!()
    }

    /// Attempts to match the path against registered routes.
    ///
    /// Any given path can match, at most, one route.
    pub fn match_path<'a>(&'a self, path: &Path) -> Option<&'a T> {
        todo!()
    }
}

impl<T> Default for Routes<T> {
    fn default() -> Self {
        Self {
            root: Node {
                handler: None,
                routes: HashMap::new(),
            },
        }
    }
}

// TODO: Evaluate whether a secure hasher is necessary. Routes are set by a
//       trusted config file and may not require HashDoS resistance.
pub struct Node<T> {
    handler: Option<T>,
    routes: HashMap<RoutePart, Node<T>>,
}

#[cfg(test)]
mod test {
    use super::*;

    use http::Uri;

    use crate::{
        core::{Source, SourceInner},
        up::Upstream,
    };

    #[test]
    fn matches_root() {
        let r1 = Route::new("/").unwrap();
        let r2 = Route::new("").unwrap();

        let p1 = Path::new("/").unwrap();
        let p2 = Path::new("").unwrap();

        assert!(matches!(r1.matches(&p1), RouteMatch::Match { .. }));
        assert!(matches!(r1.matches(&p2), RouteMatch::Match { .. }));

        assert!(matches!(r2.matches(&p1), RouteMatch::Match { .. }));
        assert!(matches!(r2.matches(&p2), RouteMatch::Match { .. }));
    }

    #[test]
    fn matches_wildcard() {
        let r1 = Route::new("/auth/v1/*").unwrap();
        let r2 = Route::new("/analytics/v2/*/test").unwrap();
        let r3 = Route::new("/analytics/v2/test/*").unwrap();

        let p1 = Path::new("/auth/v1/health/").unwrap();
        let p2 = Path::new("analytics/v2/auth/user_abcd").unwrap();

        assert!(matches!(r1.matches(&p1), RouteMatch::Match { .. }));
        assert!(matches!(r1.matches(&p2), RouteMatch::NotMatch));

        assert!(matches!(r2.matches(&p1), RouteMatch::NotMatch));
        assert!(matches!(r2.matches(&p2), RouteMatch::Match { .. }));

        assert!(matches!(r3.matches(&p1), RouteMatch::NotMatch));
        assert!(matches!(r3.matches(&p2), RouteMatch::NotMatch));
    }

    #[test]
    fn basic_matches() {
        let mut routes = Routes::default();

        routes.insert(
            Route::new("const/path").unwrap(),
            Source::new(SourceInner::StaticHtml(String::from("a"))),
        );

        let path = Path::new("const/path").unwrap();

        let source = routes.match_path(&path).unwrap();
        match source.inner() {
            SourceInner::StaticHtml(text) => {
                assert_eq!(1, text.len());
                assert_eq!('a', text.chars().next().unwrap());
            }
            _ => {
                panic!("SourceInner is not StaticHtml")
            }
        }
    }

    #[test]
    fn wildcard_matches() {
        let mut routes = Routes::default();
        routes.insert(
            Route::new("const/*").unwrap(),
            Source::new(SourceInner::StaticHtml(String::from("b"))),
        );
        let path0 = Path::new("const/path").unwrap();
        let path1 = Path::new("const/path1/path2/").unwrap();
        let path3 = Path::new("").unwrap();

        let source = routes.match_path(&path0).unwrap();
        match source.inner() {
            SourceInner::StaticHtml(text) => {
                assert_eq!(1, text.len());
                assert_eq!('b', text.chars().next().unwrap());
            }
            _ => {
                panic!("SourceInner is not StaticHtml")
            }
        }

        let source = routes.match_path(&path1).unwrap();
        match source.inner() {
            SourceInner::StaticHtml(text) => {
                assert_eq!(1, text.len());
                assert_eq!('b', text.chars().next().unwrap());
            }
            _ => {
                panic!("SourceInner is not StaticHtml")
            }
        }

        assert!(routes.match_path(&path3).is_none());
    }

    #[tokio::test]
    async fn mixed_matches() {
        let mut routes = Routes::default();
        routes.insert(
            Route::new("/").unwrap(),
            Source::new(SourceInner::StaticHtml(String::from("b"))),
        );
        routes.insert(
            Route::new("/dyn").unwrap(),
            Source::new(SourceInner::StaticHtml(String::from("c"))),
        );
        routes.insert(
            Route::new("/test/*").unwrap(),
            Source::new(SourceInner::StaticHtml(String::from("d"))),
        );
        routes.insert(
            Route::new("/nginx/*").unwrap(),
            Source::new(SourceInner::StaticHtml(String::from("e"))),
        );
        routes.insert(
            Route::new("/secure/*").unwrap(),
            Source::new(SourceInner::Https(
                Uri::from_static("https://localhost"),
                Upstream::test().await.unwrap(),
            )),
        );
        let path = Path::new("/secure").unwrap();
        let source = routes.match_path(&path).unwrap();

        assert!(matches!(source.inner(), SourceInner::Https(..)));
    }
}
