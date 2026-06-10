//! Resolve the active storage provider at startup: env override → DB settings
//! → local default. Also decrypts secret config fields before building.

use crate::media::store;
use rustapi_media::secret as media_secret;
use rustapi_media::{build, secret_fields, StorageProvider};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;

/// Decrypt any secret fields in `config` in place, using `key`.
pub fn decrypt_secrets(provider: &str, config: &mut Value, key: &[u8; 32]) {
    if let Some(obj) = config.as_object_mut() {
        for name in secret_fields(provider) {
            if let Some(Value::String(s)) = obj.get(name) {
                if media_secret::is_encrypted(s) {
                    if let Ok(plain) = media_secret::decrypt(key, s) {
                        obj.insert(name.to_string(), Value::String(plain));
                    }
                }
            }
        }
    }
}

/// Build the initial provider. Env wins, else DB, else local default.
pub async fn resolve_provider(
    pool: &PgPool,
    secret_key: Option<[u8; 32]>,
) -> Arc<dyn StorageProvider> {
    // 1. Env override.
    if let Ok(provider) = std::env::var("RUSTAPI_MEDIA_PROVIDER") {
        if let Some(cfg) = env_config(&provider) {
            if let Ok(p) = build(&provider, &cfg) {
                return Arc::from(p);
            }
            tracing::warn!(%provider, "RUSTAPI_MEDIA_* env config invalid; falling back");
        }
    }
    // 2. DB settings.
    if let Ok(Some(row)) = store::get_settings(pool).await {
        let mut cfg = row.config.clone();
        if let Some(key) = &secret_key {
            decrypt_secrets(&row.provider, &mut cfg, key);
        }
        if let Ok(p) = build(&row.provider, &cfg) {
            return Arc::from(p);
        }
        tracing::warn!(provider = %row.provider, "stored media settings invalid; falling back to local");
    }
    // 3. Local default.
    let cfg = json!({ "base_dir": "./media-data" });
    Arc::from(build("local", &cfg).expect("local provider always builds"))
}

fn env_config(provider: &str) -> Option<Value> {
    match provider {
        "local" => Some(json!({
            "base_dir": std::env::var("RUSTAPI_MEDIA_BASE_DIR").unwrap_or_else(|_| "./media-data".into()),
        })),
        "s3" => Some(json!({
            "bucket": std::env::var("RUSTAPI_S3_BUCKET").ok()?,
            "region": std::env::var("RUSTAPI_S3_REGION").unwrap_or_else(|_| "us-east-1".into()),
            "endpoint": std::env::var("RUSTAPI_S3_ENDPOINT").ok(),
            "access_key": std::env::var("RUSTAPI_S3_ACCESS_KEY").ok()?,
            "secret_key": std::env::var("RUSTAPI_S3_SECRET_KEY").ok()?,
        })),
        _ => None,
    }
}

/// Parse `RUSTAPI_SECRET_KEY` (hex, 64 chars → 32 bytes). Returns None if unset.
pub fn secret_key_from_env() -> Option<[u8; 32]> {
    let hex = std::env::var("RUSTAPI_SECRET_KEY").ok()?;
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    bytes.try_into().ok()
}
