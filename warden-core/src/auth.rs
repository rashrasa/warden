use std::sync::Arc;

use hyper::{Request, body::Incoming};

use crate::{
    core::config::{Configuration, Filter},
    utils::path,
};

const USER_HEADER: &str = "x-warden-user";

pub struct AuthProvider {
    pub config: Arc<Configuration>,
}

#[derive(Default)]
pub enum Authorization {
    #[default]
    Allowed,

    Blocked,
}

impl AuthProvider {
    pub fn parse_role(&self, request: &Request<Incoming>) -> Option<String> {
        match request.headers().get(USER_HEADER) {
            Some(v) => String::from_utf8(v.as_bytes().to_vec()).ok(),
            None => None,
        }
    }

    pub fn verify_request(&self, request: &Request<Incoming>) -> anyhow::Result<Authorization> {
        let path = path(request);

        if let Some(h) = self.config.handlers.get(path) {
            match &h.permission.filter {
                Filter::Allow => {
                    if let Some(r) = self.parse_role(request) {
                        if h.permission.roles.contains(&r) {
                            return Ok(Authorization::Allowed);
                        } else {
                            return Ok(Authorization::Blocked);
                        }
                    }
                }
                Filter::Block => {
                    if let Some(r) = self.parse_role(request) {
                        if h.permission.roles.contains(&r) {
                            return Ok(Authorization::Blocked);
                        } else {
                            return Ok(Authorization::Allowed);
                        }
                    } else {
                        return Ok(Authorization::Allowed);
                    }
                }
            }
        }

        Ok(Authorization::default())
    }
}
