//! Env-first configuration loader.

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_ttl_secs: i64,
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
        let jwt_secret = std::env::var("RUSTAPI_JWT_SECRET")
            .context("RUSTAPI_JWT_SECRET must be set")?;
        if jwt_secret.len() < 32 {
            return Err(anyhow!("RUSTAPI_JWT_SECRET must be at least 32 characters"));
        }
        let jwt_ttl_secs = std::env::var("RUSTAPI_JWT_TTL_SECS")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(86400);
        let bind = std::env::var("RUSTAPI_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
        let log = std::env::var("RUSTAPI_LOG").unwrap_or_else(|_| "info".into());
        let page_size_max = std::env::var("RUSTAPI_PAGE_SIZE_MAX")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(100);
        let studio_dir = std::env::var("RUSTAPI_STUDIO_DIR").ok().filter(|s| !s.is_empty());
        let seed = std::env::var("RUSTAPI_SEED")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| !matches!(s.as_str(), "0" | "false" | "no"))
            .unwrap_or(true);
        Ok(Self {
            database_url,
            jwt_secret,
            jwt_ttl_secs,
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
    fn rejects_short_jwt_secret() {
        std::env::set_var("DATABASE_URL", "postgres://x");
        std::env::set_var("RUSTAPI_JWT_SECRET", "short");
        let err = Config::from_env().unwrap_err();
        assert!(err.to_string().contains("at least 32"));
    }
}
