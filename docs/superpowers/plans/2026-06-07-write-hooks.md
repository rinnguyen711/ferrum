# Write Hooks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `WriteHook` trait with `before_write` (transform/reject the request body before validation) and `after_write` (observe the saved record post-commit, may fail the response) callbacks around content create/update, wired in like the existing `EventSink` seam.

**Architecture:** A single `WriteHook` trait lives in `crates/http/src/state.rs` next to `EventSink`, with two default-implemented async methods and a `WriteContext` value. `AppState` gains an `Arc<dyn WriteHook>` field defaulting to `NoopHook`. The content `create`/`update` handlers call `before_write` after authz and before `body_to_binds`, and `after_write` after commit and before `EventSink::emit`. Tests drive the full stack over HTTP via the `TestApp` harness, extended to inject a custom hook.

**Tech Stack:** Rust, axum, sqlx, async-trait, serde_json. Integration tests use testcontainers (real Postgres) + reqwest, following the existing `crates/bin/tests/common` harness.

Spec: `docs/superpowers/specs/2026-06-07-write-hooks-design.md`

---

## File structure

- `crates/http/src/state.rs` — add `WriteOp`, `WriteContext`, `WriteHook`, `NoopHook`; add `hooks` field to `AppState`. (Responsibility: pluggable trait definitions + app state. Already holds `Authz`/`EventSink`; this fits.)
- `crates/http/src/lib.rs` — export the four new symbols.
- `crates/http/src/routes/content.rs` — call `before_write`/`after_write` in `create` and `update`.
- `crates/bin/src/main.rs` — wire `NoopHook` into the production `AppState`.
- `crates/bin/tests/common/mod.rs` — add `spawn_with_hook` so a test can inject a `WriteHook`; default `spawn` wires `NoopHook`.
- `crates/bin/tests/write_hooks.rs` — new integration test file (create + reused `post` content type; one test-side hook with switches driven by request data).

Note: `crates/core/src/error.rs` is **not** modified — hooks reuse existing `Error` variants (`Validation`, `Forbidden`, `Internal`).

---

## Task 1: Define `WriteHook` trait, context, and `NoopHook`

**Files:**
- Modify: `crates/http/src/state.rs` (add types after the `EventSink`/`NoopSink` block, around line 47)
- Modify: `crates/http/src/state.rs` (add `hooks` field to `AppState`, around line 68)
- Modify: `crates/http/src/lib.rs:20` (exports)

This task is type-definition + wiring; its behavior is exercised by the integration tests in Tasks 4–5. We verify it here only by compiling.

- [ ] **Step 1: Add the imports needed for the new types**

In `crates/http/src/state.rs`, the existing top imports are:

```rust
use async_trait::async_trait;
use ferrum_core::{role_allows, Action, Event, Principal};
```

Change the `ferrum_core` line to also bring in `Error`, and add `serde_json`:

```rust
use async_trait::async_trait;
use ferrum_core::{role_allows, Action, Error, Event, Principal};
use serde_json::{Map, Value};
```

- [ ] **Step 2: Add `WriteOp`, `WriteContext`, `WriteHook`, `NoopHook`**

In `crates/http/src/state.rs`, immediately after the `NoopSink` impl block (the `impl EventSink for NoopSink` ends around line 47), add:

```rust
/// Which content write a hook is being invoked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOp {
    Create,
    Update,
}

/// Context passed to a `WriteHook`. Borrows live only for the duration of the
/// hook call. `content_type` is the registry name (e.g. `"article"`); the hook
/// dispatches per type itself.
pub struct WriteContext<'a> {
    pub content_type: &'a str,
    pub operation: WriteOp,
    pub principal: &'a Principal,
}

/// Developer extension point around content writes. Wired into `AppState` like
/// `EventSink`; the default `NoopHook` leaves behavior unchanged.
#[async_trait]
pub trait WriteHook: Send + Sync + 'static {
    /// Runs after authz and JSON parse, before schema validation
    /// (`body_to_binds`). May add, remove, or rewrite fields, or return `Err`
    /// to reject the request. The returned body is validated against the
    /// schema by the framework, so injected values must satisfy it.
    async fn before_write(
        &self,
        ctx: &WriteContext<'_>,
        body: Map<String, Value>,
    ) -> Result<Map<String, Value>, Error> {
        let _ = ctx;
        Ok(body)
    }

    /// Runs after the write commits, with the final saved record (after
    /// `row_to_json`, before populate/media-embed). The write is already
    /// durable; returning `Err` surfaces as a 5xx but does not roll back. For
    /// fire-and-forget fan-out (webhooks, cache bust) use `EventSink` instead.
    async fn after_write(
        &self,
        ctx: &WriteContext<'_>,
        record: &Value,
    ) -> Result<(), Error> {
        let _ = (ctx, record);
        Ok(())
    }
}

/// Default no-op hook. Both methods keep their trait defaults.
pub struct NoopHook;

#[async_trait]
impl WriteHook for NoopHook {}
```

