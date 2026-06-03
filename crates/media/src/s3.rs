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
