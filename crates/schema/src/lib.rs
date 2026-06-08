#![forbid(unsafe_code)]

pub mod bind;
pub mod component;
pub mod registry;
pub mod service;

pub use component::{ComponentRegistry, ComponentService};
pub use registry::SchemaRegistry;
pub use service::SchemaService;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
