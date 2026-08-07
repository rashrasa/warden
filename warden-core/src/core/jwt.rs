use anyhow::Context;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub role: String,
    pub exp: usize,
}

// TODO: extract encoding secret
pub fn issue_jwt(role: String, exp: usize) -> anyhow::Result<String> {
    Ok(encode(
        &Header::new(Algorithm::HS512),
        &Claims { role, exp },
        &EncodingKey::from_secret("secret".as_bytes()),
    )?)
}

pub fn verify_jwt(jwt: &[u8]) -> anyhow::Result<Claims> {
    let claims = decode(
        jwt,
        &DecodingKey::from_secret("secret".as_bytes()),
        &Validation::new(Algorithm::HS512),
    )
    .with_context(|| "failed to decode jwt")?;

    Ok(claims.claims)
}
