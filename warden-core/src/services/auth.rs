use anyhow::Context;
use http::StatusCode;
use log::error;

use crate::{
    core::{
        config::{ConfigurationDesc, FilterDesc},
        jwt::verify_jwt,
    },
    utils::{http_error, path},
};

#[derive(Debug)]
pub struct AuthService;

impl AuthService {
    pub async fn handle_request(
        config: impl AsRef<ConfigurationDesc>,
        request: crate::Request,
    ) -> Result<crate::Request, crate::FullResponse> {
        match Self::verify_request(config, &request) {
            Ok(a) => match a {
                Authorization::Allowed => Ok(request),
                Authorization::Blocked => Err(http_error(StatusCode::UNAUTHORIZED)),
            },
            Err(e) => {
                error!("{:#}", e.context("error verifying request"));
                Err(http_error(StatusCode::INTERNAL_SERVER_ERROR))
            }
        }
    }

    pub fn parse_role(
        config: impl AsRef<ConfigurationDesc>,
        request: &crate::Request,
    ) -> anyhow::Result<String> {
        let config = config.as_ref();
        match request.inner.headers().get(hyper::header::AUTHORIZATION) {
            // TODO: indexing like this can panic
            Some(v) => verify_jwt(&v.as_bytes()[7..], config.default_jwt_secret()?)
                .map(|v| v.role)
                .with_context(|| "failed to verify jwt"),
            None => Err(anyhow::Error::msg("no auth header")),
        }
    }

    pub fn verify_request(
        config: impl AsRef<ConfigurationDesc>,
        request: &crate::Request,
    ) -> anyhow::Result<Authorization> {
        let path = path(request);

        if let Some(h) = config.as_ref().handlers.get(path) {
            match &h.permission.filter {
                FilterDesc::Allow => match Self::parse_role(&config, request)
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
                    match Self::parse_role(&config, request)
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

#[derive(Default)]
pub enum Authorization {
    #[default]
    Allowed,

    Blocked,
}