- [ ] **Step 3: Add the `hooks` field to `AppState`**

In `crates/http/src/state.rs`, the `AppState` struct currently has:

```rust
    pub authz: Arc<dyn Authz>,
    pub events: Arc<dyn EventSink>,
    pub config: AppConfig,
```

Insert the `hooks` field after `events`:

```rust
    pub authz: Arc<dyn Authz>,
    pub events: Arc<dyn EventSink>,
    pub hooks: Arc<dyn WriteHook>,
    pub config: AppConfig,
```

- [ ] **Step 4: Export the new symbols**

In `crates/http/src/lib.rs`, the last line is:

```rust
pub use state::{AlwaysAllow, AppConfig, AppState, Authz, EventSink, NoopSink, RoleAuthz};
```

Replace it with:

```rust
pub use state::{
    AlwaysAllow, AppConfig, AppState, Authz, EventSink, NoopHook, NoopSink, RoleAuthz,
    WriteContext, WriteHook, WriteOp,
};
```

- [ ] **Step 5: Wire `NoopHook` into the production state**

In `crates/bin/src/main.rs`, update the import on line 4:

```rust
use ferrum_http::{build_router, mount_studio, resolve_provider, secret_key_from_env, AppConfig, AppState, NoopHook, NoopSink, RoleAuthz};
```

Then in the `AppState { .. }` literal (around line 41), add `hooks` after `events`:

```rust
        authz: Arc::new(RoleAuthz),
        events: Arc::new(NoopSink),
        hooks: Arc::new(NoopHook),
```

- [ ] **Step 6: Wire `NoopHook` into the test harness default**

The harness `AppState` literal must also compile. In `crates/bin/tests/common/mod.rs`, update the import:

```rust
use ferrum_http::{build_router, resolve_provider, secret_key_from_env, AppConfig, AppState, NoopHook, NoopSink, RoleAuthz};
```

This task adds the field; the harness will be refactored to inject a custom hook in Task 3, but it must compile now. In the `AppState { .. }` literal inside `spawn_with_docs`, add `hooks` after `events`:

```rust
            authz: Arc::new(RoleAuthz),
            events: Arc::new(NoopSink),
            hooks: Arc::new(NoopHook),
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo build -p ferrum-http -p ferrum`
Expected: builds clean (warnings about unused `NoopHook` in tests are acceptable until Task 3).

- [ ] **Step 8: Commit**

```bash
git add crates/http/src/state.rs crates/http/src/lib.rs crates/bin/src/main.rs crates/bin/tests/common/mod.rs
git commit -m "feat(http): add WriteHook trait and NoopHook seam"
```

---

## Task 2: Call `before_write` / `after_write` in the content handlers

**Files:**
- Modify: `crates/http/src/routes/content.rs:111-144` (`create`)
- Modify: `crates/http/src/routes/content.rs:178-236` (`update`)

No new test here — behavior is unchanged with the default `NoopHook`, and the existing `crates/bin/tests/integration_content.rs` suite proves create/update still work. Tasks 4–5 add the hook-specific tests. This task is verified by the existing suite staying green.

- [ ] **Step 1: Import the new symbols into the content module**

In `crates/http/src/routes/content.rs`, the imports include:

```rust
use crate::state::AppState;
```

Add a sibling import line just below it:

```rust
use crate::state::{AppState, WriteContext, WriteOp};
```

(Remove the now-duplicated `use crate::state::AppState;` — combine into the one line above.)

- [ ] **Step 2: Call `before_write` in `create`**

In `create`, the body currently begins:

```rust
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let (binds_map, checks, links, media_checks, media_links) = body_to_binds(&ct, body, true)?;
```

Insert the hook call between the `ct` fetch and `body_to_binds`:

```rust
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;

    let ctx = WriteContext {
        content_type: &ct.name,
        operation: WriteOp::Create,
        principal: &principal,
    };
    let body = state.hooks.before_write(&ctx, body).await.map_err(ApiError)?;

    let (binds_map, checks, links, media_checks, media_links) = body_to_binds(&ct, body, true)?;
```

- [ ] **Step 3: Call `after_write` in `create`**

In `create`, the tail currently reads:

