use anyhow::{Context, Result};
use rustapi::config::Config;
use rustapi::seed;
use rustapi_http::{build_router, mount_studio, AppConfig, AppState, NoopSink, RoleAuthz};
use rustapi_schema::{SchemaRegistry, SchemaService, MIGRATOR};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tracing_subscriber::{prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = Config::from_env()?;
    init_tracing(&cfg.log);

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "rustapi starting");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&cfg.database_url)
        .await
        .context("connect to Postgres")?;
    tracing::info!(max_connections = 10, "postgres connected");

    MIGRATOR.run(&pool).await.context("run internal migrations")?;
    tracing::info!("internal migrations applied");

    let registry = SchemaRegistry::new();
    registry.reload_from_db(&pool).await.context("hydrate schema registry")?;
    tracing::info!(schemas = registry.list().await.len(), "schema registry hydrated");

    let schemas = SchemaService::new(pool.clone(), registry.clone());

    seed::seed_if_empty(&pool, &schemas, cfg.seed)
        .await
        .context("seed default content")?;

    let state = AppState {
        pool,
        schemas,
        authz: Arc::new(RoleAuthz),
        events: Arc::new(NoopSink),
        config: AppConfig {
            jwt_secret: cfg.jwt_secret.clone(),
            jwt_ttl_secs: cfg.jwt_ttl_secs,
            page_size_max: cfg.page_size_max,
        },
    };

    let mut app = build_router(state);
    tracing::info!(content = "/api", admin = "/admin/content-types", health = "/healthz", "routes mounted");
    if let Some(ref dir) = cfg.studio_dir {
        app = mount_studio(app, dir);
        tracing::info!(dir = %dir, route = "/studio", "studio UI mounted");
    }
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await.context("bind")?;
    let addr = listener.local_addr().context("local addr")?;
    tracing::info!(addr = %addr, port = addr.port(), "rustapi listening");
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
