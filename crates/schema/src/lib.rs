#![forbid(unsafe_code)]

pub mod bind;
pub mod component;
pub mod registry;
pub mod service;
pub mod sync;

pub use component::{ComponentRegistry, ComponentService};
pub use registry::SchemaRegistry;
pub use service::SchemaService;
pub use sync::{sync_from_path, SyncMode};

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
