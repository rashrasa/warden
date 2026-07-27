/*
    Note: Likely, a separate, optimized data structure will be used for
    storing routes and matching paths against them and current APIs will be replaced.
*/

#[derive(Debug)]
pub struct Route {
    parts: Vec<RoutePart>,
}

#[derive(Debug)]
enum RoutePart {
    Wildcard,
    Part(String),
}

impl Route {
    /// Creates a new route. Route ignores parts after the first wildcard(*) symbol.
    pub fn new(mut path: &str) -> anyhow::Result<Self> {
        let mut parts = vec![];
        if let Some(p) = path.strip_suffix("/") {
            path = p;
        }
        if let Some(p) = path.strip_prefix("/") {
            path = p;
        }
        for part in path.split("/") {
            let part = part.trim();
            if part == "*" {
                parts.push(RoutePart::Wildcard);
                break;
            } else {
                parts.push(RoutePart::Part(part.to_owned()));
            }
        }
        Ok(Self { parts })
    }

    pub fn matches<'a>(&self, path: &'a Path) -> RouteMatch<'a> {
        let mut route_parts = self.parts.iter().enumerate();
        let mut path_parts = path.parts.iter();
        loop {
            // Inv: matched so far
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

#[cfg(test)]
mod test {
    use super::*;

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
}
