#![forbid(unsafe_code)]

pub mod bind;
pub mod registry;

pub use registry::SchemaRegistry;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
