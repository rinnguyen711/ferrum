//! HS256 JWT encode/decode.

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// User id.
    pub sub: Uuid,
    pub email: String,
    pub roles: Vec<String>,
    /// Issued-at (unix seconds).
    pub iat: i64,
    /// Expiry (unix seconds).
    pub exp: i64,
}

/// Sign claims for `sub`/`email`/`roles`, expiring `ttl_secs` from now.
pub fn sign(
    secret: &[u8],
    sub: Uuid,
    email: &str,
    roles: &[String],
    ttl_secs: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub,
        email: email.to_string(),
        roles: roles.to_vec(),
        iat: now,
        exp: now + ttl_secs,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )
}

/// Verify an HS256 token and return its claims. Rejects bad signature / expiry.
pub fn verify(secret: &[u8], token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::new(Algorithm::HS256),
    )?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-at-least-32-bytes-long!!";

    #[test]
    fn round_trip() {
        let id = Uuid::new_v4();
        let token = sign(SECRET, id, "a@b.c", &["admin".into()], 3600).unwrap();
        let claims = verify(SECRET, &token).unwrap();
        assert_eq!(claims.sub, id);
        assert_eq!(claims.email, "a@b.c");
        assert_eq!(claims.roles, vec!["admin".to_string()]);
    }

    #[test]
    fn wrong_secret_rejected() {
        let token = sign(SECRET, Uuid::new_v4(), "a@b.c", &[], 3600).unwrap();
        assert!(verify(b"a-completely-different-secret-32xx!!", &token).is_err());
    }

    #[test]
    fn expired_rejected() {
        // jsonwebtoken's default validation allows 60s leeway, so push the
        // expiry well past it (-120s) to guarantee rejection.
        let token = sign(SECRET, Uuid::new_v4(), "a@b.c", &[], -120).unwrap();
        assert!(verify(SECRET, &token).is_err());
    }
}
