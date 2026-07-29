use std::sync::Arc;

use http::StatusCode;
use hyper::service::Service;
use log::error;

use crate::{
    PinnedFuture,
    core::config::{ConfigurationDesc, FilterDesc},
    utils::{http_error, path},
};

const USER_HEADER: &str = "x-warden-user";

#[derive(Debug)]
pub struct AuthService<S> {
    inner: S,
    config: Arc<ConfigurationDesc>,
}

impl<S: Clone> Clone for AuthService<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            config: Arc::clone(&self.config),
        }
    }
}

impl<S> AuthService<S> {
    pub fn new(config: Arc<ConfigurationDesc>, inner: S) -> Self {
        Self { inner, config }
    }
}

#[derive(Default)]
pub enum Authorization {
    #[default]
    Allowed,

    Blocked,
}

impl<S> Service<crate::Request> for AuthService<S>
where
    S: Service<
            crate::Request,
            Response = crate::FullResponse,
            Error = anyhow::Error,
            Future = PinnedFuture<Result<crate::FullResponse, anyhow::Error>>,
        >,
{
    type Response = crate::FullResponse;
    type Error = anyhow::Error;
    type Future = PinnedFuture<Result<Self::Response, Self::Error>>;

    fn call(&self, req: crate::Request) -> Self::Future {
        match self.verify_request(&req) {
            Ok(a) => match a {
                Authorization::Allowed => Box::pin(self.inner.call(req)),
                Authorization::Blocked => {
                    Box::pin(async move { Ok(http_error(StatusCode::UNAUTHORIZED)) })
                }
            },
            Err(e) => {
                error!("{}", e.context("error verifying request"));
                Box::pin(async move { Ok(http_error(StatusCode::INTERNAL_SERVER_ERROR)) })
            }
        }
    }
}

impl<S> AuthService<S>
where
    S: Service<crate::Request>,
{
    pub fn parse_role(&self, request: &crate::Request) -> Option<String> {
        match request.inner.headers().get(USER_HEADER) {
            Some(v) => String::from_utf8(v.as_bytes().to_vec()).ok(),
            None => None,
        }
    }

    pub fn verify_request(&self, request: &crate::Request) -> anyhow::Result<Authorization> {
        let path = path(request);

        if let Some(h) = self.config.handlers.get(path) {
            match &h.permission.filter {
                FilterDesc::Allow => {
                    if let Some(r) = self.parse_role(request) {
                        if h.permission.roles.contains(&r) {
                            return Ok(Authorization::Allowed);
                        } else {
                            return Ok(Authorization::Blocked);
                        }
                    }
                }
                FilterDesc::Block => {
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
