# `rustapi` Binary + Integration Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Boot the server (config, pool, migrations, registry, router) and prove end-to-end flows with integration tests against a real Postgres via `testcontainers`.

**Architecture:** A thin `main` that loads env, runs internal migrations, reload registry, builds router, serves. Integration tests each spin a fresh Postgres container and run schema-isolated test suites against the in-process router.

**Tech Stack:** `tokio`, `axum`, `sqlx`, `tracing-subscriber`, `testcontainers`, `reqwest`.

**Prerequisites:** Plans 00–04 complete.

---

### Task 1: Config loader

**Files:**
- Create: `crates/bin/src/config.rs`
- Modify: `crates/bin/src/main.rs`
- Test: inline

- [ ] **Step 1: Write `crates/bin/src/config.rs`**

```rust
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
```

- [ ] **Step 2: Update `crates/bin/src/main.rs` to use it (will overwrite again in Task 2)**

```rust
mod config;

fn main() {
    println!("rustapi bootstrap — see Task 2");
}
```

- [ ] **Step 3: Build**

Run: `cargo build -p rustapi`
Expected: PASS.

- [ ] **Step 4: Run config test**

Run: `cargo test -p rustapi --bin rustapi config`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/bin
git commit -m "feat(bin): env config loader with admin key length check"
```

---

### Task 2: Server bootstrap (`main`)

**Files:**
- Modify: `crates/bin/src/main.rs`

- [ ] **Step 1: Write `crates/bin/src/main.rs`**

```rust
mod config;

use anyhow::{Context, Result};
use config::Config;
use rustapi_http::{build_router, AlwaysAllow, AppConfig, AppState, NoopSink};
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

    let app = build_router(state);
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
```

- [ ] **Step 2: Build**

Run: `cargo build -p rustapi`
Expected: PASS, no warnings.

- [ ] **Step 3: Commit**

```bash
git add crates/bin
git commit -m "feat(bin): server bootstrap (pool, migrations, registry, router)"
```

---

### Task 3: Test harness — Postgres container + app spawn

**Files:**
- Create: `crates/bin/tests/common/mod.rs`

- [ ] **Step 1: Write `crates/bin/tests/common/mod.rs`**

```rust
//! Shared integration-test plumbing. Spins a real Postgres via testcontainers
//! and the rustapi router in-process, hitting it via reqwest.

use rustapi_http::{build_router, AlwaysAllow, AppConfig, AppState, NoopSink};
use rustapi_schema::{SchemaRegistry, SchemaService, MIGRATOR};
use sqlx::PgPool;
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres as PgImage;

pub const ADMIN_KEY: &str = "test-admin-key-with-32-characters!!";

pub struct TestApp {
    pub base_url: String,
    pub pool: PgPool,
    pub client: reqwest::Client,
    _pg: ContainerAsync<PgImage>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

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
            base_url: format!("http://{}", addr),
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
```

- [ ] **Step 2: Smoke test** — `crates/bin/tests/integration_smoke.rs`

```rust
mod common;
use common::TestApp;

#[tokio::test]
async fn healthz_ok() {
    let app = TestApp::spawn().await;
    let resp = app.client.get(app.url("/healthz")).send().await.unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn admin_requires_key() {
    let app = TestApp::spawn().await;
    let resp = app.client.get(app.url("/admin/content-types")).send().await.unwrap();
    assert_eq!(resp.status(), 401);
}
```

- [ ] **Step 3: Run smoke test**

Run: `cargo test -p rustapi --test integration_smoke -- --nocapture`
Expected: PASS (slow — Docker image pull on first run).

- [ ] **Step 4: Commit**

```bash
git add crates/bin/tests
git commit -m "test(bin): integration harness with testcontainers postgres"
```

---

### Task 4: Schema CRUD integration test

**Files:**
- Create: `crates/bin/tests/integration_schema.rs`

- [ ] **Step 1: Write `crates/bin/tests/integration_schema.rs`**

```rust
mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn create_list_get_delete_content_type() {
    let app = TestApp::spawn().await;

    // Create
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true, "max_length": 64}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // List
    let resp = app
        .admin(app.client.get(app.url("/admin/content-types")))
        .send()
        .await
        .unwrap();
    let list: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "post");

