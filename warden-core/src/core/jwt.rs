use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode, errors::ErrorKind,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub role: String,
    pub exp: usize,
}

// TODO: extract encoding secret
pub fn issue_jwt(role: String, exp: usize, secret: &[u8]) -> anyhow::Result<String> {
    Ok(encode(
        &Header::new(Algorithm::HS512),
        &Claims { role, exp },
        &EncodingKey::from_secret(secret),
    )?)
}

pub fn verify_jwt(jwt: &[u8], secret: &[u8]) -> anyhow::Result<Claims> {
    let claims = match decode(
        jwt,
        &DecodingKey::from_secret(secret),
        &Validation::new(Algorithm::HS512),
    ) {
        Ok(c) => Ok(c),
        Err(e) => match e.kind() {
            ErrorKind::ExpiredSignature => Err(anyhow::Error::from(e).context("token expired")),
            _ => Err(anyhow::Error::from(e).context("failed to decode jwt")),
        },
    }?;

    Ok(claims.claims)
}
