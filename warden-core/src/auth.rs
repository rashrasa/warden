use std::{collections::HashSet, sync::Arc};

use hyper::{Request, body::Incoming};
use tokio::sync::RwLock;

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
            "/dynamic" => return Ok(Authorization::Allowed),
            "/dyn" => return Ok(Authorization::Allowed),
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

#[derive(Debug, Clone)]
pub struct Role {
    id: u64,
    metadata: Arc<RwLock<RoleMetadata>>,
}

#[derive(Debug)]
pub struct RoleMetadata {
    name: String,
    keys: HashSet<String>,
}

pub enum Ruleset {
    AllowList(HashSet<String>),
    BlockList(HashSet<String>),
}

impl Ruleset {
    fn is_allowed(&self, key: &str) -> bool {
        match self {
            Ruleset::AllowList(l) => l.contains(key),
            Ruleset::BlockList(l) => !l.contains(key),
        }
    }
}
