//! Shared integration-test plumbing. Spins a real Postgres via testcontainers
//! and the rustapi router in-process, hitting it via reqwest.

use rustapi_http::{
    build_router, resolve_provider, secret_key_from_env, AppConfig, AppState, EventSink,
    NoopAuditSink, NoopHook, NoopSink, RoleAuthz, RoleRegistry, WriteHook,
};
use rustapi_schema::{
    ComponentRegistry, ComponentService, SchemaRegistry, SchemaService, MIGRATOR,
};
use sqlx::PgPool;
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres as PgImage;
use tokio::sync::RwLock;

#[allow(dead_code)]
pub const JWT_SECRET: &str = "test-jwt-secret-with-32-characters!!";
#[allow(dead_code)]
pub const TEST_EMAIL: &str = "admin@example.test";
#[allow(dead_code)]
pub const TEST_PASSWORD: &str = "admin-password-123";

#[allow(dead_code)]
pub struct TestApp {
    pub base_url: String,
    pub pool: PgPool,
    pub client: reqwest::Client,
    /// The same SchemaService (and registry) the in-process router uses, so a
    /// test can mutate schema state and have the router observe it.
    pub schemas: SchemaService,
    pub components: ComponentService,
    /// Bearer token for the seeded admin user (set by `spawn`).
    pub token: String,
    _pg: ContainerAsync<PgImage>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

#[allow(dead_code)]
impl TestApp {
    pub async fn spawn() -> Self {
        Self::spawn_full(true, Arc::new(NoopHook), vec![]).await
    }

    pub async fn spawn_with_docs(docs_enabled: bool) -> Self {
        Self::spawn_full(docs_enabled, Arc::new(NoopHook), vec![]).await
    }

    /// Spawn with a custom `WriteHook` injected into `AppState`.
    #[allow(dead_code)]
    pub async fn spawn_with_hook(hook: Arc<dyn WriteHook>) -> Self {
        Self::spawn_full(true, hook, vec![]).await
    }

    /// Spawn with custom routers injected into `build_router`.
    #[allow(dead_code)]
    pub async fn spawn_with_routers(routers: Vec<axum::Router<AppState>>) -> Self {
        Self::spawn_full(true, Arc::new(NoopHook), routers).await
    }

    /// Spawn with a custom `EventSink` injected into `AppState`.
    #[allow(dead_code)]
    pub async fn spawn_with_sink(sink: Arc<dyn EventSink>) -> Self {
        Self::spawn_full_with_sink(true, Arc::new(NoopHook), vec![], sink).await
    }

    async fn spawn_full(
        docs_enabled: bool,
        hook: Arc<dyn WriteHook>,
        routers: Vec<axum::Router<AppState>>,
    ) -> Self {
        Self::spawn_full_with_sink(docs_enabled, hook, routers, Arc::new(NoopSink)).await
    }

    async fn spawn_full_with_sink(
        docs_enabled: bool,
        hook: Arc<dyn WriteHook>,
        routers: Vec<axum::Router<AppState>>,
        sink: Arc<dyn EventSink>,
    ) -> Self {
        let pg = PgImage::default().start().await.expect("pg start");
        let port = pg.get_host_port_ipv4(5432).await.expect("pg port");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .expect("pool");

        MIGRATOR.run(&pool).await.expect("migrate");

        let roles = RoleRegistry::new();
        roles.reload_from_db(&pool).await.expect("hydrate roles");

        let registry = SchemaRegistry::new();
        registry.reload_from_db(&pool).await.expect("hydrate");
        let schemas = SchemaService::new(pool.clone(), registry.clone());

        let component_registry = ComponentRegistry::new();
        component_registry
            .reload_from_db(&pool)
            .await
            .expect("hydrate components");
        let components = ComponentService::new(pool.clone(), component_registry);

        let media_dir =
            std::env::temp_dir().join(format!("rustapi-media-test-{}", uuid::Uuid::new_v4()));
        std::env::set_var(
            "RUSTAPI_MEDIA_BASE_DIR",
            media_dir.to_string_lossy().to_string(),
        );
        std::env::set_var("RUSTAPI_MEDIA_PROVIDER", "local");
        let secret_key = secret_key_from_env();
        let storage = Arc::new(RwLock::new(resolve_provider(&pool, secret_key).await));

        let state = AppState {
            pool: pool.clone(),
            schemas: schemas.clone(),
            components: components.clone(),
            authz: Arc::new(RoleAuthz::new(Arc::new(roles.clone()))),
            roles,
            gql: rustapi_http::graphql::GqlRegistry::new(),
            events: sink,
            audit: Arc::new(NoopAuditSink),
            hooks: hook,
            config: AppConfig {
                jwt_secret: JWT_SECRET.into(),
                jwt_ttl_secs: 3600,
                page_size_max: 100,
                docs_enabled,
                api_version: "test".into(),
                public_base_url: "/".into(),
            },
            storage,
            secret_key,
        };

        let app = build_router(state, routers);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let server = axum::serve(listener, app);
            tokio::select! {
                _ = server => {}
                _ = rx => {}
            }
        });

        let base_url = format!("http://{addr}");
        let client = reqwest::Client::new();

        // First-run setup → creates the admin user.
        let resp = client
            .post(format!("{base_url}/auth/setup"))
            .json(&serde_json::json!({ "email": TEST_EMAIL, "password": TEST_PASSWORD }))
            .send()
            .await
            .expect("setup request");
        assert_eq!(resp.status(), 201, "setup should create first admin");

        // Login → bearer token.
        let login: serde_json::Value = client
            .post(format!("{base_url}/auth/login"))
            .json(&serde_json::json!({ "email": TEST_EMAIL, "password": TEST_PASSWORD }))
            .send()
            .await
            .expect("login request")
            .json()
            .await
            .expect("login json");
        let token = login["token"]
            .as_str()
            .expect("token in login response")
            .to_string();

        Self {
            base_url,
            pool,
            client,
            schemas,
            components,
            token,
            _pg: pg,
            _shutdown: tx,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Attach the seeded admin's bearer token. (Method name kept as `admin`
    /// so existing call sites need no change.)
    pub fn admin(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder.header("authorization", format!("Bearer {}", self.token))
    }
}
