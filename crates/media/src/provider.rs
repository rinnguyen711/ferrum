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
