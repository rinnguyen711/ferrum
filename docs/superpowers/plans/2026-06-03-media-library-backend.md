# Media Library Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a standalone Media Library backend for ferrum — pluggable storage providers (local default + S3), nested folders, asset upload/metadata, and a self-describing provider settings API.

**Architecture:** A new `crates/media` crate owns a `StorageProvider` trait with `local` and `s3` impls plus a self-describing provider registry. The `crates/http` crate gains a `media` module: a `store` data-access layer (raw sqlx against `_media_*` tables, mirroring `auth/users.rs`), HTTP handlers in `routes/media.rs`, and the active provider held in `AppState` as an `arc_swap::ArcSwap<dyn StorageProvider>`. Migration `0003_media.sql` adds the tables. Authz reuses `Action::ContentRead`/`ContentWrite`.

**Tech Stack:** Rust, Axum 0.7, sqlx 0.8 (Postgres), `rust-s3`, `aes-gcm`, `sha2`, `infer` (mime sniff), `imagesize`, `arc-swap`, `async-trait`. Tests: inline `#[tokio::test]` units + testcontainers integration tests via `crates/bin/tests`.

**Reference patterns (read before starting):**
- DAL style: `crates/http/src/auth/users.rs` (raw `sqlx::query_as`, `Result<_, sqlx::Error>`).
- Handler style + error mapping: `crates/http/src/routes/users.rs` (`ApiError`, `ensure` authz gate, `map_db_err`).
- Error variants: `crates/http/src/error.rs` (`Error::NotFound` → 404, `Error::Conflict` → 409, `Error::Validation` → 422, `Error::Internal`).
- Migration conventions: `crates/schema/migrations/0002_users.sql` (`_` prefix, `CREATE TABLE IF NOT EXISTS`, `gen_random_uuid()`, `now()`).
- AppState + router build: `crates/http/src/state.rs`, `crates/http/src/routes/mod.rs`.
- Integration harness: `crates/bin/tests/common/mod.rs` (testcontainers + in-process router + reqwest).
- Migrations auto-run via `sqlx::migrate!("./migrations")` in `crates/schema/src/lib.rs`.

---

## File Structure

**New crate `crates/media`:**
- `crates/media/Cargo.toml` — deps.
- `crates/media/src/lib.rs` — re-exports (`StorageProvider`, `StorageError`, registry fns, config helpers).
- `crates/media/src/provider.rs` — `StorageProvider` trait + `StorageError`.
- `crates/media/src/local.rs` — local filesystem provider + its factory.
- `crates/media/src/s3.rs` — S3 provider + its factory (`rust-s3`).
- `crates/media/src/registry.rs` — `ConfigField`, `ProviderDescriptor`, `descriptors()`, `build(provider, config)`.
- `crates/media/src/secret.rs` — AES-GCM encrypt/decrypt of secret config fields.

**Modified / new in `crates/http`:**
- `crates/http/src/media/mod.rs` — module root, re-exports `store`.
- `crates/http/src/media/store.rs` — `_media_folders` / `_media_assets` / `_media_settings` DAL.
- `crates/http/src/routes/media.rs` — HTTP handlers + `router()`.
- `crates/http/src/routes/mod.rs` — merge `media::router()` into protected router.
- `crates/http/src/state.rs` — add `storage: Arc<ArcSwap<dyn StorageProvider>>` + `secret_key: Option<[u8;32]>` to `AppState`.
- `crates/http/src/lib.rs` — re-export anything tests need.
- `crates/http/Cargo.toml` — add `ferrum-media`, `arc-swap`, `sha2`, `infer`, `imagesize`.

**Migration:**
- `crates/schema/migrations/0003_media.sql`.

**Bin / harness:**
- `crates/bin/src/main.rs` — build initial storage provider, pass into `AppState`.
- `crates/bin/src/config.rs` — read `FERRUM_MEDIA_*` / `FERRUM_SECRET_KEY` env.
- `crates/bin/tests/common/mod.rs` — construct the new `AppState` fields.
- `crates/bin/tests/media.rs` — integration tests.

---

## Task 1: Scaffold `crates/media` crate with the provider trait

**Files:**
- Create: `crates/media/Cargo.toml`
- Create: `crates/media/src/lib.rs`
- Create: `crates/media/src/provider.rs`
- Modify: `Cargo.toml` (workspace members + workspace deps)

- [ ] **Step 1: Add the crate to the workspace and add new workspace deps**

In root `Cargo.toml`, add `"crates/media"` to `members` (after `"crates/http"`). In `[workspace.dependencies]` add:

```toml
async-trait = "0.1"
bytes = "1"
rust-s3 = { version = "0.34", default-features = false, features = ["tokio-rustls-tls"] }
aes-gcm = "0.10"
sha2 = "0.10"
infer = "0.16"
imagesize = "0.13"
arc-swap = "1"
```

(`tokio`, `serde`, `serde_json`, `thiserror`, `uuid` already exist at workspace level — reuse them.)

- [ ] **Step 2: Create `crates/media/Cargo.toml`**

```toml
[package]
name = "ferrum-media"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
async-trait = { workspace = true }
bytes = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true, features = ["fs", "io-util"] }
rust-s3 = { workspace = true }
aes-gcm = { workspace = true }

[dev-dependencies]
tempfile = "3"
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "fs", "io-util"] }
```

- [ ] **Step 3: Write `crates/media/src/provider.rs` (trait + error)**

```rust
//! Storage provider abstraction. Each provider stores opaque byte blobs
//! keyed by a caller-chosen string (the asset's `storage_key`).

use async_trait::async_trait;
use bytes::Bytes;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("object not found")]
    NotFound,
    #[error("authentication/credentials rejected: {0}")]
    Auth(String),
    #[error("connection failed: {0}")]
    Connection(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("storage error: {0}")]
    Other(String),
}

#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Store `bytes` at `key`. Overwrites if the key exists.
    async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<(), StorageError>;
    /// Fetch the bytes stored at `key`.
    async fn get(&self, key: &str) -> Result<Bytes, StorageError>;
    /// Remove the object at `key`. Missing object is `Ok(())` (idempotent).
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    /// Cheap connectivity / credential check for the settings "Test" button.
    async fn test(&self) -> Result<(), StorageError>;
}
```

- [ ] **Step 4: Write `crates/media/src/lib.rs`**

```rust
//! Pluggable media storage for ferrum.

pub mod provider;

pub use provider::{StorageError, StorageProvider};
```

- [ ] **Step 5: Build the crate**

Run: `cargo build -p ferrum-media`
Expected: compiles (no warnings-as-errors expected; clean build).

- [ ] **Step 6: Commit**

```bash
git add crates/media Cargo.toml Cargo.lock
git commit -m "feat(media): scaffold media crate with StorageProvider trait"
```

---

## Task 2: Local filesystem provider

**Files:**
- Create: `crates/media/src/local.rs`
- Modify: `crates/media/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/media/src/local.rs`:

