use anyhow::Context;
use http::StatusCode;
use log::error;

use crate::{
    core::{
        config::{ConfigurationDesc, FieldDesc, FilterDesc, IdentityProviderDesc, ValueDesc},
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

    pub fn verify_request(
        config: impl AsRef<ConfigurationDesc>,
        request: &crate::Request,
    ) -> anyhow::Result<Authorization> {
        let config = config.as_ref();
        let path = path(request);

        if let Some(h) = config.handlers.get(path) {
            let field = &h.permission.field;
            let value = &h.permission.value;
            let filter = &h.permission.filter;
            match field {
                FieldDesc::JwtClaim { provider, key } => {
                    if let Some(auth_header) =
                        request.inner.headers().get(hyper::header::AUTHORIZATION)
                    {
                        let token = &auth_header.as_bytes()[7..];
                        if let Some(provider) = config.providers.get(provider)
                            && let IdentityProviderDesc::Jwt { public_key_pem } = provider
                        {
                            let claims = match verify_jwt(token, public_key_pem.as_bytes())
                                .with_context(|| "failed to verify jwt")
                            {
                                Ok(c) => c,
                                Err(e) => {
                                    error!("{e:#}");
                                    return Ok(Authorization::Blocked);
                                }
                            };
                            if let Some(claim_value) = claims.other.get(key) {
                                match filter {
                                    FilterDesc::Equals => match value {
                                        ValueDesc::Any { any } => {
                                            if any.iter().any(|v| v == claim_value) {
                                                return Ok(Authorization::Allowed);
                                            }
                                        }
                                        ValueDesc::Value { value } => {
                                            if value == claim_value {
                                                return Ok(Authorization::Allowed);
                                            }
                                        }
                                    },
                                    FilterDesc::NotEquals => match value {
                                        ValueDesc::Any { any } => {
                                            if any.iter().all(|v| v != claim_value) {
                                                return Ok(Authorization::Allowed);
                                            }
                                        }
                                        ValueDesc::Value { value } => {
                                            if value != claim_value {
                                                return Ok(Authorization::Allowed);
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(Authorization::Blocked)
    }
}

pub enum Authorization {
    Allowed,
    Blocked,
}
