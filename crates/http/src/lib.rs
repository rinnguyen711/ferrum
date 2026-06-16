#![forbid(unsafe_code)]

pub mod auth;
pub mod content_api;
pub mod cursor;
pub mod entry;
pub mod error;
pub mod filter;
pub mod graphql;
pub mod media;
pub mod media_embed;
pub mod middleware;
pub mod openapi;
pub mod populate;
pub mod query;
pub mod reqctx;
pub mod roles;
pub mod routes;
pub mod state;

pub use error::ApiError;
pub use media::boot::{resolve_provider, secret_key_from_env};
pub use roles::RoleRegistry;
pub use routes::{build_router, mount_studio};
pub use rustapi_media::{descriptors, LocalProvider, StorageProvider};
pub use state::{
    AlwaysAllow, AppConfig, AppState, AuditSink, Authz, EventSink, NoopAuditSink, NoopHook,
    NoopSink, RoleAuthz, WriteContext, WriteHook, WriteOp,
};
