#![forbid(unsafe_code)]

pub mod bind;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