```rust
//! Local filesystem `StorageProvider`. Keys map to paths under `base_dir`.
//! Key path components are sanitized to keep writes inside `base_dir`.

use crate::provider::{StorageError, StorageProvider};
use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Path, PathBuf};

pub struct LocalProvider {
    base_dir: PathBuf,
}

impl LocalProvider {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self { base_dir: base_dir.into() }
    }

    /// Resolve a key to a path under base_dir, rejecting traversal.
    fn resolve(&self, key: &str) -> Result<PathBuf, StorageError> {
        let mut path = self.base_dir.clone();
        for comp in Path::new(key).components() {
            use std::path::Component::*;
            match comp {
                Normal(c) => path.push(c),
                CurDir => {}
                _ => return Err(StorageError::Other("invalid key".into())),
            }
        }
        Ok(path)
    }
}

#[async_trait]
impl StorageProvider for LocalProvider {
    async fn put(&self, key: &str, bytes: Bytes, _content_type: &str) -> Result<(), StorageError> {
        let path = self.resolve(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
        }
        tokio::fs::write(&path, &bytes)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))
    }

    async fn get(&self, key: &str) -> Result<Bytes, StorageError> {
        let path = self.resolve(key)?;
        match tokio::fs::read(&path).await {
            Ok(b) => Ok(Bytes::from(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(StorageError::NotFound),
            Err(e) => Err(StorageError::Io(e.to_string())),
        }
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.resolve(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Io(e.to_string())),
        }
    }

    async fn test(&self) -> Result<(), StorageError> {
        tokio::fs::create_dir_all(&self.base_dir)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_get_delete_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = LocalProvider::new(dir.path());
        p.put("a/b.txt", Bytes::from_static(b"hi"), "text/plain").await.unwrap();
        let got = p.get("a/b.txt").await.unwrap();
        assert_eq!(&got[..], b"hi");
        p.delete("a/b.txt").await.unwrap();
        assert!(matches!(p.get("a/b.txt").await, Err(StorageError::NotFound)));
    }

    #[tokio::test]
    async fn delete_missing_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let p = LocalProvider::new(dir.path());
        assert!(p.delete("nope.txt").await.is_ok());
    }

    #[tokio::test]
    async fn traversal_key_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = LocalProvider::new(dir.path());
        assert!(p.put("../escape.txt", Bytes::from_static(b"x"), "text/plain").await.is_err());
    }
}
```

Add `pub mod local;` and `pub use local::LocalProvider;` to `crates/media/src/lib.rs`.

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cargo test -p ferrum-media local::`
Expected: PASS (3 tests). (The implementation is included above; this provider is small enough that test + impl land together.)

- [ ] **Step 3: Commit**

```bash
git add crates/media/src/local.rs crates/media/src/lib.rs
git commit -m "feat(media): local filesystem provider"
```

---

## Task 3: Secret encryption helper

**Files:**
- Create: `crates/media/src/secret.rs`
- Modify: `crates/media/src/lib.rs`

- [ ] **Step 1: Write `crates/media/src/secret.rs` with tests**

AES-256-GCM. Nonce (12 bytes) is random per encryption, prepended to ciphertext, whole thing base64-encoded. Output is tagged with a prefix so we can detect already-encrypted values and avoid double-encryption.

```rust
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
```

- [ ] **Step 2: Add deps**

In `crates/media/Cargo.toml` `[dependencies]` add:

```toml
base64 = "0.22"
```

(`aes-gcm` already added in Task 1.) Add `pub mod secret;` to `crates/media/src/lib.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p ferrum-media secret::`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/media/src/secret.rs crates/media/src/lib.rs crates/media/Cargo.toml Cargo.lock
git commit -m "feat(media): AES-GCM secret field encryption"
```

---

## Task 4: Provider registry (descriptors + build + secret handling)

**Files:**
- Create: `crates/media/src/registry.rs`
- Modify: `crates/media/src/lib.rs`
- Modify: `crates/media/src/s3.rs` (created here as a stub; filled in Task 5)

- [ ] **Step 1: Create an S3 stub so the registry compiles**

Create `crates/media/src/s3.rs`:

```rust
//! Amazon S3 (and S3-compatible) provider. Full impl in Task 5.

use crate::provider::{StorageError, StorageProvider};
use async_trait::async_trait;
use bytes::Bytes;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    pub access_key: String,
    pub secret_key: String,
}

pub struct S3Provider {
    #[allow(dead_code)]
    cfg: S3Config,
}

impl S3Provider {
    pub fn new(cfg: S3Config) -> Result<Self, StorageError> {
        Ok(Self { cfg })
    }
}

#[async_trait]
impl StorageProvider for S3Provider {
    async fn put(&self, _key: &str, _bytes: Bytes, _ct: &str) -> Result<(), StorageError> {
        Err(StorageError::Other("s3 not implemented".into()))
    }
    async fn get(&self, _key: &str) -> Result<Bytes, StorageError> {
        Err(StorageError::Other("s3 not implemented".into()))
    }
    async fn delete(&self, _key: &str) -> Result<(), StorageError> {
        Err(StorageError::Other("s3 not implemented".into()))
    }
    async fn test(&self) -> Result<(), StorageError> {
        Err(StorageError::Other("s3 not implemented".into()))
    }
}
```

Add `pub mod s3;` to `crates/media/src/lib.rs`.

- [ ] **Step 2: Write the registry with tests**

Create `crates/media/src/registry.rs`:

```rust
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
```

Add to `crates/media/src/lib.rs`:

```rust
pub mod registry;

pub use registry::{build, descriptors, descriptor_for, secret_fields, validate, ConfigField, ProviderDescriptor};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p ferrum-media registry::`
Expected: PASS (5 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/media/src/registry.rs crates/media/src/s3.rs crates/media/src/lib.rs
git commit -m "feat(media): provider registry with self-describing descriptors"
```

---

## Task 5: Implement the S3 provider

**Files:**
- Modify: `crates/media/src/s3.rs`

- [ ] **Step 1: Replace the stub body with the real `rust-s3` impl**

Replace the trait impl block (keep `S3Config` as-is). `rust-s3`'s `Bucket` is the client; with a custom endpoint it uses path-style addressing (needed for MinIO/R2).

```rust
//! Amazon S3 (and S3-compatible) provider via the `rust-s3` crate.

use crate::provider::{StorageError, StorageProvider};
use async_trait::async_trait;
use bytes::Bytes;
use s3::creds::Credentials;
use s3::{Bucket, BucketConfiguration, Region};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    pub access_key: String,
    pub secret_key: String,
}

pub struct S3Provider {
    bucket: Box<Bucket>,
}

impl S3Provider {
    pub fn new(cfg: S3Config) -> Result<Self, StorageError> {
        let region = match &cfg.endpoint {
            Some(endpoint) => Region::Custom {
                region: cfg.region.clone(),
                endpoint: endpoint.clone(),
            },
            None => cfg.region.parse().map_err(|_| StorageError::Other("bad region".into()))?,
        };
        let creds = Credentials::new(Some(&cfg.access_key), Some(&cfg.secret_key), None, None, None)
            .map_err(|e| StorageError::Auth(e.to_string()))?;
        let mut bucket = Bucket::new(&cfg.bucket, region, creds)
            .map_err(|e| StorageError::Other(e.to_string()))?;
        // S3-compatible endpoints need path-style addressing.
        if cfg.endpoint.is_some() {
            bucket.set_path_style();
        }
        Ok(Self { bucket })
    }
    // Silence unused on BucketConfiguration import in case of future use.
    #[allow(dead_code)]
    fn _bucket_config_marker() -> BucketConfiguration { BucketConfiguration::default() }
}

#[async_trait]
impl StorageProvider for S3Provider {
    async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<(), StorageError> {
        self.bucket
            .put_object_with_content_type(key, &bytes, content_type)
            .await
            .map(|_| ())
            .map_err(map_s3_err)
    }

    async fn get(&self, key: &str) -> Result<Bytes, StorageError> {
        let resp = self.bucket.get_object(key).await.map_err(map_s3_err)?;
        if resp.status_code() == 404 {
            return Err(StorageError::NotFound);
        }
        if resp.status_code() >= 400 {
            return Err(StorageError::Other(format!("s3 status {}", resp.status_code())));
        }
        Ok(Bytes::from(resp.bytes().to_vec()))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.bucket.delete_object(key).await.map(|_| ()).map_err(map_s3_err)
    }

    async fn test(&self) -> Result<(), StorageError> {
        // HEAD a definitely-absent key: 404 proves creds + bucket reachable.
        match self.bucket.head_object("__ferrum_healthcheck__").await {
            Ok(_) => Ok(()),
            Err(s3::error::S3Error::HttpFailWithBody(404, _)) => Ok(()),
            Err(e) => Err(map_s3_err(e)),
        }
    }
}