    // Get one
    let resp = app
        .admin(app.client.get(app.url("/admin/content-types/post")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["display_name"], "Post");

    // Delete without confirm → 422
    let resp = app
        .admin(app.client.delete(app.url("/admin/content-types/post")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);

    // Delete with confirm
    let resp = app
        .admin(app.client.delete(app.url("/admin/content-types/post?confirm=true")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    // Gone
    let resp = app
        .admin(app.client.get(app.url("/admin/content-types/post")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn rejects_invalid_type_name() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "Bad Name",
            "display_name": "X",
            "fields": [{"name": "title", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn rejects_duplicate_create() {
    let app = TestApp::spawn().await;
    let payload = json!({
        "name": "post",
        "display_name": "Post",
        "fields": [{"name": "title", "kind": "string"}]
    });
    let resp = app.admin(app.client.post(app.url("/admin/content-types"))).json(&payload).send().await.unwrap();
    assert_eq!(resp.status(), 201);
    let resp = app.admin(app.client.post(app.url("/admin/content-types"))).json(&payload).send().await.unwrap();
    assert_eq!(resp.status(), 409);
}
```

- [ ] **Step 2: Run**

Run: `cargo test -p rustapi --test integration_schema`
Expected: PASS — 3 tests.

- [ ] **Step 3: Commit**

```bash
git add crates/bin/tests
git commit -m "test(bin): schema CRUD integration coverage"
```

---

### Task 5: Content CRUD integration test

**Files:**
- Create: `crates/bin/tests/integration_content.rs`

- [ ] **Step 1: Write `crates/bin/tests/integration_content.rs`**

```rust
mod common;
use common::TestApp;
use serde_json::json;

async fn make_post_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true, "max_length": 64},
                {"name": "views", "kind": "integer"},
                {"name": "published", "kind": "boolean", "default": false}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
}

#[tokio::test]
async fn full_entry_lifecycle() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;

    // Create entry
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "Hello", "views": 3}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let entry: serde_json::Value = resp.json().await.unwrap();
    let id = entry["id"].as_str().unwrap().to_string();
    assert_eq!(entry["title"], "Hello");
    assert_eq!(entry["views"], 3);
    assert_eq!(entry["published"], false);

    // List
    let resp = app.admin(app.client.get(app.url("/api/post"))).send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["id"], id);

    // Get one
    let resp = app.admin(app.client.get(app.url(&format!("/api/post/{id}")))).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    // Update
    let resp = app
        .admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({"title": "Hello v2", "views": 10, "published": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["title"], "Hello v2");
    assert_eq!(updated["published"], true);

    // Delete
    let resp = app.admin(app.client.delete(app.url(&format!("/api/post/{id}")))).send().await.unwrap();
    assert_eq!(resp.status(), 204);

    let resp = app.admin(app.client.get(app.url(&format!("/api/post/{id}")))).send().await.unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn required_field_missing_rejected() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"views": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn unknown_field_rejected() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "x", "ghost": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn pagination_and_sort() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;
    for i in 0..5 {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"title": format!("t{i}"), "views": i}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }
    let resp = app
        .admin(app.client.get(app.url("/api/post?page=1&pageSize=2&sort=views:desc")))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 5);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"][0]["views"], 4);
    assert_eq!(body["data"][1]["views"], 3);
}

#[tokio::test]
async fn unknown_sort_field_rejected() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?sort=ghost:asc")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}
```

- [ ] **Step 2: Run**

Run: `cargo test -p rustapi --test integration_content`
Expected: PASS — 5 tests.

- [ ] **Step 3: Commit**

```bash
git add crates/bin/tests
git commit -m "test(bin): content CRUD integration coverage"
```

---

### Task 6: Patch integration test

**Files:**
- Create: `crates/bin/tests/integration_patch.rs`

- [ ] **Step 1: Write `crates/bin/tests/integration_patch.rs`**

```rust
mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn add_then_drop_field() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [{"name": "title", "kind": "string", "required": true}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Add `views` field
    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({
            "add_fields": [{"name": "views", "kind": "integer"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Entry now accepts the new field
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "x", "views": 42}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Drop `views`
    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({
            "drop_fields": ["views"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Posting `views` is now unknown
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "y", "views": 9}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn cannot_drop_system_field() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [{"name": "title", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({"drop_fields": ["id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn cannot_re_add_existing_field() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [{"name": "title", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({"add_fields": [{"name": "title", "kind": "text"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}
```

- [ ] **Step 2: Run**

Run: `cargo test -p rustapi --test integration_patch`
Expected: PASS — 3 tests.

- [ ] **Step 3: Commit**

```bash
git add crates/bin/tests
git commit -m "test(bin): PATCH content-type integration coverage"
```

---

### Task 7: Full workspace test run + README dev section

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Run all tests**

Run: `cargo test --workspace`
Expected: PASS — all unit + integration tests.

If a test fails: do NOT mark this task complete. Open a follow-up task to fix the bug in the relevant crate, then re-run.

- [ ] **Step 2: Expand `README.md`**

```markdown
# rustapi

Headless CMS framework in Rust. v1 in progress — see [design spec](docs/superpowers/specs/2026-05-28-rustapi-core-design.md).

## Dev

Requires: Rust 1.82, Docker (for integration tests).

```sh
# Build
cargo build --workspace

# Run unit + integration tests (spawns ephemeral Postgres via testcontainers)
cargo test --workspace

# Run the server against an external Postgres
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/rustapi
export RUSTAPI_ADMIN_KEY=$(openssl rand -hex 32)
cargo run -p rustapi
```

## API

See the [design spec §4](docs/superpowers/specs/2026-05-28-rustapi-core-design.md) for the full HTTP surface.

Quick start:

```sh
# Create a content type
curl -X POST http://localhost:8080/admin/content-types \
  -H "x-api-key: $RUSTAPI_ADMIN_KEY" \
  -H "content-type: application/json" \
  -d '{"name":"post","display_name":"Post","fields":[{"name":"title","kind":"string","required":true}]}'

# Create an entry
curl -X POST http://localhost:8080/api/post \
  -H "x-api-key: $RUSTAPI_ADMIN_KEY" \
  -H "content-type: application/json" \
  -d '{"title":"Hello"}'
```
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: expand README with dev and API quickstart"
```

---

## Self-Review Notes

- Spec §5.1 boot sequence (config → pool → migrate → reload registry → router → serve) → Task 2.
- Spec §7.1 config var list with admin-key length check → Task 1.
- Spec §7.4 integration test strategy via testcontainers → Task 3. Schema isolation per-test handled by spawning a fresh container per test rather than schema-per-test — simpler and reliably parallel.
- Spec §4 all endpoints exercised: smoke (Task 3), schema CRUD (Task 4), content CRUD + pagination + sort (Task 5), PATCH add/drop (Task 6).
- Concurrency note from Plan 03 §5.5 (potential create race): not exercised here; flagged for follow-up if a regression surfaces.
- `RUSTAPI_LOG` JSON output verified visually during dev; no test asserts log shape.
