#![forbid(unsafe_code)]

pub mod error;
pub mod middleware;
pub mod query;
pub mod routes;
pub mod state;

pub use error::ApiError;
pub use routes::build_router;
pub use state::{AlwaysAllow, AppConfig, AppState, Authz, EventSink, NoopSink};