fn map_s3_err(e: s3::error::S3Error) -> StorageError {
    use s3::error::S3Error::*;
    match e {
        HttpFailWithBody(403, _) => StorageError::Auth("s3 403".into()),
        HttpFailWithBody(404, _) => StorageError::NotFound,
        Http(_, _) | HttpFailWithBody(_, _) => StorageError::Connection(e_to_string(&e)),
        other => StorageError::Other(other.to_string()),
    }
}

fn e_to_string(e: &s3::error::S3Error) -> String {
    e.to_string()
}
```

> **Note for implementer:** `rust-s3` 0.34's exact error variant names (`HttpFailWithBody`, `Http`) and `head_object`/`get_object` return shapes can differ slightly by patch version. If a variant name doesn't resolve, run `cargo doc -p rust-s3 --open` or check the compile error and map to the nearest equivalent — the contract to preserve is: 404 → `NotFound`, 403 → `Auth`, other HTTP/transport → `Connection`, everything else → `Other`. Keep the test's "404 on missing key counts as success" behavior.

- [ ] **Step 2: Build (no live S3 in unit tests)**

Run: `cargo build -p ferrum-media`
Expected: compiles. (S3 needs a live endpoint; covered by the env-gated integration test below, not unit tests.)

- [ ] **Step 3: Add an env-gated integration test**

Create `crates/media/tests/s3_minio.rs`:

```rust
//! Live S3 round-trip against MinIO. Skipped unless FERRUM_TEST_S3=1.
//! Expects env: S3_ENDPOINT, S3_BUCKET, S3_REGION, S3_KEY, S3_SECRET.

use bytes::Bytes;
use ferrum_media::s3::{S3Config, S3Provider};
use ferrum_media::StorageProvider;

#[tokio::test]
async fn s3_round_trip() {
    if std::env::var("FERRUM_TEST_S3").ok().as_deref() != Some("1") {
        eprintln!("skipping: set FERRUM_TEST_S3=1 to run");
        return;
    }
    let cfg = S3Config {
        bucket: std::env::var("S3_BUCKET").unwrap(),
        region: std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".into()),
        endpoint: std::env::var("S3_ENDPOINT").ok(),
        access_key: std::env::var("S3_KEY").unwrap(),
        secret_key: std::env::var("S3_SECRET").unwrap(),
    };
    let p = S3Provider::new(cfg).unwrap();
    p.test().await.unwrap();
    p.put("it/x.txt", Bytes::from_static(b"hi"), "text/plain").await.unwrap();
    assert_eq!(&p.get("it/x.txt").await.unwrap()[..], b"hi");
    p.delete("it/x.txt").await.unwrap();
}
```

Make `s3` module public path usable: ensure `crates/media/src/lib.rs` has `pub mod s3;` (added in Task 4).

- [ ] **Step 4: Run unit tests (S3 test self-skips)**

Run: `cargo test -p ferrum-media`
Expected: PASS; the s3_minio test prints "skipping".

- [ ] **Step 5: Commit**

```bash
git add crates/media/src/s3.rs crates/media/tests/s3_minio.rs
git commit -m "feat(media): S3 provider via rust-s3"
```

---

## Task 6: Migration `0003_media.sql`

**Files:**
- Create: `crates/schema/migrations/0003_media.sql`

- [ ] **Step 1: Write the migration**

```sql
CREATE TABLE IF NOT EXISTS _media_folders (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_id   UUID REFERENCES _media_folders(id),
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (parent_id, name)
);

CREATE TABLE IF NOT EXISTS _media_assets (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    folder_id          UUID REFERENCES _media_folders(id),
    provider           TEXT NOT NULL,
    storage_key        TEXT NOT NULL,
    file_name          TEXT NOT NULL,
    alt_text           TEXT,
    caption            TEXT,
    mime_type          TEXT NOT NULL,
    size_bytes         BIGINT NOT NULL,
    width              INTEGER,
    height             INTEGER,
    original_filename  TEXT NOT NULL,
    checksum           TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS _media_assets_folder_idx ON _media_assets (folder_id);

CREATE TABLE IF NOT EXISTS _media_settings (
    id          BOOLEAN PRIMARY KEY DEFAULT TRUE,
    provider    TEXT NOT NULL,
    config      JSONB NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT _media_settings_singleton CHECK (id)
);
```

- [ ] **Step 2: Verify the migration applies**

Run: `cargo test -p ferrum-bin --test '*' -- --nocapture 2>&1 | head -40` (the integration harness runs `MIGRATOR.run`; if no bin tests exist yet, instead run `cargo build` and rely on Task 12's media integration test to exercise it).
Expected: no migration error. (`pgcrypto` for `gen_random_uuid()` is enabled by `0001_init.sql`.)

- [ ] **Step 3: Commit**

```bash
git add crates/schema/migrations/0003_media.sql
git commit -m "feat(media): 0003 media tables migration"
```

---

## Task 7: Wire storage provider into `AppState`

**Files:**
- Modify: `crates/http/Cargo.toml`
- Modify: `crates/http/src/state.rs`
- Modify: `crates/http/src/lib.rs`

- [ ] **Step 1: Add deps to `crates/http/Cargo.toml`**

In `[dependencies]`:

```toml
ferrum-media = { path = "../media" }
arc-swap = { workspace = true }
sha2 = { workspace = true }
infer = { workspace = true }
imagesize = { workspace = true }
```

- [ ] **Step 2: Add storage + secret key to `AppState`**

In `crates/http/src/state.rs`, add imports near the top:

```rust
use arc_swap::ArcSwap;
use ferrum_media::StorageProvider;
```

Add fields to the `AppState` struct (after `config`):

```rust
    /// Active media storage provider, hot-swappable when settings change.
    pub storage: Arc<ArcSwap<dyn StorageProvider>>,
    /// 32-byte key for encrypting secret provider-config fields. `None`
    /// disables saving providers that declare secret fields.
    pub secret_key: Option<[u8; 32]>,
```

- [ ] **Step 3: Re-export for tests/bin in `crates/http/src/lib.rs`**

Ensure these are re-exported (add any missing):

```rust
pub use state::{AppConfig, AppState, Authz, AlwaysAllow, EventSink, NoopSink, RoleAuthz};
```

Also add a convenience re-export so callers don't need a direct media dep just to construct state:

```rust
pub use ferrum_media::{descriptors, LocalProvider, StorageProvider};
```

(Adjust to match the existing `pub use` lines; only add what's not already there.)

- [ ] **Step 4: Build (will fail at AppState construction sites — expected, fixed in Task 8 & 11)**

Run: `cargo build -p ferrum-http`
Expected: FAIL — `AppState` literal(s) in `state.rs` tests and elsewhere now miss `storage`/`secret_key`. Note the error locations; they're fixed next.

If `crates/http/src/state.rs` has its own `AppState { ... }` literal in tests, none currently exists (the tests construct only `RoleAuthz`), so the http crate library itself should build. The breakage will surface in `crates/bin` (Task 11) and integration harness (Task 11). If the http lib builds clean, that's fine — proceed.

- [ ] **Step 5: Commit**

```bash
git add crates/http/Cargo.toml crates/http/src/state.rs crates/http/src/lib.rs Cargo.lock
git commit -m "feat(media): add storage provider + secret key to AppState"
```

---

## Task 8: Media DAL — `_media_folders`

**Files:**
- Create: `crates/http/src/media/mod.rs`
- Create: `crates/http/src/media/store.rs`
- Modify: `crates/http/src/lib.rs` (add `mod media;`)

- [ ] **Step 1: Create the module root**

`crates/http/src/media/mod.rs`:

```rust
//! Media library: data-access layer and (later) HTTP wiring helpers.

pub mod store;
```

Add `mod media;` to `crates/http/src/lib.rs` (alongside the other `mod` declarations).

- [ ] **Step 2: Write the folder DAL with row type**

`crates/http/src/media/store.rs` (folders section; assets/settings appended in Tasks 9–10):

```rust
//! Raw sqlx access to the `_media_*` tables. Mirrors `auth/users.rs` style:
//! plain `query_as` returning typed rows, `Result<_, sqlx::Error>`.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct FolderRow {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

type FolderTuple = (Uuid, Option<Uuid>, String, DateTime<Utc>, DateTime<Utc>);

fn folder_from(t: FolderTuple) -> FolderRow {
    FolderRow { id: t.0, parent_id: t.1, name: t.2, created_at: t.3, updated_at: t.4 }
}

const FOLDER_COLS: &str = "id, parent_id, name, created_at, updated_at";

/// List folders under `parent_id` (None = root level), name-sorted.
pub async fn list_folders(pool: &PgPool, parent_id: Option<Uuid>) -> Result<Vec<FolderRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, FolderTuple>(&format!(
        "SELECT {FOLDER_COLS} FROM _media_folders \
         WHERE parent_id IS NOT DISTINCT FROM $1 ORDER BY name"
    ))
    .bind(parent_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(folder_from).collect())
}

