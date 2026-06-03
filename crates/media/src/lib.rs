//! Pluggable media storage for rustapi.

pub mod local;
pub mod provider;

pub use local::LocalProvider;
pub use provider::{StorageError, StorageProvider};
