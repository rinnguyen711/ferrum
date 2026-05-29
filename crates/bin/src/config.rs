//! Env-first configuration loader.

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub admin_key: String,
    pub bind: String,
    pub log: String,
    pub page_size_max: u32,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL must be set")?;
        let admin_key = std::env::var("RUSTAPI_ADMIN_KEY")
            .context("RUSTAPI_ADMIN_KEY must be set")?;
        if admin_key.len() < 32 {
            return Err(anyhow!("RUSTAPI_ADMIN_KEY must be at least 32 characters"));
        }
        let bind = std::env::var("RUSTAPI_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
        let log = std::env::var("RUSTAPI_LOG").unwrap_or_else(|_| "info".into());
        let page_size_max = std::env::var("RUSTAPI_PAGE_SIZE_MAX")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(100);
        Ok(Self {
            database_url,
            admin_key,
            bind,
            log,
            page_size_max,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_key() {
        std::env::set_var("DATABASE_URL", "postgres://x");
        std::env::set_var("RUSTAPI_ADMIN_KEY", "short");
        let err = Config::from_env().unwrap_err();
        assert!(err.to_string().contains("at least 32"));
    }
}