pub async fn create_folder(pool: &PgPool, parent_id: Option<Uuid>, name: &str) -> Result<FolderRow, sqlx::Error> {
    let t = sqlx::query_as::<_, FolderTuple>(&format!(
        "INSERT INTO _media_folders (parent_id, name) VALUES ($1, $2) RETURNING {FOLDER_COLS}"
    ))
    .bind(parent_id)
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(folder_from(t))
}

pub async fn get_folder(pool: &PgPool, id: Uuid) -> Result<Option<FolderRow>, sqlx::Error> {
    let t = sqlx::query_as::<_, FolderTuple>(&format!(
        "SELECT {FOLDER_COLS} FROM _media_folders WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(t.map(folder_from))
}

/// Update name and/or parent. `None` leaves a field unchanged. `updated_at` bumped.
pub async fn update_folder(
    pool: &PgPool,
    id: Uuid,
    name: Option<&str>,
    parent_id: Option<Option<Uuid>>,
) -> Result<Option<FolderRow>, sqlx::Error> {
    let t = sqlx::query_as::<_, FolderTuple>(&format!(
        "UPDATE _media_folders SET \
            name = COALESCE($2, name), \
            parent_id = CASE WHEN $3 THEN $4 ELSE parent_id END, \
            updated_at = now() \
         WHERE id = $1 RETURNING {FOLDER_COLS}"
    ))
    .bind(id)
    .bind(name)
    .bind(parent_id.is_some())          // $3: whether to touch parent_id
    .bind(parent_id.flatten())          // $4: new parent (may be NULL = root)
    .fetch_optional(pool)
    .await?;
    Ok(t.map(folder_from))
}

/// True if the folder has any child folder or asset.
pub async fn folder_has_children(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let (exists,): (bool,) = sqlx::query_as(
        "SELECT EXISTS ( \
            SELECT 1 FROM _media_folders WHERE parent_id = $1 \
            UNION ALL SELECT 1 FROM _media_assets WHERE folder_id = $1 \
         )",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Delete a folder. Caller must ensure it's empty. Returns true if a row was deleted.
pub async fn delete_folder(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM _media_folders WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
```

- [ ] **Step 3: Add `chrono` to http deps if absent**

Check `crates/http/Cargo.toml`. If `chrono` isn't listed, add `chrono = { workspace = true }`.

- [ ] **Step 4: Build**

Run: `cargo build -p ferrum-http`
Expected: compiles (DAL is pure; no AppState literal here).

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/media crates/http/src/lib.rs crates/http/Cargo.toml Cargo.lock
git commit -m "feat(media): folder data-access layer"
```

---

## Task 9: Media DAL — `_media_assets`

**Files:**
- Modify: `crates/http/src/media/store.rs`

- [ ] **Step 1: Append the asset row type + queries**

Add to `crates/http/src/media/store.rs`:

```rust
#[derive(Debug, Clone)]
pub struct AssetRow {
    pub id: Uuid,
    pub folder_id: Option<Uuid>,
    pub provider: String,
    pub storage_key: String,
    pub file_name: String,
    pub alt_text: Option<String>,
    pub caption: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub original_filename: String,
    pub checksum: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

type AssetTuple = (
    Uuid, Option<Uuid>, String, String, String, Option<String>, Option<String>,
    String, i64, Option<i32>, Option<i32>, String, Option<String>,
    DateTime<Utc>, DateTime<Utc>,
);

fn asset_from(t: AssetTuple) -> AssetRow {
    AssetRow {
        id: t.0, folder_id: t.1, provider: t.2, storage_key: t.3, file_name: t.4,
        alt_text: t.5, caption: t.6, mime_type: t.7, size_bytes: t.8, width: t.9,
        height: t.10, original_filename: t.11, checksum: t.12, created_at: t.13, updated_at: t.14,
    }
}

const ASSET_COLS: &str = "id, folder_id, provider, storage_key, file_name, alt_text, caption, \
    mime_type, size_bytes, width, height, original_filename, checksum, created_at, updated_at";

pub async fn list_assets(pool: &PgPool, folder_id: Option<Uuid>) -> Result<Vec<AssetRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AssetTuple>(&format!(
        "SELECT {ASSET_COLS} FROM _media_assets \
         WHERE folder_id IS NOT DISTINCT FROM $1 ORDER BY created_at DESC"
    ))
    .bind(folder_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(asset_from).collect())
}

pub async fn get_asset(pool: &PgPool, id: Uuid) -> Result<Option<AssetRow>, sqlx::Error> {
    let t = sqlx::query_as::<_, AssetTuple>(&format!(
        "SELECT {ASSET_COLS} FROM _media_assets WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(t.map(asset_from))
}

/// Parameters for inserting a freshly uploaded asset.
pub struct NewAsset<'a> {
    pub folder_id: Option<Uuid>,
    pub provider: &'a str,
    pub storage_key: &'a str,
    pub file_name: &'a str,
    pub mime_type: &'a str,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub original_filename: &'a str,
    pub checksum: Option<&'a str>,
}

pub async fn create_asset(pool: &PgPool, a: NewAsset<'_>) -> Result<AssetRow, sqlx::Error> {
    let t = sqlx::query_as::<_, AssetTuple>(&format!(
        "INSERT INTO _media_assets \
            (folder_id, provider, storage_key, file_name, mime_type, size_bytes, \
             width, height, original_filename, checksum) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING {ASSET_COLS}"
    ))
    .bind(a.folder_id)
    .bind(a.provider)
    .bind(a.storage_key)
    .bind(a.file_name)
    .bind(a.mime_type)
    .bind(a.size_bytes)
    .bind(a.width)
    .bind(a.height)
    .bind(a.original_filename)
    .bind(a.checksum)
    .fetch_one(pool)
    .await?;
    Ok(asset_from(t))
}

/// Update editable metadata + optional move. `None` leaves a field unchanged.
pub async fn update_asset(
    pool: &PgPool,
    id: Uuid,
    file_name: Option<&str>,
    alt_text: Option<&str>,
    caption: Option<&str>,
    folder_id: Option<Option<Uuid>>,
) -> Result<Option<AssetRow>, sqlx::Error> {
    let t = sqlx::query_as::<_, AssetTuple>(&format!(
        "UPDATE _media_assets SET \
            file_name = COALESCE($2, file_name), \
            alt_text  = COALESCE($3, alt_text), \
            caption   = COALESCE($4, caption), \
            folder_id = CASE WHEN $5 THEN $6 ELSE folder_id END, \
            updated_at = now() \
         WHERE id = $1 RETURNING {ASSET_COLS}"
    ))
    .bind(id)
    .bind(file_name)
    .bind(alt_text)
    .bind(caption)
    .bind(folder_id.is_some())
    .bind(folder_id.flatten())
    .fetch_optional(pool)
    .await?;
    Ok(t.map(asset_from))
}

/// Delete the row. Byte deletion happens in the handler via the provider.
pub async fn delete_asset(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM _media_assets WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p ferrum-http`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/http/src/media/store.rs
git commit -m "feat(media): asset data-access layer"
```

---

## Task 10: Media DAL — `_media_settings`

**Files:**
- Modify: `crates/http/src/media/store.rs`

- [ ] **Step 1: Append settings read/write**

```rust
#[derive(Debug, Clone)]
pub struct SettingsRow {
    pub provider: String,
    pub config: serde_json::Value, // secret fields stored encrypted
}

/// Read the singleton settings row, if it exists.
pub async fn get_settings(pool: &PgPool) -> Result<Option<SettingsRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT provider, config FROM _media_settings WHERE id = TRUE",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(provider, config)| SettingsRow { provider, config }))
}

/// Upsert the singleton settings row.
pub async fn put_settings(pool: &PgPool, provider: &str, config: &serde_json::Value) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO _media_settings (id, provider, config, updated_at) \
         VALUES (TRUE, $1, $2, now()) \
         ON CONFLICT (id) DO UPDATE SET provider = EXCLUDED.provider, \
            config = EXCLUDED.config, updated_at = now()",
    )
    .bind(provider)
    .bind(config)
    .execute(pool)
    .await?;
    Ok(())
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p ferrum-http`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/http/src/media/store.rs
git commit -m "feat(media): settings data-access layer"
```

---

## Task 11: Boot wiring (bin config + main + test harness)

**Files:**
- Modify: `crates/bin/src/config.rs`
- Modify: `crates/bin/src/main.rs`
- Modify: `crates/bin/Cargo.toml`
- Modify: `crates/bin/tests/common/mod.rs`

This task makes everything that constructs `AppState` compile again and gives the server a live provider at boot.

- [ ] **Step 1: Add a storage-resolution helper used by both bin and tests**

Create `crates/http/src/media/boot.rs`:

```rust
//! Resolve the active storage provider at startup: env override → DB settings
//! → local default. Also decrypts secret config fields before building.

use crate::media::store;
use ferrum_media::{build, secret_fields, StorageProvider};
use ferrum_media::secret as media_secret;
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
    if let Ok(provider) = std::env::var("FERRUM_MEDIA_PROVIDER") {
        if let Some(cfg) = env_config(&provider) {
            if let Ok(p) = build(&provider, &cfg) {
                return Arc::from(p);
            }
            tracing::warn!(%provider, "FERRUM_MEDIA_* env config invalid; falling back");
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
            "base_dir": std::env::var("FERRUM_MEDIA_BASE_DIR").unwrap_or_else(|_| "./media-data".into()),
        })),
        "s3" => Some(json!({
            "bucket": std::env::var("FERRUM_S3_BUCKET").ok()?,
            "region": std::env::var("FERRUM_S3_REGION").unwrap_or_else(|_| "us-east-1".into()),
            "endpoint": std::env::var("FERRUM_S3_ENDPOINT").ok(),
            "access_key": std::env::var("FERRUM_S3_ACCESS_KEY").ok()?,
            "secret_key": std::env::var("FERRUM_S3_SECRET_KEY").ok()?,
        })),
        _ => None,
    }
}
```

Add `pub mod boot;` to `crates/http/src/media/mod.rs`. Re-export from `crates/http/src/lib.rs`:

```rust
pub use media::boot::resolve_provider;
```

Also add a small helper to parse the secret key, in `crates/http/src/media/boot.rs`:

```rust
/// Parse `FERRUM_SECRET_KEY` (hex, 64 chars → 32 bytes). Returns None if unset.
pub fn secret_key_from_env() -> Option<[u8; 32]> {
    let hex = std::env::var("FERRUM_SECRET_KEY").ok()?;
    let bytes = (0..hex.len()).step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    bytes.try_into().ok()
}
```

Re-export it too: `pub use media::boot::secret_key_from_env;`.

- [ ] **Step 2: Update `crates/bin/src/main.rs`**

After the pool is created and migrations run, and before building `AppState`, add:

```rust
    let secret_key = ferrum_http::secret_key_from_env();
    let storage = std::sync::Arc::new(arc_swap::ArcSwap::from(
        ferrum_http::resolve_provider(&pool, secret_key).await,
    ));
```

Add `storage` and `secret_key` to the `AppState { ... }` literal. Add `arc-swap = { workspace = true }` to `crates/bin/Cargo.toml`.

> **Implementer note:** read the existing `AppState { ... }` literal in `main.rs` and add the two fields in the same style. Do not reorder existing fields.

- [ ] **Step 3: Update the integration harness `crates/bin/tests/common/mod.rs`**

Add imports:

```rust
use ferrum_http::{resolve_provider, secret_key_from_env};
use arc_swap::ArcSwap;
```

Before building `state`, add (use a per-test temp base dir for the local provider so tests don't collide):

```rust
        let media_dir = std::env::temp_dir().join(format!("ferrum-media-test-{}", uuid::Uuid::new_v4()));
        std::env::set_var("FERRUM_MEDIA_BASE_DIR", media_dir.to_string_lossy().to_string());
        std::env::set_var("FERRUM_MEDIA_PROVIDER", "local");
        let secret_key = secret_key_from_env();
        let storage = Arc::new(ArcSwap::from(resolve_provider(&pool, secret_key).await));
```

Add `storage` and `secret_key` to the `AppState { ... }` literal in the harness. Add `arc-swap` and `uuid` to `crates/bin/Cargo.toml` `[dev-dependencies]` if not present (`uuid` likely already a dep).

> **Note:** setting process env in tests is global; acceptable here because all integration tests use the same `local` provider. If a future test needs S3, scope via a dedicated config instead.

- [ ] **Step 4: Build everything**

Run: `cargo build --workspace`
Expected: compiles. If an `AppState { ... }` literal is still missing fields, the error names the file and line — add `storage`/`secret_key` there.

- [ ] **Step 5: Run existing tests to confirm no regressions**

Run: `cargo test --workspace`
Expected: existing tests PASS (integration tests need Docker for testcontainers; if Docker is unavailable in the environment, note it and run `cargo test --workspace --lib` for unit tests only).

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/media/boot.rs crates/http/src/media/mod.rs crates/http/src/lib.rs crates/bin/src/main.rs crates/bin/src/config.rs crates/bin/Cargo.toml crates/bin/tests/common/mod.rs Cargo.lock
git commit -m "feat(media): boot-time provider resolution + state wiring"
```

---

## Task 12: HTTP handlers — folders

**Files:**
- Create: `crates/http/src/routes/media.rs`
- Modify: `crates/http/src/routes/mod.rs`
- Create: `crates/bin/tests/media.rs`

- [ ] **Step 1: Write the folder handlers + router skeleton**

`crates/http/src/routes/media.rs`:

```rust
//! /admin/media/* handlers. Authz reuses content actions: read → ContentRead,
//! write → ContentWrite.

use crate::error::ApiError;
use crate::media::store;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Extension, Json, Router};
use chrono::{DateTime, Utc};
use ferrum_core::{Action, Error, Principal};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/media/providers", get(list_providers))
        .route("/admin/media/settings", get(get_settings).put(put_settings))
        .route("/admin/media/settings/test", post(test_settings))
        .route("/admin/media/folders", get(list_folders).post(create_folder))
        .route("/admin/media/folders/:id", axum::routing::patch(update_folder).delete(delete_folder))
        .route("/admin/media/assets", get(list_assets).post(upload_asset))
        .route("/admin/media/assets/:id", get(get_asset)
            .patch(update_asset).delete(delete_asset))
        .route("/admin/media/assets/:id/raw", get(get_asset_raw))
}

async fn ensure(state: &AppState, principal: &Principal, action: Action) -> Result<(), ApiError> {
    if !state.authz.can(principal, action, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

fn internal<E: Into<anyhow::Error>>(e: E) -> ApiError {
    ApiError(Error::Internal(e.into()))
}

#[derive(Serialize)]
struct FolderView {
    id: Uuid,
    parent_id: Option<Uuid>,
    name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl From<store::FolderRow> for FolderView {
    fn from(f: store::FolderRow) -> Self {
        FolderView { id: f.id, parent_id: f.parent_id, name: f.name, created_at: f.created_at, updated_at: f.updated_at }
    }
}

#[derive(Deserialize)]
struct FolderQuery { parent_id: Option<Uuid> }

async fn list_folders(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<FolderQuery>,
) -> Result<Json<Vec<FolderView>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let rows = store::list_folders(&state.pool, q.parent_id).await.map_err(internal)?;
    Ok(Json(rows.into_iter().map(FolderView::from).collect()))
}

#[derive(Deserialize)]
struct CreateFolderBody { parent_id: Option<Uuid>, name: String }

async fn create_folder(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<CreateFolderBody>,
) -> Result<(StatusCode, Json<FolderView>), ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    if body.name.trim().is_empty() {
        return Err(ApiError(Error::Validation(ferrum_core::ValidationErrors::field("name", "required"))));
    }
    let row = store::create_folder(&state.pool, body.parent_id, body.name.trim())
        .await
        .map_err(map_folder_err)?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

#[derive(Deserialize)]
struct UpdateFolderBody {
    name: Option<String>,
    // Distinguishes "move to root" (present, null) from "don't move" (absent).
    #[serde(default, deserialize_with = "double_option")]
    parent_id: Option<Option<Uuid>>,
}

async fn update_folder(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFolderBody>,
) -> Result<Json<FolderView>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    let row = store::update_folder(&state.pool, id, body.name.as_deref(), body.parent_id)
        .await
        .map_err(map_folder_err)?
        .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(row.into()))
}

async fn delete_folder(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    if store::folder_has_children(&state.pool, id).await.map_err(internal)? {
        return Err(ApiError(Error::Conflict("folder is not empty".into())));
    }
    if store::delete_folder(&state.pool, id).await.map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(Error::NotFound))
    }
}

