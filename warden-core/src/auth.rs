use hyper::{Request, body::Incoming};

use crate::core::path;

const USER_HEADER: &str = "x-warden-user";
const AUTHORIZED_USERS: [&str; 2] = ["user1", "user2"];

pub trait AuthProviderMaker<A: AuthProvider> {
    fn make() -> A;
}

pub trait AuthProvider {
    fn verify_request(request: &Request<Incoming>) -> anyhow::Result<bool>;
}

pub struct DefaultAuthProvider;
impl AuthProviderMaker<Self> for DefaultAuthProvider {
    fn make() -> Self {
        Self
    }
}

impl AuthProvider for DefaultAuthProvider {
    fn verify_request(request: &Request<Incoming>) -> anyhow::Result<bool> {
        let path = path(request);

        // public routes
        match path {
            "/favicon.ico" => return Ok(true),
            "/status" => return Ok(true),
            "/bad-route" => return Err(anyhow::Error::msg("bad route")),
            "" => return Ok(true),
            _ => {}
        }

        match request.headers().get(USER_HEADER) {
            None => return Ok(false),
            Some(user) => {
                let user_str = String::from_utf8(user.as_bytes().to_vec())?;
                if !AUTHORIZED_USERS.contains(&user_str.as_str()) {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}
