//! Library facet of the ferrum binary, exposing modules that integration
//! tests need to call directly (e.g. seeding).
pub mod audit_sink;
pub mod config;
pub mod migrate;
pub mod webhook_worker;