```rust
    write_links(&mut tx, &ct.name, &links, new_id).await?;
    write_media_links(&mut tx, &ct.name, &media_links, new_id).await?;
    tx.commit().await.map_err(db)?;

    state.events.emit(Event::EntryCreated { content_type: ct.name.clone(), id: new_id }).await;
    Ok((StatusCode::CREATED, Json(body)))
```

Insert the `after_write` call after `tx.commit()` and before `events.emit`. Note `body` here is already the saved record from the earlier `row_to_json(&ct, &row)?`:

```rust
    write_links(&mut tx, &ct.name, &links, new_id).await?;
    write_media_links(&mut tx, &ct.name, &media_links, new_id).await?;
    tx.commit().await.map_err(db)?;

    state.hooks.after_write(&ctx, &body).await.map_err(ApiError)?;
    state.events.emit(Event::EntryCreated { content_type: ct.name.clone(), id: new_id }).await;
    Ok((StatusCode::CREATED, Json(body)))
```

- [ ] **Step 4: Call `before_write` in `update`**

In `update`, the body currently begins:

```rust
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let (mut binds_map, checks, links, media_checks, media_links) = body_to_binds(&ct, body, true)?;
```

Insert the hook call between the `ct` fetch and `body_to_binds`:

```rust
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;

    let ctx = WriteContext {
        content_type: &ct.name,
        operation: WriteOp::Update,
        principal: &principal,
    };
    let body = state.hooks.before_write(&ctx, body).await.map_err(ApiError)?;

    let (mut binds_map, checks, links, media_checks, media_links) = body_to_binds(&ct, body, true)?;
```

- [ ] **Step 5: Call `after_write` in `update`**

In `update`, the tail currently reads:

```rust
    write_links(&mut tx, &ct.name, &links, id).await?;
    write_media_links(&mut tx, &ct.name, &media_links, id).await?;
    tx.commit().await.map_err(db)?;

    state.events.emit(Event::EntryUpdated { content_type: ct.name.clone(), id }).await;
    Ok(Json(row_to_json(&ct, &row)?))
```

`row_to_json` is currently computed inline in the return. Compute it once into a binding so the hook and the response share it:

```rust
    write_links(&mut tx, &ct.name, &links, id).await?;
    write_media_links(&mut tx, &ct.name, &media_links, id).await?;
    tx.commit().await.map_err(db)?;

    let record = row_to_json(&ct, &row)?;
    state.hooks.after_write(&ctx, &record).await.map_err(ApiError)?;
    state.events.emit(Event::EntryUpdated { content_type: ct.name.clone(), id }).await;
    Ok(Json(record))
```

- [ ] **Step 6: Verify existing behavior is unchanged**

Run: `cargo test -p ferrum --test integration_content`
Expected: all tests pass (NoopHook is a no-op; create/update unchanged).

- [ ] **Step 7: Commit**

```bash
git add crates/http/src/routes/content.rs
git commit -m "feat(http): invoke WriteHook around content create/update"
```

---

## Task 3: Extend the test harness to inject a hook

**Files:**
- Modify: `crates/bin/tests/common/mod.rs` (add `spawn_with_hook`; route `spawn`/`spawn_with_docs` through it)

- [ ] **Step 1: Add the `WriteHook` import to the harness**

In `crates/bin/tests/common/mod.rs`, update the `ferrum_http` import (set in Task 1 Step 6) to also bring in `WriteHook`:

```rust
use ferrum_http::{build_router, resolve_provider, secret_key_from_env, AppConfig, AppState, NoopHook, NoopSink, RoleAuthz, WriteHook};
```

Add `use std::sync::Arc;` is already present.

- [ ] **Step 2: Refactor `spawn_with_docs` to take a hook**

Change the harness so the hook is a parameter. Replace the current
`spawn`/`spawn_with_docs` method signatures and the `AppState` `hooks` line.

Current:

```rust
    pub async fn spawn() -> Self {
        Self::spawn_with_docs(true).await
    }

    pub async fn spawn_with_docs(docs_enabled: bool) -> Self {
```

Replace with:

```rust
    pub async fn spawn() -> Self {
        Self::spawn_full(true, Arc::new(NoopHook)).await
    }

    pub async fn spawn_with_docs(docs_enabled: bool) -> Self {
        Self::spawn_full(docs_enabled, Arc::new(NoopHook)).await
    }

    /// Spawn with a custom `WriteHook` injected into `AppState`.
    pub async fn spawn_with_hook(hook: Arc<dyn WriteHook>) -> Self {
        Self::spawn_full(true, hook).await
    }

    async fn spawn_full(docs_enabled: bool, hook: Arc<dyn WriteHook>) -> Self {
```

