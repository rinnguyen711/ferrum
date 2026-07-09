//! Amazon S3 (and S3-compatible) provider backed by the `rust-s3` crate.

use crate::provider::{StorageError, StorageProvider};
use async_trait::async_trait;
use bytes::Bytes;
use s3::creds::Credentials;
use s3::error::S3Error;
use s3::{Bucket, Region};
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
            None => cfg
                .region
                .parse()
                .map_err(|_| StorageError::Other("bad region".into()))?,
        };

        let creds = Credentials::new(
            Some(&cfg.access_key),
            Some(&cfg.secret_key),
            None,
            None,
            None,
        )
        .map_err(|e| StorageError::Auth(e.to_string()))?;

        let bucket = Bucket::new(&cfg.bucket, region, creds)
            .map_err(|e| StorageError::Other(e.to_string()))?;

        // Use path-style addressing for S3-compatible endpoints (e.g. MinIO).
        let bucket = if cfg.endpoint.is_some() {
            bucket.with_path_style()
        } else {
            bucket
        };

        Ok(Self { bucket })
    }
}

/// Map an `S3Error` to a `StorageError` following the behavior contract:
/// 404 → NotFound, 403 → Auth, other HTTP/transport → Connection, else → Other.
fn map_err(e: S3Error) -> StorageError {
    match &e {
        S3Error::HttpFailWithBody(status, _) => map_status(*status, e),
        S3Error::HttpFail => StorageError::Connection(e.to_string()),
        S3Error::Reqwest(_) => StorageError::Connection(e.to_string()),
        S3Error::Io(_) => StorageError::Connection(e.to_string()),
        _ => StorageError::Other(e.to_string()),
    }
}

fn map_status(status: u16, e: S3Error) -> StorageError {
    match status {
        404 => StorageError::NotFound,
        403 => StorageError::Auth(e.to_string()),
        _ => StorageError::Connection(e.to_string()),
    }
}

#[async_trait]
impl StorageProvider for S3Provider {
    async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<(), StorageError> {
        self.bucket
            .put_object_with_content_type(key, &bytes, content_type)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Bytes, StorageError> {
        let resp = self.bucket.get_object(key).await.map_err(|e| {
            // get_object raises S3Error::HttpFailWithBody for HTTP errors
            map_err(e)
        })?;
        let status = resp.status_code();
        if status == 404 {
            return Err(StorageError::NotFound);
        }
        if status >= 400 {
            return Err(StorageError::Connection(format!("HTTP {status}")));
        }
        Ok(resp.into_bytes())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.bucket.delete_object(key).await.map_err(map_err)?;
        Ok(())
    }

    async fn test(&self) -> Result<(), StorageError> {
        // HEAD a key that should never exist; a 404 proves creds + bucket are reachable.
        match self.bucket.head_object("__ferrum_healthcheck__").await {
            Ok(_) => Ok(()),
            Err(S3Error::HttpFailWithBody(404, _)) => Ok(()),
            Err(S3Error::HttpFailWithBody(403, msg)) => {
                Err(StorageError::Auth(format!("403 {msg}")))
            }
            Err(e) => Err(map_err(e)),
        }
    }
}
