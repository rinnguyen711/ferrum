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
