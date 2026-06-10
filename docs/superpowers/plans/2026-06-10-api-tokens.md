# API Tokens Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow admins to create scoped, optionally-expiring API tokens that authenticate via `Authorization: Bearer <token>` for external/frontend consumers.

**Architecture:** Token format `rat_<32-hex-bytes>` stored as SHA-256 hash in `_api_tokens`. Auth middleware distinguishes JWT (3 dots) from opaque token; token path does a hash-lookup + `last_used_at` update. A new `Principal::ApiToken` variant carries scopes; `RoleAuthz` maps scopes to `Action`.

**Tech Stack:** Rust (axum, sqlx, sha2), React + TypeScript (react-router-dom, existing `apiFetch`)

---

## File Map

| File | Change |
|------|--------|
| `crates/schema/migrations/0007_api_tokens.sql` | Create — new migration |
| `crates/core/src/principal.rs` | Modify — add `ApiToken` variant + `action_to_scope` |
| `crates/sql/src/api_tokens.rs` | Create — DB ops: insert, list, delete, lookup_by_hash |
| `crates/sql/src/lib.rs` | Modify — pub mod + re-exports |
| `crates/http/Cargo.toml` | Modify — add `sha2` dependency |
| `crates/http/src/middleware/auth.rs` | Modify — token branch in `require_auth` |
| `crates/http/src/state.rs` | Modify — `RoleAuthz` new arm for `ApiToken` |
| `crates/http/src/routes/api_tokens.rs` | Create — GET/POST/DELETE `/api/admin/tokens` |
| `crates/http/src/routes/mod.rs` | Modify — merge `api_tokens::router()` |
| `crates/bin/tests/integration_api_tokens.rs` | Create — integration tests |
| `ui/src/api/types.ts` | Modify — add `ApiToken`, `NewApiToken` types |
| `ui/src/api/endpoints.ts` | Modify — add `listApiTokens`, `createApiToken`, `revokeApiToken` |
| `ui/src/screens/ApiTokens.tsx` | Create — token list + create modal + revoke confirm |
| `ui/src/App.tsx` | Modify — add `/settings/api-tokens` route |
| `ui/src/components/shell.tsx` | Modify — fix nav link to `/settings/api-tokens` |

---

## Task 1: Migration

**Files:**
- Create: `crates/schema/migrations/0007_api_tokens.sql`

- [ ] **Step 1: Write the migration**

```sql
CREATE TABLE _api_tokens (
  id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
  name         TEXT        NOT NULL,
  token_hash   TEXT        NOT NULL UNIQUE,
  scopes       TEXT[]      NOT NULL,
  expires_at   TIMESTAMPTZ,
  last_used_at TIMESTAMPTZ,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- [ ] **Step 2: Verify migration runs**

```bash
cargo test --workspace --test integration_schema 2>&1 | tail -5
```

Expected: all existing tests still pass (migration auto-runs via `MIGRATOR` in TestApp).

- [ ] **Step 3: Commit**

```bash
git add crates/schema/migrations/0007_api_tokens.sql
git commit -m "feat(sql): migration 0007 — _api_tokens table"
```

---

## Task 2: `Principal::ApiToken` variant + `action_to_scope`

**Files:**
- Modify: `crates/core/src/principal.rs`

- [ ] **Step 1: Add the variant and helper**

Replace the existing `Principal` enum and add `action_to_scope` in `crates/core/src/principal.rs`:

```rust
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Principal {
    /// An authenticated user, built from verified JWT claims.
    User {
        id: Uuid,
        email: String,
        roles: Vec<String>,
    },
    /// An API token, built from a DB lookup. Carries explicit action scopes.
    ApiToken {
        id: Uuid,
        scopes: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SchemaRead,
    SchemaWrite,
    ContentRead,
    ContentWrite,
    UserRead,
    UserWrite,
}

impl Principal {
    pub fn kind(&self) -> &'static str {
        match self {
            Principal::User { .. } => "user",
            Principal::ApiToken { .. } => "api_token",
        }
    }
}