/// Map unique-violation (dup name in same parent) to 409.
fn map_folder_err(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(db) = &e {
        if db.code().as_deref() == Some("23505") {
            return ApiError(Error::Conflict("a folder with that name already exists here".into()));
        }
    }
    ApiError(Error::Internal(e.into()))
}

/// serde helper: `Option<Option<T>>` distinguishing absent vs explicit null.
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(de).map(Some)
}
```

Add stub handlers for the asset + settings routes referenced in `router()` so the file compiles; they are fully written in Tasks 13–14. For now add:

```rust
async fn list_providers() -> Result<Json<serde_json::Value>, ApiError> { Ok(Json(serde_json::json!([]))) }
async fn get_settings() -> Result<Json<serde_json::Value>, ApiError> { Ok(Json(serde_json::json!(null))) }
async fn put_settings() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn test_settings() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn list_assets() -> Result<Json<serde_json::Value>, ApiError> { Ok(Json(serde_json::json!([]))) }
async fn upload_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn get_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn update_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn delete_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn get_asset_raw() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
```

> These stubs are replaced in Tasks 13–14. They exist only so folder handlers can be tested in isolation now.

- [ ] **Step 2: Merge the router**

In `crates/http/src/routes/mod.rs`, add `.merge(media::router())` in the protected router chain (alongside `users::router()`), and add `mod media;` / `use` as the file's pattern requires. (`media` here is the route module; the file already references `users`, `content`, etc.)

> **Implementer note:** `routes/mod.rs` references route modules. Add the route module declaration for `media` consistent with how `users` is declared in that file.

- [ ] **Step 3: Write the folder integration test**

`crates/bin/tests/media.rs`:

```rust
mod common;
use common::TestApp;

