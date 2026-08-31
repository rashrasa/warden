pub mod auth;
pub mod route;

pub use auth::AuthService;
use http::StatusCode;
use log::error;
pub use route::RouterService;

use crate::{core::config::ConfigurationDesc, services::route::Routes, utils::http_error};

pub struct RequestService;

impl RequestService {
    pub async fn handle_request(
        config: impl AsRef<ConfigurationDesc>,
        routes: impl AsRef<Routes>,
        request: crate::Request,
    ) -> crate::FullResponse {
        let request = match AuthService::handle_request(&config, request).await {
            Ok(r) => r,
            Err(e) => return e,
        };

        match RouterService::route(&config, routes, request).await {
            Ok(res) => res,
            Err(e) => {
                error!("{:#}", e.context("failed to handle request"));
                http_error(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}
