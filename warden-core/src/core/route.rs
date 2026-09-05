/*
    Note: Likely, a separate, optimized data structure will be used for
    storing routes and matching paths against them and current APIs will be replaced.
*/

use std::collections::HashMap;

/// Describes a route.
///
/// Parts are separated by a backslash (/) and parts can either be
/// a valid URI part or a wildcard.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Route {
    parts: Box<[RoutePart]>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) enum RoutePart {
    Wildcard,
    Part(Box<str>),
}

impl From<&str> for RoutePart {
    fn from(value: &str) -> Self {
        Self::Part(value.into())
    }
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
            parts.push(RoutePart::Part(part.into()));
        }
        Ok(Self {
            parts: parts.into(),
        })
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
                            if *p != **r_p {
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
    /// Only contains valid URI path components
    /// without any delimiters (e.g. `/`, `?`, `#`, etc.) or
    /// unescaped reserved URI characters.
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
/// Efficiently maps [`Route`]s to a generic value. Mainly
/// used to store HTTP route handlers.
///
/// [`FALLBACK`] specifies whether to fallback to a previous wildcard branch
/// if a non-wildcard branch only partially matched when looking up values
/// by path.
pub struct Routes<T> {
    root: Node<T>,
}

impl<T> Routes<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts value at the provided route. Returns the old value if it existed.
    pub fn insert(&mut self, route: Route, value: T) -> Option<T> {
        let mut current = &mut self.root;
        for part in route.parts {
            current = current.routes.entry(part).or_default();
        }

        current.handler.replace(value)
    }

    /// Gets the value at the provided route if it exists.
    pub fn get<'a>(&'a self, route: &Route) -> Option<&'a T> {
        let mut current = &self.root;
        for part in &route.parts {
            current = current.routes.get(part)?;
        }

        current.handler.as_ref()
    }
}

impl<T> Router<T> for Routes<T> {
    fn match_path<'a>(&'a self, path: &Path) -> Option<&'a T> {
        let mut current = &self.root;
        for s in &path.parts {
            let part = s.as_str().into();
            match current.routes.get(&part) {
                Some(v) => {
                    current = v;
                }
                None => {
                    current = current.routes.get(&RoutePart::Wildcard)?;
                }
            }
        }

        current.handler.as_ref()
    }
}

impl<T> Default for Routes<T> {
    fn default() -> Self {
        Self {
            root: Default::default(),
        }
    }
}

pub trait Router<T> {
    /// Attempts to match the path against registered routes.
    ///
    /// Any given path can match, at most, one route.
    fn match_path<'a>(&'a self, path: &Path) -> Option<&'a T>;
}

// TODO: Evaluate whether a secure hasher is necessary. Routes are set by a
//       trusted config file and may not require HashDoS resistance.
pub struct Node<T> {
    handler: Option<T>,
    routes: HashMap<RoutePart, Node<T>>,
}

impl<T> Default for Node<T> {
    fn default() -> Self {
        Self {
            handler: None,
            routes: HashMap::new(),
        }
    }
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