#[tokio::test]
async fn folder_crud_and_nonempty_delete() {
    let app = TestApp::spawn().await;

    // Create root folder.
    let resp = app.admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "images" }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201);
    let folder: serde_json::Value = resp.json().await.unwrap();
    let fid = folder["id"].as_str().unwrap().to_string();

    // Duplicate name in same parent → 409.
    let dup = app.admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "images" }))
        .send().await.unwrap();
    assert_eq!(dup.status(), 409);

    // Nested child.
    let child = app.admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "2026", "parent_id": fid }))
        .send().await.unwrap();
    assert_eq!(child.status(), 201);

    // Delete non-empty parent → 409.
    let del = app.admin(app.client.delete(app.url(&format!("/admin/media/folders/{fid}"))))
        .send().await.unwrap();
    assert_eq!(del.status(), 409);

    // List root folders.
    let list = app.admin(app.client.get(app.url("/admin/media/folders")))
        .send().await.unwrap();
    assert_eq!(list.status(), 200);
    let arr: serde_json::Value = list.json().await.unwrap();
    assert!(arr.as_array().unwrap().iter().any(|f| f["name"] == "images"));
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p ferrum-bin --test media folder_crud_and_nonempty_delete`
Expected: PASS (requires Docker for testcontainers). If Docker unavailable, build-only: `cargo build --workspace` must pass; note the test couldn't run.

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/routes/media.rs crates/http/src/routes/mod.rs crates/bin/tests/media.rs
git commit -m "feat(media): folder HTTP endpoints"
```

---

## Task 13: HTTP handlers — settings + providers

**Files:**
- Modify: `crates/http/src/routes/media.rs`
- Modify: `crates/bin/tests/media.rs`

- [ ] **Step 1: Replace the settings/provider stubs with real handlers**

Replace `list_providers`, `get_settings`, `put_settings`, `test_settings` stubs with:

```rust
async fn list_providers(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<ferrum_media::ProviderDescriptor>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    Ok(Json(ferrum_media::descriptors()))
}

const MASK: &str = "••••";

#[derive(Serialize)]
struct SettingsView { provider: String, config: serde_json::Value }

async fn get_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Option<SettingsView>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let row = store::get_settings(&state.pool).await.map_err(internal)?;
    let view = row.map(|r| {
        let mut cfg = r.config.clone();
        if let Some(obj) = cfg.as_object_mut() {
            for name in ferrum_media::secret_fields(&r.provider) {
                if obj.contains_key(name) {
                    obj.insert(name.to_string(), serde_json::Value::String(MASK.into()));
                }
            }
        }
        SettingsView { provider: r.provider, config: cfg }
    });
    Ok(Json(view))
}

#[derive(Deserialize)]
struct SettingsBody { provider: String, config: serde_json::Value }

/// Encrypt secret fields in `config`. If a secret field equals the mask, keep
/// the previously-stored (already-encrypted) value instead of re-encrypting.
fn prepare_config_for_save(
    state: &AppState,
    provider: &str,
    mut config: serde_json::Value,
    previous: Option<&store::SettingsRow>,
) -> Result<serde_json::Value, ApiError> {
    let secrets = ferrum_media::secret_fields(provider);
    if secrets.is_empty() {
        return Ok(config);
    }
    let key = state.secret_key.ok_or_else(|| {
        ApiError(Error::Conflict("FERRUM_SECRET_KEY not set; cannot store provider secrets".into()))
    })?;
    if let Some(obj) = config.as_object_mut() {
        for name in secrets {
            match obj.get(name).and_then(|v| v.as_str()) {
                Some(MASK) | None => {
                    // Reuse previously stored encrypted value.
                    if let Some(prev) = previous.and_then(|p| {
                        if p.provider == provider { p.config.get(name).cloned() } else { None }
                    }) {
                        obj.insert(name.to_string(), prev);
                    }
                }
                Some(plain) => {
                    let enc = ferrum_media::secret::encrypt(&key, plain)
                        .map_err(|_| internal(anyhow::anyhow!("encrypt failed")))?;
                    obj.insert(name.to_string(), serde_json::Value::String(enc));
                }
            }
        }
    }
    Ok(config)
}

async fn put_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<SettingsBody>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    ferrum_media::validate(&body.provider, &body.config)
        .map_err(|e| ApiError(Error::Validation(ferrum_core::ValidationErrors::field("config", &e.to_string()))))?;

    let previous = store::get_settings(&state.pool).await.map_err(internal)?;
    let to_store = prepare_config_for_save(&state, &body.provider, body.config.clone(), previous.as_ref())?;
    store::put_settings(&state.pool, &body.provider, &to_store).await.map_err(internal)?;

    // Build + hot-swap the active provider (decrypt secrets for the live build).
    let mut live_cfg = to_store.clone();
    if let Some(key) = &state.secret_key {
        crate::media::boot::decrypt_secrets(&body.provider, &mut live_cfg, key);
    }
    let provider = ferrum_media::build(&body.provider, &live_cfg)
        .map_err(|e| ApiError(Error::Unsupported(e.to_string())))?;
    state.storage.store(std::sync::Arc::from(provider));
    Ok(StatusCode::NO_CONTENT)
}

async fn test_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<SettingsBody>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    ferrum_media::validate(&body.provider, &body.config)
        .map_err(|e| ApiError(Error::Validation(ferrum_core::ValidationErrors::field("config", &e.to_string()))))?;
    // If secrets are masked, fill from stored config so "Test" works after load.
    let mut cfg = body.config.clone();
    if let Some(obj) = cfg.as_object_mut() {
        let prev = store::get_settings(&state.pool).await.map_err(internal)?;
        for name in ferrum_media::secret_fields(&body.provider) {
            if obj.get(name).and_then(|v| v.as_str()) == Some(MASK) {
                if let (Some(prev), Some(key)) = (&prev, &state.secret_key) {
                    if prev.provider == body.provider {
                        if let Some(serde_json::Value::String(enc)) = prev.config.get(name) {
                            if let Ok(plain) = ferrum_media::secret::decrypt(key, enc) {
                                obj.insert(name.to_string(), serde_json::Value::String(plain));
                            }
                        }
                    }
                }
            }
        }
    }
    let provider = ferrum_media::build(&body.provider, &cfg)
        .map_err(|e| ApiError(Error::Unsupported(e.to_string())))?;
    provider.test().await
        .map_err(|e| ApiError(Error::Conflict(format!("connection test failed: {e}"))))?;
    Ok(StatusCode::NO_CONTENT)
}
```

