//! Pluggable media storage for rustapi.

pub mod local;
pub mod provider;
pub mod registry;
pub mod s3;
pub mod secret;

pub use local::LocalProvider;
pub use provider::{StorageError, StorageProvider};
pub use registry::{build, descriptors, descriptor_for, secret_fields, validate, ConfigField, ProviderDescriptor};
