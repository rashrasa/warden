use anyhow::Context;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, errors::ErrorKind};

use crate::core::config::Claims;

/// Validates token signature and expiration.
pub fn verify_jwt(jwt: &[u8], public_key_pem: &[u8]) -> anyhow::Result<Claims> {
    let claims = match decode(
        jwt,
        &DecodingKey::from_ed_pem(public_key_pem)
            .with_context(|| "failed to create DecodingKey")?,
        &Validation::new(Algorithm::EdDSA),
    ) {
        Ok(c) => Ok(c),
        Err(e) => match e.kind() {
            ErrorKind::ExpiredSignature => Err(anyhow::Error::from(e).context("token expired")),
            ErrorKind::Json(ser_err) => {
                if ser_err.is_data() {
                    Err(anyhow::Error::from(e)
                        .context("jwt deserialization failed. exp claim may be missing"))
                } else {
                    Err(anyhow::Error::from(e).context("jwt deserialization failed"))
                }
            }
            _ => Err(anyhow::Error::from(e).context("failed to decode jwt")),
        },
    }?;

    Ok(claims.claims)
}
