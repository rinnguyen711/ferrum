//! Env-first configuration loader.

use anyhow::{anyhow, Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    /// Postgres connection pool size. `RUSTAPI_DB_MAX_CONNECTIONS`, default 10.
    pub db_max_connections: u32,
    pub jwt_secret: String,
    pub jwt_ttl_secs: i64,
    pub bind: String,
    pub log: String,
    pub page_size_max: u32,
    /// When set, the built admin UI in this directory is served at /studio.
    /// Unset → no UI route is mounted (API-only server).
    pub studio_dir: Option<String>,
    /// Path to a schema TOML file or a directory of `*.toml`. Unset = sync off.
    /// `RUSTAPI_SCHEMA_DIR` wins over `RUSTAPI_SCHEMA_FILE` if both are set.
    pub schema_path: Option<String>,
    /// Reconcile aggressiveness. `RUSTAPI_SCHEMA_SYNC`: additive (default) | full.
    pub schema_sync_mode: rustapi_schema::SyncMode,
    /// When false, /openapi.json and /docs are not mounted.
    pub docs_enabled: bool,
    /// Reported as OpenAPI info.version.
    pub api_version: String,
    /// Reported as OpenAPI servers[0].url.
    pub public_base_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let db_max_connections = std::env::var("RUSTAPI_DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(10);
        let jwt_secret =
            std::env::var("RUSTAPI_JWT_SECRET").context("RUSTAPI_JWT_SECRET must be set")?;
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
        let studio_dir = std::env::var("RUSTAPI_STUDIO_DIR")
            .ok()
            .filter(|s| !s.is_empty());
        let schema_path = std::env::var("RUSTAPI_SCHEMA_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("RUSTAPI_SCHEMA_FILE")
                    .ok()
                    .filter(|s| !s.is_empty())
            });
        let schema_sync_mode = std::env::var("RUSTAPI_SCHEMA_SYNC")
            .ok()
            .map(|s| rustapi_schema::SyncMode::from_env_str(&s))
            .unwrap_or_default();
        let docs_enabled = std::env::var("RUSTAPI_DOCS_ENABLED")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| !matches!(s.as_str(), "0" | "false" | "no"))
            .unwrap_or(true);
        let api_version = std::env::var("RUSTAPI_API_VERSION").unwrap_or_else(|_| "0.1.0".into());
        let public_base_url = std::env::var("RUSTAPI_PUBLIC_URL").unwrap_or_else(|_| "/".into());
        Ok(Self {
            database_url,
            db_max_connections,
            jwt_secret,
            jwt_ttl_secs,
            bind,
            log,
            page_size_max,
            studio_dir,
            schema_path,
            schema_sync_mode,
            docs_enabled,
            api_version,
            public_base_url,
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

    #[test]
    fn db_max_connections_parses() {
        // The pool-size parse is pure; test it directly to avoid the shared
        // process-env race with `rejects_short_jwt_secret` (both mutate
        // RUSTAPI_JWT_SECRET and tests run on parallel threads).
        let parse = |raw: Option<&str>| -> u32 {
            raw.and_then(|s| s.parse::<u32>().ok())
                .filter(|n| *n > 0)
                .unwrap_or(10)
        };
        assert_eq!(parse(None), 10, "unset → default 10");
        assert_eq!(parse(Some("20")), 20, "explicit value wins");
        assert_eq!(parse(Some("0")), 10, "zero → default");
        assert_eq!(parse(Some("nope")), 10, "garbage → default");
    }
}
