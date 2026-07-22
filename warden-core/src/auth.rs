use hyper::{Request, body::Incoming};

use crate::utils::path;

const USER_HEADER: &str = "x-warden-user";
const AUTHORIZED_USERS: [&str; 2] = ["user1", "user2"];

pub trait AuthProvider {
    fn verify_request(request: &Request<Incoming>) -> anyhow::Result<Authorization>;
}

pub enum Authorization {
    Allowed,
    Blocked,
}

pub struct DefaultAuthProvider;

impl AuthProvider for DefaultAuthProvider {
    fn verify_request(request: &Request<Incoming>) -> anyhow::Result<Authorization> {
        let path = path(request);

        // public routes
        match path {
            "/favicon.ico" => return Ok(Authorization::Allowed),
            "/status" => return Ok(Authorization::Allowed),
            "/bad-route" => return Err(anyhow::Error::msg("bad route")),
            "" => return Ok(Authorization::Allowed),
            _ => {}
        }

        match request.headers().get(USER_HEADER) {
            None => return Ok(Authorization::Blocked),
            Some(user) => {
                let user_str = String::from_utf8(user.as_bytes().to_vec())?;
                if !AUTHORIZED_USERS.contains(&user_str.as_str()) {
                    return Ok(Authorization::Blocked);
                }
            }
        }

        Ok(Authorization::Blocked)
    }
}
