#![forbid(unsafe_code)]

pub mod bind;
pub mod registry;
pub mod service;

pub use registry::SchemaRegistry;
pub use service::SchemaService;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
