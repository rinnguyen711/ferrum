//! Self-describing provider registry. The HTTP layer exposes `descriptors()`
//! so UIs can render config forms generically, and calls `build()` to
//! instantiate the active provider from a JSON config blob.

use crate::local::LocalProvider;
use crate::provider::{StorageError, StorageProvider};
use crate::s3::{S3Config, S3Provider};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConfigField {
    pub name: &'static str,
    pub label: &'static str,
    pub r#type: &'static str, // "string"
    pub required: bool,
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProviderDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    pub fields: Vec<ConfigField>,
}

fn field(name: &'static str, label: &'static str, required: bool, secret: bool) -> ConfigField {
    ConfigField { name, label, r#type: "string", required, secret }
}

pub fn descriptors() -> Vec<ProviderDescriptor> {
    vec![
        ProviderDescriptor {
            id: "local",
            label: "Local Filesystem",
            fields: vec![
                field("base_dir", "Base directory", true, false),
                field("public_url", "Public URL prefix", false, false),
            ],
        },
        ProviderDescriptor {
            id: "s3",
            label: "Amazon S3",
            fields: vec![
                field("bucket", "Bucket", true, false),
                field("region", "Region", true, false),
                field("endpoint", "Endpoint (S3-compatible)", false, false),
                field("access_key", "Access key", true, false),
                field("secret_key", "Secret key", true, true),
            ],
        },
    ]
}

pub fn descriptor_for(id: &str) -> Option<ProviderDescriptor> {
    descriptors().into_iter().find(|d| d.id == id)
}

/// Names of the secret fields for a provider (for encrypt/mask logic).
pub fn secret_fields(id: &str) -> Vec<&'static str> {
    descriptor_for(id)
        .map(|d| d.fields.iter().filter(|f| f.secret).map(|f| f.name).collect())
        .unwrap_or_default()
}

/// Validate a config blob against the provider descriptor: provider exists,
/// every required field is a present non-empty string.
pub fn validate(id: &str, config: &Value) -> Result<(), StorageError> {
    let desc = descriptor_for(id).ok_or_else(|| StorageError::Other(format!("unknown provider `{id}`")))?;
    let obj = config.as_object().ok_or_else(|| StorageError::Other("config must be an object".into()))?;
    for f in &desc.fields {
        if f.required {
            let ok = obj.get(f.name).and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
            if !ok {
                return Err(StorageError::Other(format!("missing required field `{}`", f.name)));
            }
        }
    }
    Ok(())
}

/// Build a live provider from a (already-decrypted) config blob.
pub fn build(id: &str, config: &Value) -> Result<Box<dyn StorageProvider>, StorageError> {
    match id {
        "local" => {
            let base_dir = config.get("base_dir").and_then(|v| v.as_str())
                .ok_or_else(|| StorageError::Other("local: base_dir required".into()))?;
            Ok(Box::new(LocalProvider::new(base_dir)))
        }
        "s3" => {
            let cfg: S3Config = serde_json::from_value(config.clone())
                .map_err(|e| StorageError::Other(format!("s3 config: {e}")))?;
            Ok(Box::new(S3Provider::new(cfg)?))
        }
        other => Err(StorageError::Other(format!("unknown provider `{other}`"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn descriptors_include_local_and_s3() {
        let ids: Vec<_> = descriptors().iter().map(|d| d.id).collect();
        assert!(ids.contains(&"local"));
        assert!(ids.contains(&"s3"));
    }

    #[test]
    fn s3_secret_key_is_marked_secret() {
        assert_eq!(secret_fields("s3"), vec!["secret_key"]);
        assert!(secret_fields("local").is_empty());
    }

    #[test]
    fn validate_rejects_missing_required() {
        assert!(validate("s3", &json!({"bucket": "b"})).is_err());
        assert!(validate("local", &json!({"base_dir": "/tmp/x"})).is_ok());
    }

    #[test]
    fn validate_rejects_unknown_provider() {
        assert!(validate("ftp", &json!({})).is_err());
    }

    #[test]
    fn build_local_succeeds() {
        assert!(build("local", &json!({"base_dir": "/tmp/x"})).is_ok());
    }
}
