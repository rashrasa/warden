use std::sync::Arc;

use anyhow::Context;
use http::StatusCode;
use hyper::service::Service;
use log::{error, warn};
use static_assertions::assert_impl_all;

use crate::{
    PinnedFuture,
    core::{
        config::{ConfigurationDesc, FilterDesc},
        jwt::verify_jwt,
    },
    utils::{http_error, path},
};

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
    pub fn parse_role(&self, request: &crate::Request) -> anyhow::Result<String> {
        match request.inner.headers().get(hyper::header::AUTHORIZATION) {
            Some(v) => verify_jwt(&v.as_bytes()[7..])
                .map(|v| v.role)
                .with_context(|| format!("failed to verify jwt {v:?}")),
            None => Err(anyhow::Error::msg("no auth header")),
        }
    }

    pub fn verify_request(&self, request: &crate::Request) -> anyhow::Result<Authorization> {
        let path = path(request);

        if let Some(h) = self.config.handlers.get(path) {
            match &h.permission.filter {
                FilterDesc::Allow => match self
                    .parse_role(request)
                    .with_context(|| "failed to parse role for allow filter")
                {
                    Ok(r) => {
                        if h.permission.roles.contains(&r) {
                            return Ok(Authorization::Allowed);
                        } else {
                            return Ok(Authorization::Blocked);
                        }
                    }
                    Err(e) => {
                        error!("{e:#}");
                        return Ok(Authorization::Blocked);
                    }
                },
                FilterDesc::Block => {
                    match self
                        .parse_role(request)
                        .with_context(|| "failed to parse role for block filter")
                    {
                        Ok(r) => {
                            if h.permission.roles.contains(&r) {
                                return Ok(Authorization::Blocked);
                            } else {
                                return Ok(Authorization::Allowed);
                            }
                        }
                        Err(e) => {
                            error!("{e:#}");
                            return Ok(Authorization::Blocked);
                        }
                    }
                }
            }
        }

        Ok(Authorization::default())
    }
}

type SendType = u64;
assert_impl_all!(SendType: Send);
assert_impl_all!(AuthService<SendType>: Send);