- [ ] **Step 3: Use the injected hook in the `AppState` literal**

In the `AppState { .. }` literal inside the (now) `spawn_full` body, change the `hooks` line set in Task 1 Step 6 from:

```rust
            hooks: Arc::new(NoopHook),
```

to:

```rust
            hooks: hook,
```

- [ ] **Step 4: Verify the harness still compiles and existing tests pass**

Run: `cargo test -p ferrum --test integration_content`
Expected: all tests pass (default `spawn` still wires `NoopHook`).

- [ ] **Step 5: Commit**

```bash
git add crates/bin/tests/common/mod.rs
git commit -m "test(http): allow injecting a WriteHook into TestApp"
```

---

## Task 4: Test `before_write` — transform and reject

**Files:**
- Create: `crates/bin/tests/write_hooks.rs`

The test-side hook reads request data to decide its behavior, so one hook type
covers transform, reject, and (Task 5) after-write cases. We reuse the same
`post` content type shape as `integration_content.rs` (with an extra optional
`slug` string field).

**Files (additional):**
- Modify: `crates/bin/Cargo.toml` (add `async-trait` dev-dependency)

- [ ] **Step 0: Add the `async-trait` dev-dependency**

The test hook impls need `#[async_trait]`. `async-trait` is already a
workspace dependency (`Cargo.toml:52`) but the bin crate does not pull it in.
In `crates/bin/Cargo.toml`, under `[dev-dependencies]`, add:

```toml
async-trait.workspace = true
```

Verify it resolves:

Run: `cargo build -p ferrum --tests`
Expected: builds (no test file yet referencing it is fine).

- [ ] **Step 1: Write the failing test file with a transforming hook**

Create `crates/bin/tests/write_hooks.rs`:

```rust
mod common;

use async_trait::async_trait;
use common::TestApp;
use ferrum_core::{Error, ValidationErrors};
use ferrum_http::{WriteContext, WriteHook, WriteOp};
use serde_json::{json, Map, Value};
use std::sync::Arc;

/// Test hook driven entirely by request data so one type covers all cases:
/// - before_write injects `slug` from `title` when `slug` is absent
/// - before_write rejects when `title` equals "REJECT"
/// - before_write injects an UNKNOWN field when `title` equals "INJECTBAD"
///   (proves the framework re-validates the hook's output)
/// - after_write returns Err when `title` equals "POSTFAIL"
struct TestHook;

#[async_trait]
impl WriteHook for TestHook {
    async fn before_write(
        &self,
        _ctx: &WriteContext<'_>,
        mut body: Map<String, Value>,
    ) -> Result<Map<String, Value>, Error> {
        if body.get("title").and_then(|v| v.as_str()) == Some("REJECT") {
            return Err(Error::Validation(ValidationErrors::single(
                "title not allowed".to_string(),
            )));
        }
        if body.get("title").and_then(|v| v.as_str()) == Some("INJECTBAD") {
            body.insert("not_a_field".to_string(), Value::Bool(true));
            return Ok(body);
        }
        if !body.contains_key("slug") {
            if let Some(title) = body.get("title").and_then(|v| v.as_str()) {
                let slug = title.to_lowercase().replace(' ', "-");
                body.insert("slug".to_string(), Value::String(slug));
            }
        }
        Ok(body)
    }

    async fn after_write(
        &self,
        _ctx: &WriteContext<'_>,
        record: &Value,
    ) -> Result<(), Error> {
        if record.get("title").and_then(|v| v.as_str()) == Some("POSTFAIL") {
            return Err(Error::Internal(anyhow::anyhow!("after_write failed")));
        }
        Ok(())
    }
}

async fn make_post_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true, "max_length": 64},
                {"name": "slug", "kind": "string", "max_length": 80}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn before_write_transforms_body() {
    let app = TestApp::spawn_with_hook(Arc::new(TestHook)).await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "Hello World" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let entry: Value = resp.json().await.unwrap();
    assert_eq!(entry["slug"], "hello-world", "hook should derive slug");
}

#[tokio::test]
async fn before_write_rejects_request() {
    let app = TestApp::spawn_with_hook(Arc::new(TestHook)).await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "REJECT" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "rejection should be a validation error");

    // Nothing was written: list is empty.
    let resp = app.admin(app.client.get(app.url("/api/post"))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 0, "rejected request must not persist");
}

#[tokio::test]
async fn before_write_output_is_revalidated() {
    let app = TestApp::spawn_with_hook(Arc::new(TestHook)).await;
    make_post_type(&app).await;

    // Hook injects an unknown field; body_to_binds must reject it (proves the
    // hook's output goes through schema validation, not around it).
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "INJECTBAD" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "injected unknown field must be rejected");

    let resp = app.admin(app.client.get(app.url("/api/post"))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 0, "invalid injected field must not persist");
}
```