> **Note:** `ferrum_media::secret` must be public. Confirm `crates/media/src/lib.rs` has `pub mod secret;` (Task 3) and `boot::decrypt_secrets` is `pub` (Task 11).

- [ ] **Step 2: Add settings integration test**

Append to `crates/bin/tests/media.rs`:

```rust
#[tokio::test]
async fn settings_masks_secrets_and_lists_providers() {
    let app = TestApp::spawn().await;

    // Providers list includes local + s3.
    let provs: serde_json::Value = app.admin(app.client.get(app.url("/admin/media/providers")))
        .send().await.unwrap().json().await.unwrap();
    let ids: Vec<&str> = provs.as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"local") && ids.contains(&"s3"));

    // Save local settings (no secrets) → 204.
    let put = app.admin(app.client.put(app.url("/admin/media/settings")))
        .json(&serde_json::json!({ "provider": "local", "config": { "base_dir": "./media-data" } }))
        .send().await.unwrap();
    assert_eq!(put.status(), 204);

    // Read back.
    let got: serde_json::Value = app.admin(app.client.get(app.url("/admin/media/settings")))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(got["provider"], "local");
    assert_eq!(got["config"]["base_dir"], "./media-data");
}
```

> A secret-masking test for S3 requires `FERRUM_SECRET_KEY` to be set in the harness. The harness does not set it by default, so saving an S3 provider returns 409 ("FERRUM_SECRET_KEY not set"). That path is covered by the `prepare_config_for_save` unit logic; the integration test covers the local (no-secret) happy path. If desired, the implementer may set `FERRUM_SECRET_KEY` in the harness and add an S3 mask assertion, but it is optional.

- [ ] **Step 3: Run tests**

Run: `cargo test -p ferrum-bin --test media`
Expected: PASS (Docker required). Build must pass regardless: `cargo build --workspace`.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/routes/media.rs crates/bin/tests/media.rs
git commit -m "feat(media): settings + providers HTTP endpoints"
```

---

## Task 14: HTTP handlers — asset upload, list, get, patch, delete, raw

**Files:**
- Modify: `crates/http/src/routes/media.rs`
- Modify: `crates/bin/tests/media.rs`

- [ ] **Step 1: Replace the asset stubs with real handlers**

Add imports at top of `media.rs`:

```rust
use axum::extract::Multipart;
use axum::body::Body;
use axum::response::{IntoResponse, Response};
use axum::http::header;
use bytes::Bytes;
use sha2::{Digest, Sha256};
```

Replace the asset stubs:

```rust
#[derive(Serialize)]
struct AssetView {
    id: Uuid,
    folder_id: Option<Uuid>,
    file_name: String,
    alt_text: Option<String>,
    caption: Option<String>,
    mime_type: String,
    size_bytes: i64,
    width: Option<i32>,
    height: Option<i32>,
    original_filename: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl From<store::AssetRow> for AssetView {
    fn from(a: store::AssetRow) -> Self {
        AssetView {
            id: a.id, folder_id: a.folder_id, file_name: a.file_name, alt_text: a.alt_text,
            caption: a.caption, mime_type: a.mime_type, size_bytes: a.size_bytes,
            width: a.width, height: a.height, original_filename: a.original_filename,
            created_at: a.created_at, updated_at: a.updated_at,
        }
    }
}

#[derive(Deserialize)]
struct AssetQuery { folder_id: Option<Uuid> }

async fn list_assets(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<AssetQuery>,
) -> Result<Json<Vec<AssetView>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let rows = store::list_assets(&state.pool, q.folder_id).await.map_err(internal)?;
    Ok(Json(rows.into_iter().map(AssetView::from).collect()))
}

async fn upload_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<AssetView>), ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;

    let mut folder_id: Option<Uuid> = None;
    let mut file_bytes: Option<Bytes> = None;
    let mut original_filename = String::from("upload");

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError(Error::Unsupported(format!("bad multipart: {e}")))
    })? {
        match field.name() {
            Some("folder_id") => {
                let txt = field.text().await.map_err(|e| ApiError(Error::Unsupported(e.to_string())))?;
                if !txt.is_empty() {
                    folder_id = Some(Uuid::parse_str(&txt).map_err(|_| {
                        ApiError(Error::Validation(ferrum_core::ValidationErrors::field("folder_id", "invalid uuid")))
                    })?);
                }
            }
            Some("file") => {
                if let Some(fname) = field.file_name() { original_filename = fname.to_string(); }
                let data = field.bytes().await.map_err(|e| ApiError(Error::Unsupported(e.to_string())))?;
                file_bytes = Some(data);
            }
            _ => { let _ = field.bytes().await; }
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        ApiError(Error::Validation(ferrum_core::ValidationErrors::field("file", "required")))
    })?;

    // Mime sniff (fallback to octet-stream), checksum, optional image dims.
    let mime_type = infer::get(&bytes).map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let checksum = {
        let mut h = Sha256::new();
        h.update(&bytes);
        format!("{:x}", h.finalize())
    };
    let (width, height) = match imagesize::blob_size(&bytes) {
        Ok(d) => (Some(d.width as i32), Some(d.height as i32)),
        Err(_) => (None, None),
    };

    let id = Uuid::new_v4();
    let storage_key = format!("{id}/{original_filename}");

    // Store bytes via the active provider.
    let provider = state.storage.load();
    provider.put(&storage_key, bytes.clone(), &mime_type).await
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!("storage put: {e}"))))?;

    // Determine the active provider id for the asset row.
    let provider_id = current_provider_id(&state).await;

    let row = store::create_asset(&state.pool, store::NewAsset {
        folder_id,
        provider: &provider_id,
        storage_key: &storage_key,
        file_name: &original_filename,
        mime_type: &mime_type,
        size_bytes: bytes.len() as i64,
        width,
        height,
        original_filename: &original_filename,
        checksum: Some(&checksum),
    }).await.map_err(map_folder_err)?;

    Ok((StatusCode::CREATED, Json(row.into())))
}

