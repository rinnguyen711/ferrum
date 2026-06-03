//! Pluggable media storage for rustapi.

pub mod provider;

pub use provider::{StorageError, StorageProvider};