- [ ] **Step 2: Run the tests to verify they pass**

These pass once Tasks 1–3 are in place (the hook is exercised end-to-end).

Run: `cargo test -p ferrum --test write_hooks before_write`
Expected: `before_write_transforms_body` PASS, `before_write_rejects_request` PASS, `before_write_output_is_revalidated` PASS.

If `before_write_rejects_request` returns a status other than 422, check how
`Error::Validation` maps in `crates/http/src/error.rs` and adjust the asserted
status to match that mapping (do not change the mapping).

- [ ] **Step 3: Commit**

```bash
git add crates/bin/Cargo.toml crates/bin/tests/write_hooks.rs
git commit -m "test(http): cover before_write transform and reject"
```

---

## Task 5: Test `after_write` and `WriteContext` correctness

**Files:**
- Modify: `crates/bin/tests/write_hooks.rs` (append tests; reuse `TestHook` and `make_post_type`)

- [ ] **Step 1: Add the after_write failure test**

Append to `crates/bin/tests/write_hooks.rs`:

```rust
#[tokio::test]
async fn after_write_failure_returns_500_but_write_is_durable() {
    let app = TestApp::spawn_with_hook(Arc::new(TestHook)).await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "POSTFAIL" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 500, "after_write Err surfaces as 5xx");

    // The write committed before after_write ran, so the row is durable.
    let resp = app.admin(app.client.get(app.url("/api/post"))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 1, "write must persist despite hook error");
    assert_eq!(body["data"][0]["title"], "POSTFAIL");
}
```

- [ ] **Step 2: Add a context-correctness test using a recording hook**

The recording hook captures the `WriteOp` it saw for create vs update. Append:

```rust
use std::sync::Mutex;

/// Records the operation seen by before_write so the test can assert
/// Create on POST and Update on PUT.
struct RecordingHook {
    ops: Arc<Mutex<Vec<WriteOp>>>,
}

#[async_trait]
impl WriteHook for RecordingHook {
    async fn before_write(
        &self,
        ctx: &WriteContext<'_>,
        body: Map<String, Value>,
    ) -> Result<Map<String, Value>, Error> {
        self.ops.lock().unwrap().push(ctx.operation);
        Ok(body)
    }
}

#[tokio::test]
async fn write_context_reports_create_then_update() {
    let ops = Arc::new(Mutex::new(Vec::new()));
    let app = TestApp::spawn_with_hook(Arc::new(RecordingHook { ops: ops.clone() })).await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "First" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let entry: Value = resp.json().await.unwrap();
    let id = entry["id"].as_str().unwrap().to_string();

    let resp = app
        .admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({ "title": "Second" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let seen = ops.lock().unwrap().clone();
    assert_eq!(seen, vec![WriteOp::Create, WriteOp::Update]);
}
```

- [ ] **Step 3: Run the full file**

Run: `cargo test -p ferrum --test write_hooks`
Expected: all six tests pass:
`before_write_transforms_body`, `before_write_rejects_request`,
`before_write_output_is_revalidated`,
`after_write_failure_returns_500_but_write_is_durable`,
`write_context_reports_create_then_update`.

- [ ] **Step 4: Run the whole workspace to confirm no regression**

Run: `cargo test --workspace`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/bin/tests/write_hooks.rs
git commit -m "test(http): cover after_write durability and WriteContext"
```

---

## Self-review notes

- **Spec coverage:** trait + types (Task 1), `before_write`/`after_write` call sites with ordering (Task 2), wiring + exports (Task 1 Steps 4–6), error mapping reuse (Tasks 4–5 assert 422/500), all seven spec test cases mapped:
  1. NoopHook unchanged → Task 2 Step 6 (existing suite green).
  2. before_write transforms → Task 4 `before_write_transforms_body`.
  3. before_write rejects, nothing written → Task 4 `before_write_rejects_request`.
  4. output still schema-validated → Task 4 `before_write_output_is_revalidated` (hook injects an unknown field; `body_to_binds` rejects with 422).
  5. after_write observes saved record (incl. server-set fields) → Task 5 durability test asserts the persisted row; the hook reads `record["title"]`.
  6. after_write fails, write durable → Task 5 `after_write_failure_returns_500_but_write_is_durable`.
  7. WriteContext op correctness → Task 5 `write_context_reports_create_then_update`.
- **delete_one** intentionally untouched (spec: out of scope).
- **No new error variants** — reuses `Error::Validation`/`Internal`.
