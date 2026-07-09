//! Pluggable media storage for ferrum.

pub mod local;
pub mod provider;
pub mod registry;
pub mod s3;
pub mod secret;

pub use local::LocalProvider;
pub use provider::{StorageError, StorageProvider};
pub use registry::{
    build, descriptor_for, descriptors, secret_fields, validate, ConfigField, ProviderDescriptor,
};
