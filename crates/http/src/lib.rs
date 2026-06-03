#![forbid(unsafe_code)]

pub mod auth;
pub mod entry;
pub mod error;
pub mod filter;
pub mod media;
pub mod middleware;
pub mod populate;
pub mod query;
pub mod routes;
pub mod state;

pub use error::ApiError;
pub use media::boot::{resolve_provider, secret_key_from_env};
pub use routes::{build_router, mount_studio};
pub use rustapi_media::{descriptors, LocalProvider, StorageProvider};
pub use state::{AlwaysAllow, AppConfig, AppState, Authz, EventSink, NoopSink, RoleAuthz};
