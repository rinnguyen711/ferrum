//! Argon2id password hashing.

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;

/// Hash a plaintext password into an Argon2id PHC string.
pub fn hash(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a plaintext password against a stored PHC hash. Returns false on
/// mismatch or malformed hash (never errors out to the caller).
pub fn verify(password: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_true() {
        let phc = hash("hunter2-correct").unwrap();
        assert!(verify("hunter2-correct", &phc));
    }

    #[test]
    fn wrong_password_verify_false() {
        let phc = hash("hunter2-correct").unwrap();
        assert!(!verify("hunter2-wrong", &phc));
    }

    #[test]
    fn malformed_hash_verify_false() {
        assert!(!verify("anything", "not-a-phc-string"));
    }
}