/// Maps an `Action` to its wire scope string.
pub fn action_to_scope(action: Action) -> &'static str {
    match action {
        Action::ContentRead  => "content:read",
        Action::ContentWrite => "content:write",
        Action::SchemaRead   => "schema:read",
        Action::SchemaWrite  => "schema:write",
        Action::UserRead     => "user:read",
        Action::UserWrite    => "user:write",
    }
}

/// Hardcoded role → permission map. Unknown roles grant nothing.
/// `admin` = full access; `editor` = content read/write; `viewer` = content read.
pub fn role_allows(role: &str, action: Action) -> bool {
    use Action::*;
    match role {
        "admin" => true,
        "editor" => matches!(action, ContentRead | ContentWrite),
        "viewer" => matches!(action, ContentRead),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_allows_everything() {
        for a in [Action::SchemaRead, Action::SchemaWrite, Action::ContentRead, Action::ContentWrite] {
            assert!(role_allows("admin", a), "admin should allow {a:?}");
        }
    }

    #[test]
    fn admin_allows_user_actions() {
        assert!(role_allows("admin", Action::UserRead));
        assert!(role_allows("admin", Action::UserWrite));
    }

    #[test]
    fn non_admin_denied_user_actions() {
        for role in ["editor", "viewer", "ghost"] {
            assert!(!role_allows(role, Action::UserRead), "{role} UserRead");
            assert!(!role_allows(role, Action::UserWrite), "{role} UserWrite");
        }
    }

    #[test]
    fn editor_content_only() {
        assert!(role_allows("editor", Action::ContentRead));
        assert!(role_allows("editor", Action::ContentWrite));
        assert!(!role_allows("editor", Action::SchemaRead));
        assert!(!role_allows("editor", Action::SchemaWrite));
    }

    #[test]
    fn viewer_read_only() {
        assert!(role_allows("viewer", Action::ContentRead));
        assert!(!role_allows("viewer", Action::ContentWrite));
        assert!(!role_allows("viewer", Action::SchemaRead));
    }

    #[test]
    fn unknown_role_allows_nothing() {
        for a in [Action::SchemaRead, Action::SchemaWrite, Action::ContentRead, Action::ContentWrite] {
            assert!(!role_allows("ghost", a));
        }
    }

    #[test]
    fn action_to_scope_round_trips() {
        assert_eq!(action_to_scope(Action::ContentRead), "content:read");
        assert_eq!(action_to_scope(Action::ContentWrite), "content:write");
        assert_eq!(action_to_scope(Action::SchemaRead), "schema:read");
        assert_eq!(action_to_scope(Action::SchemaWrite), "schema:write");
        assert_eq!(action_to_scope(Action::UserRead), "user:read");
        assert_eq!(action_to_scope(Action::UserWrite), "user:write");
    }

    #[test]
    fn api_token_kind() {
        let p = Principal::ApiToken { id: Uuid::nil(), scopes: vec![] };
        assert_eq!(p.kind(), "api_token");
    }
}
```

- [ ] **Step 2: Fix exhaustive match in `users.rs` handler**

In `crates/http/src/routes/users.rs` the function `principal_id` matches only `Principal::User`. Add the new arm:

```rust
fn principal_id(p: &Principal) -> Uuid {
    match p {
        Principal::User { id, .. } => *id,
        Principal::ApiToken { id, .. } => *id,
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --workspace 2>&1 | tail -15
```

Expected: all existing tests pass, no `non-exhaustive patterns` errors.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/principal.rs crates/http/src/routes/users.rs
git commit -m "feat(core): Principal::ApiToken variant + action_to_scope"
```

---

## Task 3: `RoleAuthz` new arm

**Files:**
- Modify: `crates/http/src/state.rs`

- [ ] **Step 1: Update the import and `RoleAuthz` impl**

In `crates/http/src/state.rs`, update the import and `RoleAuthz`:

```rust
use rustapi_core::{action_to_scope, role_allows, Action, Error, Event, Principal};
```

Replace the `RoleAuthz` impl:

```rust
#[async_trait]
impl Authz for RoleAuthz {
    async fn can(&self, principal: &Principal, action: Action, _content_type: &str) -> bool {
        match principal {
            Principal::User { roles, .. } => roles.iter().any(|r| role_allows(r, action)),
            Principal::ApiToken { scopes, .. } => {
                let required = action_to_scope(action);
                scopes.iter().any(|s| s == required)
            }
        }
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 3: Commit**

```bash
git add crates/http/src/state.rs
git commit -m "feat(http): RoleAuthz handles Principal::ApiToken scopes"
```

---

## Task 4: SQL module for token operations

**Files:**
- Create: `crates/sql/src/api_tokens.rs`
- Modify: `crates/sql/src/lib.rs`

- [ ] **Step 1: Add `sha2` to `crates/sql/Cargo.toml`**

Open `crates/sql/Cargo.toml` and add under `[dependencies]`:

```toml
sha2 = "0.10"
hex = "0.4"
```

- [ ] **Step 2: Create `crates/sql/src/api_tokens.rs`**

```rust
//! DB operations for _api_tokens.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ApiToken {
    pub id: Uuid,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

pub async fn insert_token(
    pool: &PgPool,
    name: &str,
    raw_token: &str,
    scopes: &[String],
    expires_at: Option<DateTime<Utc>>,
) -> Result<ApiToken, sqlx::Error> {
    let hash = hash_token(raw_token);
    sqlx::query_as!(
        ApiToken,
        r#"
        INSERT INTO _api_tokens (name, token_hash, scopes, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, scopes, expires_at, last_used_at, created_at
        "#,
        name,
        hash,
        scopes,
        expires_at,
    )
    .fetch_one(pool)
    .await
}

pub async fn list_tokens(pool: &PgPool) -> Result<Vec<ApiToken>, sqlx::Error> {
    sqlx::query_as!(
        ApiToken,
        r#"
        SELECT id, name, scopes, expires_at, last_used_at, created_at
        FROM _api_tokens
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn delete_token(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM _api_tokens WHERE id = $1",
        id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Look up a token by its SHA-256 hash. On hit, update `last_used_at` to now
/// and return the row. Returns `None` if no matching token exists.
pub async fn lookup_by_hash(pool: &PgPool, raw_token: &str) -> Result<Option<ApiToken>, sqlx::Error> {
    let hash = hash_token(raw_token);
    sqlx::query_as!(
        ApiToken,
        r#"
        UPDATE _api_tokens
        SET last_used_at = now()
        WHERE token_hash = $1
        RETURNING id, name, scopes, expires_at, last_used_at, created_at
        "#,
        hash,
    )
    .fetch_optional(pool)
    .await
}
```

- [ ] **Step 3: Export from `crates/sql/src/lib.rs`**

Add at the top of `crates/sql/src/lib.rs`:

```rust
pub mod api_tokens;
pub use api_tokens::{delete_token, hash_token, insert_token, list_tokens, lookup_by_hash, ApiToken};
```

- [ ] **Step 4: Run tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass, no compile errors.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/Cargo.toml crates/sql/src/api_tokens.rs crates/sql/src/lib.rs
git commit -m "feat(sql): api_tokens — insert, list, delete, lookup_by_hash"
```

---

## Task 5: Auth middleware — token branch

**Files:**
- Modify: `crates/http/Cargo.toml`
- Modify: `crates/http/src/middleware/auth.rs`

- [ ] **Step 1: Add `sha2` dep to `crates/http/Cargo.toml`**

The `sha2` hashing is done in `rustapi_sql::hash_token`, so `crates/http` just needs `rustapi_sql` (already a dep). No Cargo change needed — skip to Step 2.

- [ ] **Step 2: Replace `require_auth` in `crates/http/src/middleware/auth.rs`**

```rust
//! Bearer-JWT / API-token auth middleware.

use crate::auth::jwt;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;
use rustapi_core::{Error, Principal};
use rustapi_sql::lookup_by_hash;

pub async fn require_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(ApiError(Error::Unauthorized))?;

    // JWTs have exactly 3 segments separated by '.'.
    let principal = if bearer.chars().filter(|&c| c == '.').count() == 2 {
        // --- JWT path (unchanged) ---
        let claims = jwt::verify(state.config.jwt_secret.as_bytes(), bearer)
            .map_err(|_| ApiError(Error::Unauthorized))?;
        Principal::User {
            id: claims.sub,
            email: claims.email,
            roles: claims.roles,
        }
    } else {
        // --- API token path ---
        let token = lookup_by_hash(&state.pool, bearer)
            .await
            .map_err(|e| ApiError(Error::Internal(e.into())))?
            .ok_or(ApiError(Error::Unauthorized))?;

        // Check expiry.
        if let Some(exp) = token.expires_at {
            if exp < Utc::now() {
                return Err(ApiError(Error::Unauthorized));
            }
        }

        Principal::ApiToken {
            id: token.id,
            scopes: token.scopes,
        }
    };

    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/middleware/auth.rs
git commit -m "feat(http): require_auth — API token lookup branch"
```

---

## Task 6: Admin token routes

**Files:**
- Create: `crates/http/src/routes/api_tokens.rs`
- Modify: `crates/http/src/routes/mod.rs`

- [ ] **Step 1: Create `crates/http/src/routes/api_tokens.rs`**

```rust
//! /api/admin/tokens — CRUD for API tokens (admin-only).

use crate::error::ApiError;
use crate::routes::content::db;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{DateTime, Utc};
use rustapi_core::{Action, Error, Principal};
use rustapi_sql::{delete_token, insert_token, list_tokens};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/tokens", get(list).post(create))
        .route("/api/admin/tokens/:id", axum::routing::delete(revoke))
}

async fn ensure_admin(state: &AppState, principal: &Principal) -> Result<(), ApiError> {
    if !state.authz.can(principal, Action::UserWrite, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

#[derive(Serialize)]
struct TokenView {
    id: Uuid,
    name: String,
    scopes: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct CreateTokenResponse {
    token: String,
    #[serde(flatten)]
    meta: TokenView,
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    scopes: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
}

async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<TokenView>>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let rows = list_tokens(&state.pool).await.map_err(db)?;
    Ok(Json(rows.into_iter().map(|t| TokenView {
        id: t.id,
        name: t.name,
        scopes: t.scopes,
        expires_at: t.expires_at,
        last_used_at: t.last_used_at,
        created_at: t.created_at,
    }).collect()))
}

async fn create(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<CreateTokenResponse>), ApiError> {
    ensure_admin(&state, &principal).await?;

    if body.scopes.is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::field("scopes", "at least one scope is required"),
        )));
    }
    if body.name.trim().is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::field("name", "name is required"),
        )));
    }

    // Generate raw token: rat_ + 32 random bytes as hex.
    let raw = format!("rat_{}", hex::encode(generate_bytes()));

    let row = insert_token(&state.pool, &body.name, &raw, &body.scopes, body.expires_at)
        .await
        .map_err(db)?;

    Ok((StatusCode::CREATED, Json(CreateTokenResponse {
        token: raw,
        meta: TokenView {
            id: row.id,
            name: row.name,
            scopes: row.scopes,
            expires_at: row.expires_at,
            last_used_at: row.last_used_at,
            created_at: row.created_at,
        },
    })))
}

async fn revoke(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure_admin(&state, &principal).await?;
    let deleted = delete_token(&state.pool, id).await.map_err(db)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(Error::NotFound))
    }
}

fn generate_bytes() -> [u8; 32] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Use rand if available; otherwise fall back to uuid entropy.
    // rustapi already depends on `uuid` with the `v4` feature which uses
    // getrandom internally — borrow that entropy via two v4 UUIDs.
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(a.as_bytes());
    out[16..].copy_from_slice(b.as_bytes());
    out
}
```

Note: add `hex = "0.4"` to `crates/http/Cargo.toml` under `[dependencies]`.

- [ ] **Step 2: Add `hex` dep to `crates/http/Cargo.toml`**

```toml
hex = "0.4"
```

- [ ] **Step 3: Register the router in `crates/http/src/routes/mod.rs`**

Add `pub mod api_tokens;` with the other mods, and merge the router inside `build_router`:

```rust
pub mod api_tokens;
```

Inside `build_router`, add to `protected`:

```rust
.merge(api_tokens::router())
```

- [ ] **Step 4: Run tests**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/routes/api_tokens.rs crates/http/src/routes/mod.rs crates/http/Cargo.toml
git commit -m "feat(http): /api/admin/tokens — list, create, revoke"
```

---

## Task 7: Integration tests

**Files:**
- Create: `crates/bin/tests/integration_api_tokens.rs`

- [ ] **Step 1: Write the failing tests**

```rust
mod common;
use common::TestApp;
use serde_json::{json, Value};

async fn make_article_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article",
            "display_name": "Article",
            "fields": [{"name": "title", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_token(app: &TestApp, scopes: &[&str], expires_at: Option<&str>) -> String {
    let mut body = json!({ "name": "test", "scopes": scopes });
    if let Some(exp) = expires_at {
        body["expires_at"] = json!(exp);
    }
    let resp = app
        .admin(app.client.post(app.url("/api/admin/tokens")))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let v: Value = resp.json().await.unwrap();
    v["token"].as_str().unwrap().to_string()
}

fn with_token(app: &TestApp, builder: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    builder.header("authorization", format!("Bearer {token}"))
}

#[tokio::test]
async fn content_read_token_can_list() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read"], None).await;

    let resp = with_token(&app, app.client.get(app.url("/api/article")), &token)
        .send().await.unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn content_read_token_cannot_create() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read"], None).await;

    let resp = with_token(&app, app.client.post(app.url("/api/article")), &token)
        .json(&json!({"title": "hi"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 403, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn content_readwrite_token_can_create() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read", "content:write"], None).await;

    let resp = with_token(&app, app.client.post(app.url("/api/article")), &token)
        .json(&json!({"title": "hi"}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn expired_token_returns_401() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    // expires_at in the past
    let token = create_token(&app, &["content:read"], Some("2000-01-01T00:00:00Z")).await;

    let resp = with_token(&app, app.client.get(app.url("/api/article")), &token)
        .send().await.unwrap();
    assert_eq!(resp.status(), 401, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn revoked_token_returns_401() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read"], None).await;

    // Get the token id from list
    let list_resp = app
        .admin(app.client.get(app.url("/api/admin/tokens")))
        .send().await.unwrap();
    assert_eq!(list_resp.status(), 200);
    let list: Vec<Value> = list_resp.json().await.unwrap();
    let id = list[0]["id"].as_str().unwrap();

    // Revoke
    let del = app.admin(app.client.delete(app.url(&format!("/api/admin/tokens/{id}"))))
        .send().await.unwrap();
    assert_eq!(del.status(), 204);

    // Now the token should be rejected
    let resp = with_token(&app, app.client.get(app.url("/api/article")), &token)
        .send().await.unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn unknown_token_returns_401() {
    let app = TestApp::spawn().await;
    let resp = with_token(&app, app.client.get(app.url("/api/article")), "rat_notarealtoken000000000000000000000000000000000000000000000000")
        .send().await.unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn jwt_auth_still_works() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let resp = app.admin(app.client.get(app.url("/api/article"))).send().await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn create_token_no_scopes_returns_422() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/api/admin/tokens")))
        .json(&json!({"name": "bad", "scopes": []}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn last_used_at_updated_on_auth() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read"], None).await;

    // Before use — last_used_at should be null
    let list: Vec<Value> = app.admin(app.client.get(app.url("/api/admin/tokens")))
        .send().await.unwrap().json().await.unwrap();
    assert!(list[0]["last_used_at"].is_null());

    // Use the token
    with_token(&app, app.client.get(app.url("/api/article")), &token)
        .send().await.unwrap();

    // After use — last_used_at should be set
    let list2: Vec<Value> = app.admin(app.client.get(app.url("/api/admin/tokens")))
        .send().await.unwrap().json().await.unwrap();
    assert!(!list2[0]["last_used_at"].is_null());
}
```

- [ ] **Step 2: Run tests — expect failures**

```bash
cargo test --test integration_api_tokens 2>&1 | tail -20
```

Expected: compile or runtime failures (routes not yet wired at this point if running mid-plan — if Task 6 is done, tests should mostly pass).

- [ ] **Step 3: Run full suite — all green**

```bash
cargo test --workspace 2>&1 | tail -15
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/bin/tests/integration_api_tokens.rs
git commit -m "test(bin): integration tests for API tokens"
```

---

## Task 8: UI — types and API client

**Files:**
- Modify: `ui/src/api/types.ts`
- Modify: `ui/src/api/endpoints.ts`

- [ ] **Step 1: Add types to `ui/src/api/types.ts`**

Append at the end of the file:

```typescript
export interface ApiToken {
  id: string;
  name: string;
  scopes: string[];
  expires_at: string | null;
  last_used_at: string | null;
  created_at: string;
}

export interface NewApiToken {
  name: string;
  scopes: string[];
  expires_at?: string | null;
}

export interface CreatedApiToken extends ApiToken {
  token: string; // raw token — shown once
}
```

- [ ] **Step 2: Add endpoints to `ui/src/api/endpoints.ts`**

Add to the imports at the top:

```typescript
import type { ..., ApiToken, CreatedApiToken, NewApiToken } from "./types";
```

Append the three functions:

```typescript
export function listApiTokens(): Promise<ApiToken[]> {
  return apiFetch<ApiToken[]>("/api/admin/tokens");
}

export function createApiToken(body: NewApiToken): Promise<CreatedApiToken> {
  return apiFetch<CreatedApiToken>("/api/admin/tokens", { method: "POST", body });
}

export function revokeApiToken(id: string): Promise<void> {
  return apiFetch<void>(`/api/admin/tokens/${encodeURIComponent(id)}`, { method: "DELETE" });
}
```

- [ ] **Step 3: Typecheck**

```bash
cd ui && pnpm typecheck
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add ui/src/api/types.ts ui/src/api/endpoints.ts
git commit -m "feat(ui): ApiToken types + listApiTokens/createApiToken/revokeApiToken endpoints"
```

---

## Task 9: UI — ApiTokens screen

**Files:**
- Create: `ui/src/screens/ApiTokens.tsx`

- [ ] **Step 1: Create `ui/src/screens/ApiTokens.tsx`**

```tsx
import { useState } from "react";
import { Icons } from "../components/icons";
import { LoadingState, EmptyState, Notice } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { listApiTokens, createApiToken, revokeApiToken } from "../api/endpoints";
import type { ApiToken, NewApiToken } from "../api/types";
import { relTime } from "../util";
import { ApiError } from "../api/client";

const ALL_SCOPES = [
  { key: "content:read",  label: "Content — Read" },
  { key: "content:write", label: "Content — Write" },
  { key: "schema:read",   label: "Schema — Read" },
  { key: "schema:write",  label: "Schema — Write" },
  { key: "user:read",     label: "Users — Read" },
  { key: "user:write",    label: "Users — Write" },
];

export function ApiTokens() {
  const tokens = useResource(() => listApiTokens(), []);
  const [creating, setCreating] = useState(false);
  const [revokeTarget, setRevokeTarget] = useState<ApiToken | null>(null);
  const [revealed, setRevealed] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const copy = async (text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>API Tokens</h1>
          <p className="rs-cm-sub">{(tokens.data ?? []).length} token{(tokens.data ?? []).length === 1 ? "" : "s"}</p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => setCreating(true)}>
          <Icons.plus size={16} /> Create token
        </button>
      </div>

      {tokens.loading && <LoadingState />}
      {tokens.error && <EmptyState>{tokens.error.message}</EmptyState>}

      {!tokens.loading && !tokens.error && (
        <div className="rs-table-wrap">
          <table className="rs-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Scopes</th>
                <th>Expires</th>
                <th>Last used</th>
                <th>Created</th>
                <th className="rs-col-act" />
              </tr>
            </thead>
            <tbody>
              {(tokens.data ?? []).map((t) => (
                <tr key={t.id}>
                  <td className="rs-cell-title"><span className="rs-title-text">{t.name}</span></td>
                  <td>
                    <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
                      {t.scopes.map((s) => <span key={s} className="rs-type-pill">{s}</span>)}
                    </div>
                  </td>
                  <td className="rs-cell-muted">
                    {t.expires_at
                      ? new Date(t.expires_at) < new Date()
                        ? <span className="rs-badge rs-badge--warn">Expired</span>
                        : relTime(t.expires_at)
                      : "Never"}
                  </td>
                  <td className="rs-cell-muted">{t.last_used_at ? relTime(t.last_used_at) : "—"}</td>
                  <td className="rs-cell-muted">{relTime(t.created_at)}</td>
                  <td className="rs-col-act">
                    <button
                      className="rs-row-btn rs-danger"
                      title="Revoke token"
                      onClick={() => setRevokeTarget(t)}
                    >
                      <Icons.trash size={16} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {(tokens.data ?? []).length === 0 && (
            <div className="rs-empty">No API tokens yet.</div>
          )}
        </div>
      )}

      {creating && (
        <CreateModal
          revealed={revealed}
          copied={copied}
          onCopy={copy}
          onCreated={(raw) => { setRevealed(raw); tokens.refetch(); }}
          onClose={() => { setCreating(false); setRevealed(null); setCopied(false); }}
        />
      )}

      {revokeTarget && (
        <RevokeModal
          token={revokeTarget}
          onRevoked={() => { setRevokeTarget(null); tokens.refetch(); }}
          onClose={() => setRevokeTarget(null)}
        />
      )}
    </div>
  );
}

function CreateModal({
  revealed,
  copied,
  onCopy,
  onCreated,
  onClose,
}: {
  revealed: string | null;
  copied: boolean;
  onCopy: (t: string) => void;
  onCreated: (raw: string) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [scopes, setScopes] = useState<string[]>([]);
  const [expires, setExpires] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggleScope = (s: string) =>
    setScopes((prev) => prev.includes(s) ? prev.filter((x) => x !== s) : [...prev, s]);

  const submit = async () => {
    setError(null);
    if (!name.trim()) { setError("Name is required."); return; }
    if (scopes.length === 0) { setError("Select at least one scope."); return; }
    setSaving(true);
    try {
      const body: NewApiToken = { name: name.trim(), scopes };
      if (expires) body.expires_at = new Date(expires).toISOString();
      const result = await createApiToken(body);
      onCreated(result.token);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Failed to create token.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rs-modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="rs-modal">
        <div className="rs-modal-head">
          <h2>{revealed ? "Token created" : "Create API token"}</h2>
          <button className="rs-modal-close" onClick={onClose}><Icons.x size={18} /></button>
        </div>
        <div className="rs-modal-body">
          {revealed ? (
            <>
              <Notice>Copy this token now — it won't be shown again.</Notice>
              <div className="rs-token-reveal">
                <code className="rs-mono">{revealed}</code>
                <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => onCopy(revealed)}>
                  {copied ? "Copied!" : <><Icons.copy size={14} /> Copy</>}
                </button>
              </div>
            </>
          ) : (
            <>
              {error && <Notice>{error}</Notice>}
              <div className="rs-field-row">
                <label className="rs-label">Name</label>
                <input className="rs-input" value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Website frontend" />
              </div>
              <div className="rs-field-row">
                <label className="rs-label">Scopes</label>
                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  {ALL_SCOPES.map((s) => (
                    <label key={s.key} style={{ display: "flex", alignItems: "center", gap: 8, cursor: "pointer" }}>
                      <input type="checkbox" checked={scopes.includes(s.key)} onChange={() => toggleScope(s.key)} />
                      <span>{s.label}</span>
                    </label>
                  ))}
                </div>
              </div>
              <div className="rs-field-row">
                <label className="rs-label">Expires (optional)</label>
                <input className="rs-input" type="date" value={expires} onChange={(e) => setExpires(e.target.value)} />
              </div>
            </>
          )}
        </div>
        <div className="rs-modal-foot">
          {revealed ? (
            <button className="rs-btn rs-btn--primary" onClick={onClose}>Done</button>
          ) : (
            <>
              <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
              <button className="rs-btn rs-btn--primary" onClick={submit} disabled={saving}>
                {saving ? "Creating…" : "Create token"}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function RevokeModal({ token, onRevoked, onClose }: { token: ApiToken; onRevoked: () => void; onClose: () => void }) {
  const [revoking, setRevoking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = async () => {
    setRevoking(true);
    try {
      await revokeApiToken(token.id);
      onRevoked();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Failed to revoke token.");
      setRevoking(false);
    }
  };

  return (
    <div className="rs-modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="rs-modal">
        <div className="rs-modal-head">
          <h2>Revoke token</h2>
          <button className="rs-modal-close" onClick={onClose}><Icons.x size={18} /></button>
        </div>
        <div className="rs-modal-body">
          {error && <Notice>{error}</Notice>}
          <p>Revoke <strong>{token.name}</strong>? Any client using this token will lose access immediately. This cannot be undone.</p>
        </div>
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
          <button className="rs-btn rs-btn--primary rs-danger" onClick={confirm} disabled={revoking}>
            {revoking ? "Revoking…" : "Revoke"}
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Typecheck**

```bash
cd ui && pnpm typecheck
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/ApiTokens.tsx
git commit -m "feat(ui): ApiTokens screen — list, create modal, revoke confirm"
```

---

## Task 10: Wire route + fix nav link

**Files:**
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/components/shell.tsx`
- Modify: `ui/src/screens/Settings.tsx`

- [ ] **Step 1: Add route in `ui/src/App.tsx`**

Add import:

```typescript
import { ApiTokens } from "./screens/ApiTokens";
```

Add route inside the protected `<Route>` block, alongside the existing settings routes:

```tsx
<Route path="settings/api-tokens" element={<ApiTokens />} />
```

- [ ] **Step 2: Fix nav link in `ui/src/components/shell.tsx`**

Find the `SettingsPanel` items array. Change:

```typescript
{ label: "API tokens", to: "/settings" },
```

to:

```typescript
{ label: "API tokens", to: "/settings/api-tokens" },
```

- [ ] **Step 3: Strip placeholder content from `ui/src/screens/Settings.tsx`**

The current `Settings.tsx` contains the hardcoded placeholder token table. Replace the entire file with just the sign-out section (the token management has moved to `/settings/api-tokens`):

```tsx
import { useNavigate } from "react-router-dom";
import { clearToken } from "../auth";

export function Settings() {
  const navigate = useNavigate();
  const signOut = () => {
    clearToken();
    navigate("/login", { replace: true });
  };
  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Settings</h1>
        </div>
      </div>
      <div className="rs-setting-row">
        <div className="rs-setting-meta">
          <strong>Session</strong>
          <span className="rs-cell-muted">
            Your admin key is stored in this browser. Sign out to clear it.
          </span>
        </div>
        <button className="rs-btn rs-btn--ghost rs-danger" onClick={signOut}>
          Sign out
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Add `.rs-token-reveal` style to `ui/src/styles.css`**

Append after the `.rs-cm-flash` lines:

```css
.rs-token-reveal { display: flex; align-items: center; gap: 10px; padding: 10px 12px; background: var(--surface-3); border: 1px solid var(--border); border-radius: 6px; margin-top: 12px; overflow-x: auto; }
.rs-token-reveal code { flex: 1; font-size: 12px; word-break: break-all; }
```

- [ ] **Step 5: Typecheck**

```bash
cd ui && pnpm typecheck
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add ui/src/App.tsx ui/src/components/shell.tsx ui/src/screens/Settings.tsx ui/src/styles.css
git commit -m "feat(ui): wire /settings/api-tokens route, fix nav link, strip placeholder"
```

---

## Task 11: Final verification

- [ ] **Step 1: Full backend test suite**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all tests pass including `integration_api_tokens`.

- [ ] **Step 2: Clippy**

```bash
cargo clippy --workspace --all-targets 2>&1 | grep "^error" | head -10
```

Expected: no errors.

- [ ] **Step 3: UI typecheck**

```bash
cd ui && pnpm typecheck
```

Expected: no errors.

- [ ] **Step 4: Final commit if any lint fixes needed, then done**
