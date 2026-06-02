//! Env-first configuration loader.

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Config {
    pub database_url: String,
    pub admin_key: String,
    pub bind: String,
    pub log: String,
    pub page_size_max: u32,
    /// When set, the built admin UI in this directory is served at /studio.
    /// Unset → no UI route is mounted (API-only server).
    pub studio_dir: Option<String>,
    /// When true (default), an empty DB is seeded with default content types
    /// and sample data at startup. Set RUSTAPI_SEED=false to disable.
    pub seed: bool,
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
        let studio_dir = std::env::var("RUSTAPI_STUDIO_DIR").ok().filter(|s| !s.is_empty());
        let seed = std::env::var("RUSTAPI_SEED")
            .ok()
            .map(|s| !matches!(s.as_str(), "0" | "false" | "no"))
            .unwrap_or(true);
        Ok(Self {
            database_url,
            admin_key,
            bind,
            log,
            page_size_max,
            studio_dir,
            seed,
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
