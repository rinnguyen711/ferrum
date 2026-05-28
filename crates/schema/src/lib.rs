#![forbid(unsafe_code)]

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
