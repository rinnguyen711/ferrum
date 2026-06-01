#![forbid(unsafe_code)]

pub mod entry;
pub mod error;
pub mod filter;
pub mod middleware;
pub mod populate;
pub mod query;
pub mod routes;
pub mod state;

pub use error::ApiError;
pub use routes::{build_router, mount_studio};
pub use state::{AlwaysAllow, AppConfig, AppState, Authz, EventSink, NoopSink};
