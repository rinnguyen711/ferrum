//! Shared integration-test plumbing. Spins a real Postgres via testcontainers
//! and the rustapi router in-process, hitting it via reqwest.

use rustapi_http::{build_router, AlwaysAllow, AppConfig, AppState, NoopSink};
use rustapi_schema::{SchemaRegistry, SchemaService, MIGRATOR};
use sqlx::PgPool;
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres as PgImage;

#[allow(dead_code)]
pub const ADMIN_KEY: &str = "test-admin-key-with-32-characters!!";

#[allow(dead_code)]
pub struct TestApp {
    pub base_url: String,
    pub pool: PgPool,
    pub client: reqwest::Client,
    _pg: ContainerAsync<PgImage>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

#[allow(dead_code)]
impl TestApp {
    pub async fn spawn() -> Self {
        let pg = PgImage::default().start().await.expect("pg start");
        let port = pg.get_host_port_ipv4(5432).await.expect("pg port");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .expect("pool");

        MIGRATOR.run(&pool).await.expect("migrate");

        let registry = SchemaRegistry::new();
        registry.reload_from_db(&pool).await.expect("hydrate");
        let schemas = SchemaService::new(pool.clone(), registry.clone());

        let state = AppState {
            pool: pool.clone(),
            schemas,
            authz: Arc::new(AlwaysAllow),
            events: Arc::new(NoopSink),
            config: AppConfig {
                admin_key: ADMIN_KEY.into(),
                page_size_max: 100,
            },
        };

        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let server = axum::serve(listener, app);
            tokio::select! {
                _ = server => {}
                _ = rx => {}
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            pool,
            client: reqwest::Client::new(),
            _pg: pg,
            _shutdown: tx,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub fn admin(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder.header("x-api-key", ADMIN_KEY)
    }
}