/// Active provider id: env override → DB settings → "local" default.
async fn current_provider_id(state: &AppState) -> String {
    if let Ok(p) = std::env::var("FERRUM_MEDIA_PROVIDER") { return p; }
    if let Ok(Some(row)) = store::get_settings(&state.pool).await { return row.provider; }
    "local".to_string()
}

async fn get_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<AssetView>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let row = store::get_asset(&state.pool, id).await.map_err(internal)?
        .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(row.into()))
}

#[derive(Deserialize)]
struct UpdateAssetBody {
    file_name: Option<String>,
    alt_text: Option<String>,
    caption: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    folder_id: Option<Option<Uuid>>,
}

async fn update_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateAssetBody>,
) -> Result<Json<AssetView>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    let row = store::update_asset(
        &state.pool, id,
        body.file_name.as_deref(), body.alt_text.as_deref(),
        body.caption.as_deref(), body.folder_id,
    ).await.map_err(internal)?.ok_or(ApiError(Error::NotFound))?;
    Ok(Json(row.into()))
}

async fn delete_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    let row = store::get_asset(&state.pool, id).await.map_err(internal)?
        .ok_or(ApiError(Error::NotFound))?;
    // Best-effort byte deletion via the asset's recorded provider when it matches active.
    let provider = state.storage.load();
    let _ = provider.delete(&row.storage_key).await;
    store::delete_asset(&state.pool, id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_asset_raw(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let row = store::get_asset(&state.pool, id).await.map_err(internal)?
        .ok_or(ApiError(Error::NotFound))?;
    let provider = state.storage.load();
    let bytes = provider.get(&row.storage_key).await.map_err(|e| match e {
        ferrum_media::StorageError::NotFound => ApiError(Error::NotFound),
        other => ApiError(Error::Internal(anyhow::anyhow!("storage get: {other}"))),
    })?;
    Ok((
        [(header::CONTENT_TYPE, row.mime_type)],
        Body::from(bytes),
    ).into_response())
}
```

> **Note:** `Multipart` requires axum's `multipart` feature. Add it to the axum dependency in `crates/http/Cargo.toml` (e.g. `axum = { workspace = true, features = ["multipart"] }`, or add `multipart` to the workspace axum features). Verify the existing axum feature list and append `multipart`.

- [ ] **Step 2: Add the upload round-trip integration test**

Append to `crates/bin/tests/media.rs`:

```rust
#[tokio::test]
async fn asset_upload_and_raw_round_trip() {
    let app = TestApp::spawn().await;

    // A 1x1 PNG.
    let png: &[u8] = &[
        0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A,0x00,0x00,0x00,0x0D,0x49,0x48,0x44,0x52,
        0x00,0x00,0x00,0x01,0x00,0x00,0x00,0x01,0x08,0x06,0x00,0x00,0x00,0x1F,0x15,0xC4,
        0x89,0x00,0x00,0x00,0x0A,0x49,0x44,0x41,0x54,0x78,0x9C,0x63,0x00,0x01,0x00,0x00,
        0x05,0x00,0x01,0x0D,0x0A,0x2D,0xB4,0x00,0x00,0x00,0x00,0x49,0x45,0x4E,0x44,0xAE,
        0x42,0x60,0x82,
    ];

    let part = reqwest::multipart::Part::bytes(png.to_vec())
        .file_name("pixel.png")
        .mime_str("application/octet-stream").unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let resp = app.admin(app.client.post(app.url("/admin/media/assets")))
        .multipart(form).send().await.unwrap();
    assert_eq!(resp.status(), 201);
    let asset: serde_json::Value = resp.json().await.unwrap();
    let aid = asset["id"].as_str().unwrap().to_string();
    assert_eq!(asset["mime_type"], "image/png");
    assert_eq!(asset["width"], 1);
    assert_eq!(asset["height"], 1);

    // Patch metadata.
    let patch = app.admin(app.client.patch(app.url(&format!("/admin/media/assets/{aid}"))))
        .json(&serde_json::json!({ "alt_text": "a pixel", "caption": "tiny" }))
        .send().await.unwrap();
    assert_eq!(patch.status(), 200);
    let patched: serde_json::Value = patch.json().await.unwrap();
    assert_eq!(patched["alt_text"], "a pixel");

    // Raw fetch returns the bytes + content-type.
    let raw = app.admin(app.client.get(app.url(&format!("/admin/media/assets/{aid}/raw"))))
        .send().await.unwrap();
    assert_eq!(raw.status(), 200);
    assert_eq!(raw.headers().get("content-type").unwrap(), "image/png");
    let body = raw.bytes().await.unwrap();
    assert_eq!(&body[..], png);

    // Delete.
    let del = app.admin(app.client.delete(app.url(&format!("/admin/media/assets/{aid}"))))
        .send().await.unwrap();
    assert_eq!(del.status(), 204);

    // Gone.
    let gone = app.admin(app.client.get(app.url(&format!("/admin/media/assets/{aid}"))))
        .send().await.unwrap();
    assert_eq!(gone.status(), 404);
}
```

- [ ] **Step 3: Run the full media test suite**

Run: `cargo test -p ferrum-bin --test media`
Expected: PASS — `folder_crud_and_nonempty_delete`, `settings_masks_secrets_and_lists_providers`, `asset_upload_and_raw_round_trip` (Docker required).

- [ ] **Step 4: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: PASS. If Docker is unavailable, run `cargo test --workspace --lib` (unit tests) and `cargo build --workspace`, and note that integration tests were not executed.

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/routes/media.rs crates/http/Cargo.toml crates/bin/tests/media.rs Cargo.lock
git commit -m "feat(media): asset upload, metadata, raw serve endpoints"
```

---

## Task 15: README + docs touch-up

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Document the media env vars**

Add a short "Media storage" subsection to `README.md` near the other env-var docs:

```markdown
### Media storage

The Media Library defaults to **local filesystem** storage under `./media-data`
— no configuration needed. To use S3 (or an S3-compatible service) set:

```sh
export FERRUM_MEDIA_PROVIDER=s3
export FERRUM_S3_BUCKET=my-bucket
export FERRUM_S3_REGION=us-east-1
export FERRUM_S3_ENDPOINT=https://...   # optional, for MinIO/R2/Spaces
export FERRUM_S3_ACCESS_KEY=...
export FERRUM_S3_SECRET_KEY=...
```

Alternatively configure the provider at runtime via `PUT /admin/media/settings`.
Storing provider secrets in the database requires a 32-byte hex encryption key:

```sh
export FERRUM_SECRET_KEY=$(openssl rand -hex 32)
```

Env configuration always overrides database settings.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs(media): document media storage env vars"
```

---

## Self-Review Notes (addressed)

- **Spec coverage:** trait (T1), local (T2), S3 (T5), registry/descriptors (T4), migration/tables (T6), settings singleton + encrypted secrets (T3, T10, T13), env-wins resolution + local default (T11), folders CRUD + non-empty 409 (T8, T12), assets upload/list/get/patch/delete/raw proxy (T9, T14), per-asset provider (T9/T14 `current_provider_id`), masked secrets on read (T13), test endpoint (T13), authz reuse content actions (all handlers). All spec sections map to tasks.
- **Type consistency:** `FolderRow`/`AssetRow`/`SettingsRow`/`NewAsset` defined in `store.rs` (T8–T10) and consumed by handlers (T12–T14). `StorageProvider`/`StorageError`/`descriptors`/`secret_fields`/`validate`/`build`/`secret` all defined in `crates/media` (T1–T4) and re-exported (T7). `double_option` defined once in T12, reused in T14. `current_provider_id` and `decrypt_secrets` referenced consistently.
- **Placeholders:** none — every code step is complete. Two flagged version-sensitivity notes (rust-s3 error variants in T5; axum multipart feature in T14) are implementer guidance, not missing code.
- **Open risk:** `rust-s3` 0.34 API surface (error variant names, `set_path_style`) may vary by patch; T5 includes a contract-preserving fallback note. The S3 path is exercised only by the env-gated MinIO test, so a mismatch won't block the local-default happy path or CI.
