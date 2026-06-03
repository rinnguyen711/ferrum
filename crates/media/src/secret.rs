//! AES-256-GCM encryption for secret provider-config fields. Encrypted values
//! are stored as `enc:v1:<base64(nonce || ciphertext)>` so they are
//! self-identifying (avoids re-encrypting on settings round-trip).

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use base64::Engine;

const PREFIX: &str = "enc:v1:";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed")]
    Decrypt,
    #[error("malformed ciphertext")]
    Malformed,
}

pub fn is_encrypted(s: &str) -> bool {
    s.starts_with(PREFIX)
}

pub fn encrypt(key: &[u8; 32], plaintext: &str) -> Result<String, SecretError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| SecretError::Encrypt)?;
    let mut blob = nonce.to_vec();
    blob.extend_from_slice(&ct);
    let b64 = base64::engine::general_purpose::STANDARD.encode(blob);
    Ok(format!("{PREFIX}{b64}"))
}

pub fn decrypt(key: &[u8; 32], value: &str) -> Result<String, SecretError> {
    let b64 = value.strip_prefix(PREFIX).ok_or(SecretError::Malformed)?;
    let blob = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| SecretError::Malformed)?;
    if blob.len() < 12 {
        return Err(SecretError::Malformed);
    }
    let (nonce_bytes, ct) = blob.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|_| SecretError::Decrypt)?;
    String::from_utf8(pt).map_err(|_| SecretError::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; 32] { [7u8; 32] }

    #[test]
    fn round_trip() {
        let enc = encrypt(&key(), "s3-secret").unwrap();
        assert!(is_encrypted(&enc));
        assert_eq!(decrypt(&key(), &enc).unwrap(), "s3-secret");
    }

    #[test]
    fn wrong_key_fails() {
        let enc = encrypt(&key(), "x").unwrap();
        assert!(decrypt(&[9u8; 32], &enc).is_err());
    }

    #[test]
    fn plaintext_not_detected_as_encrypted() {
        assert!(!is_encrypted("plain"));
    }
}
