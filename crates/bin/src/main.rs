use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rustapi::config::Config;
use rustapi_http::{
    build_router, mount_studio, resolve_provider, secret_key_from_env, AppConfig, AppState,
    NoopHook, RoleAuthz, RoleRegistry,
};
use rustapi_schema::{
    ComponentRegistry, ComponentService, SchemaRegistry, SchemaService, MIGRATOR,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing_subscriber::{prelude::*, EnvFilter};

mod migrate;

#[derive(Debug, Parser)]
#[command(name = "rustapi", about = "Headless CMS")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Migrate an existing Postgres database into Rustapi.
    Migrate(migrate::MigrateArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Migrate(args)) = cli.command {
        tracing_subscriber::fmt().with_env_filter("info").init();
        return migrate::run(args).await;
    }

    let cfg = Config::from_env()?;
    init_tracing(&cfg.log);

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "rustapi starting");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&cfg.database_url)
        .await
        .context("connect to Postgres")?;
    tracing::info!(max_connections = 10, "postgres connected");

    MIGRATOR
        .run(&pool)
        .await
        .context("run internal migrations")?;
    tracing::info!("internal migrations applied");

    let registry = SchemaRegistry::new();
    registry
        .reload_from_db(&pool)
        .await
        .context("hydrate schema registry")?;
    tracing::info!(
        schemas = registry.list().await.len(),
        "schema registry hydrated"
    );

    let schemas = SchemaService::new(pool.clone(), registry.clone());

    let component_registry = ComponentRegistry::new();
    component_registry
        .reload_from_db(&pool)
        .await
        .context("hydrate component registry")?;
    let components = ComponentService::new(pool.clone(), component_registry);

    let roles = RoleRegistry::new();
    roles
        .reload_from_db(&pool)
        .await
        .context("hydrate role registry")?;

    if let Some(path) = &cfg.schema_path {
        rustapi_schema::sync::sync_from_path(&schemas, &components, path, cfg.schema_sync_mode)
            .await
            .context("schema sync")?;
        registry
            .reload_from_db(&pool)
            .await
            .context("reload schema registry after sync")?;
        tracing::info!(schemas = registry.list().await.len(), "schema sync applied");
    }

    let secret_key = secret_key_from_env();
    let storage = Arc::new(RwLock::new(resolve_provider(&pool, secret_key).await));

    let state = AppState {
        pool: pool.clone(),
        schemas,
        components,
        authz: Arc::new(RoleAuthz::new(Arc::new(roles.clone()))),
        roles,
        gql: rustapi_http::graphql::GqlRegistry::new(),
        events: Arc::new(rustapi::webhook_worker::DbEventSink::new(pool.clone())),
        audit: Arc::new(rustapi::audit_sink::DbAuditSink::new(pool.clone())),
        hooks: Arc::new(NoopHook),
        config: AppConfig {
            jwt_secret: cfg.jwt_secret.clone(),
            jwt_ttl_secs: cfg.jwt_ttl_secs,
            page_size_max: cfg.page_size_max,
            docs_enabled: cfg.docs_enabled,
            api_version: cfg.api_version.clone(),
            public_base_url: cfg.public_base_url.clone(),
        },
        storage,
        secret_key,
    };

    // Build the initial GraphQL schema from the hydrated content-type
    // registry. Rebuilt on content-type CRUD (see routes/schema.rs).
    {
        let types = state.schemas.registry().list().await;
        state
            .gql
            .rebuild(&types)
            .await
            .context("build initial GraphQL schema")?;
    }

    rustapi::webhook_worker::spawn_worker(pool.clone());

    let audit_retention_days = std::env::var("AUDIT_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(90);
    rustapi::audit_sink::spawn_prune_worker(pool.clone(), audit_retention_days);

    let mut app = build_router(state, vec![]);
    tracing::info!(
        content = "/api",
        admin = "/admin/content-types",
        health = "/healthz",
        "routes mounted"
    );
    if let Some(ref dir) = cfg.studio_dir {
        app = mount_studio(app, dir);
        tracing::info!(dir = %dir, route = "/studio", "studio UI mounted");
    }
    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .context("bind")?;
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
