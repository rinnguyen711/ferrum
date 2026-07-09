# Injectable Custom Routers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the bin inject custom protected endpoints into the router without editing the core `http` crate.

**Architecture:** `build_router` gains a `Vec<Router<AppState>>` parameter. Each is `.merge`d into the protected sub-router after the built-ins and before the `require_auth` layer, so injected routes are auth-gated and path collisions panic at startup. Both existing callers pass `vec![]`; behavior is unchanged for the shipped binary.

**Tech Stack:** Rust, axum, testcontainers + reqwest integration tests, real Postgres.

Spec: `docs/superpowers/specs/2026-06-07-injectable-custom-routers-design.md`

---

### Task 1: Add `extra` parameter to `build_router`

This task changes the signature, updates both callers to compile, then adds an integration test proving injected routes work and are auth-gated. The signature change and caller updates land first (a pure refactor — compiles, existing tests pass), then TDD the new behavior.

**Files:**
- Modify: `crates/http/src/routes/mod.rs:15-36` (signature + merge loop)
- Modify: `crates/bin/src/main.rs:59` (caller)
- Modify: `crates/bin/tests/common/mod.rs:34-91` (caller + new `spawn_with_routers` helper)
- Test: `crates/bin/tests/custom_routers.rs` (new)

---

- [ ] **Step 1: Change `build_router` signature and merge extras**

In `crates/http/src/routes/mod.rs`, replace the `build_router` function body (lines 15-36) with:

```rust
pub fn build_router(state: AppState, extra: Vec<Router<AppState>>) -> Router {
    let mut public = Router::new()
        .route("/healthz", get(health::healthz))
        .merge(auth::public_router());

    if state.config.docs_enabled {
        public = public.merge(openapi::router());
    }

    let mut protected = Router::new()
        .merge(schema::router())
        .merge(content::router())
        .merge(users::router())
        .merge(media::router())
        .merge(auth::protected_router());

    // Custom routers from the bin, merged after built-ins. Behind the same
    // require_auth layer; axum panics on a duplicate exact path+method so
    // collisions surface at startup.
    for r in extra {
        protected = protected.merge(r);
    }

    let protected = protected.route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        require_auth,
    ));

    public.merge(protected).with_state(state)
}
```

- [ ] **Step 2: Update `main.rs` caller**

In `crates/bin/src/main.rs`, line 59, change:

```rust
    let mut app = build_router(state);
```

to:

```rust
    let mut app = build_router(state, vec![]);
```

- [ ] **Step 3: Update test harness caller and add router-injection helper**

In `crates/bin/tests/common/mod.rs`:

Change the `build_router(state)` call (line 91) to `build_router(state, routers)` and thread an `extra` argument through `spawn_full`. Replace the four spawn methods (lines 36-50) and the `spawn_full` signature line (50) with:

```rust
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

    async fn spawn_full(
        docs_enabled: bool,
        hook: Arc<dyn WriteHook>,
        routers: Vec<axum::Router<AppState>>,
    ) -> Self {
```

Then at line 91 change:

```rust
        let app = build_router(state);
```

to:

```rust
        let app = build_router(state, routers);
```

`AppState` is already imported in this file's `use ferrum_http::{...}` line; `axum` is a workspace dep of the bin crate, so `axum::Router` resolves without a new import.

- [ ] **Step 4: Build to verify the refactor compiles**

Run: `cargo build -p ferrum-http -p ferrum`
Expected: builds clean (warnings about the unused `spawn_with_routers` are suppressed by `#[allow(dead_code)]`).

- [ ] **Step 5: Run existing tests to confirm no regression**

Run: `cargo test -p ferrum --test integration_smoke`
Expected: PASS — the empty-vec refactor changed no behavior.

- [ ] **Step 6: Write the failing integration test**

Create `crates/bin/tests/custom_routers.rs`:

```rust
mod common;

use axum::extract::State;
use axum::routing::get;
use axum::Router;
use common::TestApp;
use ferrum_http::AppState;

/// A custom endpoint injected by the bin. Returns 200 with the configured API
/// version, proving the `State<AppState>` extractor resolves on an injected
/// route. Sits under `/api/_probe`, behind `require_auth`.
async fn probe(State(state): State<AppState>) -> String {
    state.config.api_version.clone()
}

fn extra() -> Vec<Router<AppState>> {
    vec![Router::new().route("/api/_probe", get(probe))]
}

#[tokio::test]
async fn injected_route_is_reachable_with_auth() {
    let app = TestApp::spawn_with_routers(extra()).await;

    let resp = app
        .admin(app.client.get(app.url("/api/_probe")))
        .send()
        .await
        .expect("probe request");

    assert_eq!(resp.status(), 200, "authed probe should reach injected route");
    assert_eq!(resp.text().await.expect("body"), "test", "probe returns api_version from AppState");
}

#[tokio::test]
async fn injected_route_requires_auth() {
    let app = TestApp::spawn_with_routers(extra()).await;

    let resp = app
        .client
        .get(app.url("/api/_probe"))
        .send()
        .await
        .expect("probe request");

    assert_eq!(resp.status(), 401, "unauthed probe should be rejected by require_auth");
}
```

- [ ] **Step 7: Run the new test to verify it passes**

Run: `cargo test -p ferrum --test custom_routers`
Expected: PASS — both tests green. (`spawn_with_routers` is now actually used, so the dead-code allow is harmless.)

If `injected_route_requires_auth` returns 200 instead of 401, the extras were merged outside the `require_auth` layer — re-check Step 1 places the merge loop before `route_layer`.

- [ ] **Step 8: Commit**

```bash
git add crates/http/src/routes/mod.rs crates/bin/src/main.rs crates/bin/tests/common/mod.rs crates/bin/tests/custom_routers.rs
git commit -m "feat(http): inject custom routers via build_router extra param"
```

---

## Self-Review

- **Spec coverage.** API change (Step 1), merge-after-built-ins + auth-gating + panic-on-collision semantics (Step 1 comment + Step 6/7 tests), both callers updated (Steps 2-3), protected-only / plain-Vec scope (no public-extras param added), testing contract — reachable-with-auth + 401-without (Step 6). All spec sections mapped.
- **Placeholders.** None — every code step shows full code; commands have expected output.
- **Type consistency.** `build_router(state, extra: Vec<Router<AppState>>)` used identically in mod.rs, main.rs, common/mod.rs. `spawn_with_routers(Vec<axum::Router<AppState>>)` defined in Step 3, called in Step 6. `probe`/`extra` helper names consistent within the test file.
