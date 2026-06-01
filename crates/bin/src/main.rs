mod config;

use anyhow::{Context, Result};
use config::Config;
use rustapi_http::{build_router, mount_studio, AlwaysAllow, AppConfig, AppState, NoopSink};
use rustapi_schema::{SchemaRegistry, SchemaService, MIGRATOR};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tracing_subscriber::{prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    init_tracing(&cfg.log);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&cfg.database_url)
        .await
        .context("connect to Postgres")?;

    MIGRATOR.run(&pool).await.context("run internal migrations")?;

    let registry = SchemaRegistry::new();
    registry.reload_from_db(&pool).await.context("hydrate schema registry")?;

    let schemas = SchemaService::new(pool.clone(), registry.clone());

    let state = AppState {
        pool,
        schemas,
        authz: Arc::new(AlwaysAllow),
        events: Arc::new(NoopSink),
        config: AppConfig {
            admin_key: cfg.admin_key.clone(),
            page_size_max: cfg.page_size_max,
        },
    };

    let mut app = build_router(state);
    if let Some(ref dir) = cfg.studio_dir {
        app = mount_studio(app, dir);
        tracing::info!(dir = %dir, "studio UI mounted at /studio");
    }
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await.context("bind")?;
    tracing::info!(addr = %cfg.bind, "rustapi listening");
    axum::serve(listener, app).await.context("serve")?;
    Ok(())
}

fn init_tracing(filter: &str) {
    let env_filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}
