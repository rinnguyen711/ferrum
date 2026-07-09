# Auth & Authz Slice 1 (Email/Password + JWT + RBAC) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the static `FERRUM_ADMIN_KEY` shared secret with email/password login that issues HS256 JWTs, plus multi-role RBAC and a first-run setup flow.

**Architecture:** A `_users` Postgres table holds Argon2id password hashes and a `roles TEXT[]`. Auth lives as a module inside `crates/http` (`auth/`). Login signs a stateless HS256 JWT; a `require_auth` middleware verifies the bearer token and injects `Principal::User`. A `RoleAuthz` impl of the existing `Authz` trait maps roles → `Action`s via a pure `role_allows` fn in core.

**Tech Stack:** Rust, Axum 0.7, sqlx 0.8 (Postgres), `argon2` (Argon2id), `jsonwebtoken` (HS256), testcontainers for integration tests.

**Spec:** `docs/superpowers/specs/2026-06-02-auth-email-password-jwt-design.md`

---

## File Structure

- `crates/core/src/principal.rs` — **modify**: replace `Principal::Admin` with `Principal::User { id, email, roles }`; add `role_allows(role, action)` pure fn.
- `crates/core/src/error.rs` — **modify**: add `Error::Forbidden`. (`Error::Conflict(String)` already exists.)
- `crates/http/src/error.rs` — **modify**: map `Forbidden` → 403; update `Unauthorized` message.
- `crates/schema/migrations/0002_users.sql` — **create**: `_users` table.
- `crates/http/src/auth/mod.rs` — **create**: `/auth/*` router + module decls + `RoleAuthz`.
- `crates/http/src/auth/password.rs` — **create**: Argon2id hash/verify.
- `crates/http/src/auth/jwt.rs` — **create**: `Claims`, sign/verify HS256.
- `crates/http/src/auth/users.rs` — **create**: DB queries (`count`, `find_by_email`, `insert`).
- `crates/http/src/auth/handlers.rs` — **create**: setup, login, me handlers.
- `crates/http/src/middleware/auth.rs` — **modify**: replace `require_admin_key` with `require_auth`.
- `crates/http/src/state.rs` — **modify**: drop `AlwaysAllow` usage in prod; add `jwt_secret` + `jwt_ttl_secs` to `AppConfig`; keep `AlwaysAllow` for tests.
- `crates/http/src/routes/mod.rs` — **modify**: mount `/auth` public + protected; swap middleware.
- `crates/http/src/routes/content.rs` — **modify**: `ensure()` authz-deny → `Forbidden` not `Unauthorized`.
- `crates/http/src/lib.rs` — **modify**: export `RoleAuthz`, auth router.
- `crates/http/Cargo.toml` + root `Cargo.toml` — **modify**: add `argon2`, `jsonwebtoken`.
- `crates/bin/src/config.rs` — **modify**: drop `admin_key`, add `jwt_secret` + `jwt_ttl_secs`.
- `crates/bin/src/main.rs` — **modify**: build `RoleAuthz`, pass JWT config.
- `crates/bin/tests/common/mod.rs` — **modify**: setup→login helper returning bearer token; `admin()` sets `Authorization`.
- `crates/bin/tests/integration_auth.rs` — **create**: setup/login/me integration tests.
- `docker-compose.yml`, `README.md` — **modify**: swap env vars, document setup.

---

## Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml` (workspace `[workspace.dependencies]`)
- Modify: `crates/http/Cargo.toml`

- [ ] **Step 1: Add to workspace dependencies**

In root `Cargo.toml`, under `[workspace.dependencies]`, after the `uuid` line add:

```toml
# Auth
argon2 = "0.5"
jsonwebtoken = "9"
```

- [ ] **Step 2: Add to http crate**

In `crates/http/Cargo.toml`, under `[dependencies]`, after the `url = "2"` line add:

```toml
argon2 = { workspace = true }
jsonwebtoken = { workspace = true }
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p ferrum-http`
Expected: compiles (new crates downloaded), no errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/http/Cargo.toml
git commit -m "build(http): add argon2 + jsonwebtoken deps"
```

---

## Task 2: Core — `role_allows` + `Principal::User`

**Files:**
- Modify: `crates/core/src/principal.rs`

- [ ] **Step 1: Write failing tests**

Replace the whole `crates/core/src/principal.rs` with the implementation below — but first confirm the test shape compiles against the new types. Add this `#[cfg(test)]` module at the bottom of the file (it references types defined in Step 2):

```rust
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
    fn user_principal_carries_roles() {
        let p = Principal::User {
            id: uuid::Uuid::nil(),
            email: "a@b.c".into(),
            roles: vec!["admin".into()],
        };
        assert_eq!(p.kind(), "user");
    }
}
```

- [ ] **Step 2: Write the implementation**

Replace the top of `crates/core/src/principal.rs` (everything above the test module) with:

```rust
//! Identity and authorization actions.

use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Principal {
    /// An authenticated user, built from verified JWT claims.
    User {
        id: Uuid,
        email: String,
        roles: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SchemaRead,
    SchemaWrite,
    ContentRead,
    ContentWrite,
}

impl Principal {
    pub fn kind(&self) -> &'static str {
        match self {
            Principal::User { .. } => "user",
        }
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
```

- [ ] **Step 3: Verify core compiles + tests fail-then-pass**

Run: `cargo test -p ferrum-core principal`
Expected: the 5 tests above PASS. (They are written against the new code, so this is the red→green checkpoint for the unit-tested logic.)

- [ ] **Step 4: Confirm `uuid` is a core dep**

`crates/core/Cargo.toml` already lists `uuid.workspace = true` (verified). No change needed.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/principal.rs
git commit -m "feat(core): Principal::User + role_allows RBAC map"
```

---

## Task 3: Core — `Error::Forbidden`

**Files:**
- Modify: `crates/core/src/error.rs`
- Modify: `crates/http/src/error.rs`

- [ ] **Step 1: Write failing test (http mapping)**

In `crates/http/src/error.rs`, inside the existing `#[cfg(test)] mod tests`, add:

```rust
    #[tokio::test]
    async fn forbidden_is_403() {
        let resp = ApiError(Error::Forbidden).into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["code"], "forbidden");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ferrum-http forbidden_is_403`
Expected: FAIL — compile error, `Error::Forbidden` does not exist.

- [ ] **Step 3: Add the variant in core**

In `crates/core/src/error.rs`, in `enum Error`, after the `Unauthorized` variant add:

```rust
    #[error("forbidden")]
    Forbidden,
```

- [ ] **Step 4: Map it in http**

In `crates/http/src/error.rs`, in the `match self.0` block, after the `Error::Unauthorized => ...` arm add:

```rust
            Error::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "insufficient permissions".to_string(), None),
```

Also change the existing `Unauthorized` arm message from `"missing or invalid API key"` to `"missing or invalid credentials"`:

```rust
            Error::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "missing or invalid credentials".to_string(), None),
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p ferrum-http forbidden_is_403`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/error.rs crates/http/src/error.rs
git commit -m "feat(core): add Error::Forbidden (403)"
```

---

## Task 4: Migration — `_users` table

**Files:**
- Create: `crates/schema/migrations/0002_users.sql`

- [ ] **Step 1: Create the migration file**

Write `crates/schema/migrations/0002_users.sql`:

```sql
CREATE TABLE IF NOT EXISTS _users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    roles         TEXT[] NOT NULL DEFAULT '{}',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS _users_email_lower ON _users (lower(email));
```

- [ ] **Step 2: Verify the migrator compiles it in**

`crates/schema/src/lib.rs` uses `sqlx::migrate!("./migrations")`, which embeds every `.sql` file at compile time. Run:

Run: `cargo build -p ferrum-schema`
Expected: compiles; the new migration is embedded (no SQL is executed at build).

- [ ] **Step 3: Commit**

```bash
git add crates/schema/migrations/0002_users.sql
git commit -m "feat(schema): _users migration"
```

---

## Task 5: Password hashing (Argon2id)

**Files:**
- Create: `crates/http/src/auth/password.rs`
- Modify: `crates/http/src/middleware/mod.rs` is unrelated; instead add `pub mod auth;` wiring in Task 9. For now, create the module file and declare it temporarily via a test-only path.

> NOTE: `crates/http/src/auth/mod.rs` is created in Task 9. To let this task's tests run now, create a minimal `auth/mod.rs` here declaring only `password`, and add `pub mod auth;` to `crates/http/src/lib.rs`. Task 9 expands `auth/mod.rs`.

- [ ] **Step 1: Create minimal module wiring**

Create `crates/http/src/auth/mod.rs`:

```rust
//! Authentication: password hashing, JWT, user store, and /auth routes.

pub mod password;
```

In `crates/http/src/lib.rs`, after `pub mod middleware;` add:

```rust
pub mod auth;
```

- [ ] **Step 2: Write the failing test**

Create `crates/http/src/auth/password.rs`:

```rust
//! Argon2id password hashing.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

/// Hash a plaintext password into an Argon2id PHC string.
pub fn hash(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a plaintext password against a stored PHC hash. Returns false on
/// mismatch or malformed hash (never errors out to the caller).
pub fn verify(password: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_true() {
        let phc = hash("hunter2-correct").unwrap();
        assert!(verify("hunter2-correct", &phc));
    }

    #[test]
    fn wrong_password_verify_false() {
        let phc = hash("hunter2-correct").unwrap();
        assert!(!verify("hunter2-wrong", &phc));
    }

    #[test]
    fn malformed_hash_verify_false() {
        assert!(!verify("anything", "not-a-phc-string"));
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p ferrum-http password::`
Expected: 3 tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/auth/mod.rs crates/http/src/auth/password.rs crates/http/src/lib.rs
git commit -m "feat(http): Argon2id password hash/verify"
```

---

## Task 6: JWT (HS256) sign + verify

**Files:**
- Create: `crates/http/src/auth/jwt.rs`
- Modify: `crates/http/src/auth/mod.rs` (declare `jwt`)

- [ ] **Step 1: Declare the module**

In `crates/http/src/auth/mod.rs`, add under the `password` line:

```rust
pub mod jwt;
```

- [ ] **Step 2: Write the implementation + tests**

Create `crates/http/src/auth/jwt.rs`:

```rust
//! HS256 JWT encode/decode.

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// User id.
    pub sub: Uuid,
    pub email: String,
    pub roles: Vec<String>,
    /// Issued-at (unix seconds).
    pub iat: i64,
    /// Expiry (unix seconds).
    pub exp: i64,
}

/// Sign claims for `sub`/`email`/`roles`, expiring `ttl_secs` from now.
pub fn sign(
    secret: &[u8],
    sub: Uuid,
    email: &str,
    roles: &[String],
    ttl_secs: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub,
        email: email.to_string(),
        roles: roles.to_vec(),
        iat: now,
        exp: now + ttl_secs,
    };
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret))
}

/// Verify an HS256 token and return its claims. Rejects bad signature / expiry.
pub fn verify(secret: &[u8], token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::new(Algorithm::HS256),
    )?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-at-least-32-bytes-long!!";

    #[test]
    fn round_trip() {
        let id = Uuid::new_v4();
        let token = sign(SECRET, id, "a@b.c", &["admin".into()], 3600).unwrap();
        let claims = verify(SECRET, &token).unwrap();
        assert_eq!(claims.sub, id);
        assert_eq!(claims.email, "a@b.c");
        assert_eq!(claims.roles, vec!["admin".to_string()]);
    }

    #[test]
    fn wrong_secret_rejected() {
        let token = sign(SECRET, Uuid::new_v4(), "a@b.c", &[], 3600).unwrap();
        assert!(verify(b"a-completely-different-secret-32xx!!", &token).is_err());
    }

    #[test]
    fn expired_rejected() {
        // ttl -10s → already expired.
        let token = sign(SECRET, Uuid::new_v4(), "a@b.c", &[], -10).unwrap();
        assert!(verify(SECRET, &token).is_err());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p ferrum-http jwt::`
Expected: 3 tests PASS. (`expired_rejected` relies on `jsonwebtoken`'s default 60s leeway being smaller than... — note: default leeway is 0 in v9, so -10s is reliably expired.)

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/auth/jwt.rs crates/http/src/auth/mod.rs
git commit -m "feat(http): HS256 JWT sign/verify"
```

---

## Task 7: User store (DB queries)

**Files:**
- Create: `crates/http/src/auth/users.rs`
- Modify: `crates/http/src/auth/mod.rs` (declare `users`)

- [ ] **Step 1: Declare the module**

In `crates/http/src/auth/mod.rs`, add:

```rust
pub mod users;
```

- [ ] **Step 2: Write the store (integration-tested in Task 11, not unit-tested here)**

Create `crates/http/src/auth/users.rs`:

```rust
//! `_users` table access.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub roles: Vec<String>,
}

/// Count users. Used by the self-closing setup endpoint.
pub async fn count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT count(*) FROM _users")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Look up by case-insensitive email. `None` if absent.
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String, String, Vec<String>)>(
        "SELECT id, email, password_hash, roles FROM _users WHERE lower(email) = lower($1)",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .map(|opt| {
        opt.map(|(id, email, password_hash, roles)| UserRow {
            id,
            email,
            password_hash,
            roles,
        })
    })
}

/// Insert a new user. Returns the created row. Caller pre-hashes the password.
pub async fn insert(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
    roles: &[String],
) -> Result<UserRow, sqlx::Error> {
    let (id, email, password_hash, roles) = sqlx::query_as::<_, (Uuid, String, String, Vec<String>)>(
        "INSERT INTO _users (email, password_hash, roles) VALUES ($1, $2, $3) \
         RETURNING id, email, password_hash, roles",
    )
    .bind(email)
    .bind(password_hash)
    .bind(roles)
    .fetch_one(pool)
    .await?;
    Ok(UserRow {
        id,
        email,
        password_hash,
        roles,
    })
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p ferrum-http`
Expected: compiles. (Queries are runtime — `query_as` string form — so no DB needed at build.)

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/auth/users.rs crates/http/src/auth/mod.rs
git commit -m "feat(http): _users store (count/find/insert)"
```

---

## Task 8: AppConfig + RoleAuthz

**Files:**
- Modify: `crates/http/src/state.rs`
- Modify: `crates/http/src/lib.rs`

- [ ] **Step 1: Write failing test for RoleAuthz**

In `crates/http/src/state.rs`, add a test module at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_core::Principal;
    use uuid::Uuid;

    fn user(roles: &[&str]) -> Principal {
        Principal::User {
            id: Uuid::nil(),
            email: "a@b.c".into(),
            roles: roles.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn role_authz_admin_can_write_schema() {
        let az = RoleAuthz;
        assert!(az.can(&user(&["admin"]), Action::SchemaWrite, "x").await);
    }

    #[tokio::test]
    async fn role_authz_viewer_cannot_write() {
        let az = RoleAuthz;
        assert!(!az.can(&user(&["viewer"]), Action::ContentWrite, "x").await);
        assert!(az.can(&user(&["viewer"]), Action::ContentRead, "x").await);
    }

    #[tokio::test]
    async fn role_authz_union_of_roles() {
        let az = RoleAuthz;
        // editor + viewer → still no schema write
        assert!(!az.can(&user(&["editor", "viewer"]), Action::SchemaWrite, "x").await);
        assert!(az.can(&user(&["editor", "viewer"]), Action::ContentWrite, "x").await);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ferrum-http role_authz`
Expected: FAIL — `RoleAuthz` does not exist.

- [ ] **Step 3: Add RoleAuthz + extend AppConfig**

In `crates/http/src/state.rs`:

(a) Update the `use` line to import `role_allows`:

```rust
use ferrum_core::{role_allows, Action, Event, Principal};
```

(b) After the `AlwaysAllow` impl block, add:

```rust
/// Production authorizer: unions the hardcoded permissions of a user's roles.
pub struct RoleAuthz;

#[async_trait]
impl Authz for RoleAuthz {
    async fn can(&self, principal: &Principal, action: Action, _content_type: &str) -> bool {
        match principal {
            Principal::User { roles, .. } => roles.iter().any(|r| role_allows(r, action)),
        }
    }
}
```

(c) In `struct AppConfig`, replace the `admin_key` field. Change:

```rust
#[derive(Clone)]
pub struct AppConfig {
    pub admin_key: String,
    pub page_size_max: u32,
}
```

to:

```rust
#[derive(Clone)]
pub struct AppConfig {
    /// HS256 signing secret for JWTs.
    pub jwt_secret: String,
    /// Access-token lifetime in seconds.
    pub jwt_ttl_secs: i64,
    pub page_size_max: u32,
}
```

- [ ] **Step 4: Export RoleAuthz**

In `crates/http/src/lib.rs`, update the state re-export line to include `RoleAuthz`:

```rust
pub use state::{AlwaysAllow, AppConfig, Authz, AppState, EventSink, NoopSink, RoleAuthz};
```

- [ ] **Step 5: Run RoleAuthz tests**

Run: `cargo test -p ferrum-http role_authz`
Expected: 3 tests PASS. (The crate won't fully build yet — `admin_key` references in `middleware/auth.rs`, `main.rs`, `common/mod.rs` still exist. Those are fixed in Tasks 9, 12, 11. If the test target fails to compile because of `middleware/auth.rs`, proceed to Task 9 first, then re-run.)

> If the crate fails to compile due to `admin_key` usage in `middleware/auth.rs`, do Task 9 next and run this test afterward. Commit this task's changes together with Task 9.

- [ ] **Step 6: Commit (after Task 9 if needed for compile)**

```bash
git add crates/http/src/state.rs crates/http/src/lib.rs
git commit -m "feat(http): RoleAuthz + JWT config fields on AppConfig"
```

---

## Task 9: `require_auth` middleware + `/auth` router + handlers

**Files:**
- Modify: `crates/http/src/middleware/auth.rs`
- Modify: `crates/http/src/auth/mod.rs`
- Create: `crates/http/src/auth/handlers.rs`
- Modify: `crates/http/src/routes/mod.rs`
- Modify: `crates/http/src/lib.rs`

- [ ] **Step 1: Replace the middleware**

Replace the entire contents of `crates/http/src/middleware/auth.rs` with:

```rust
//! Bearer-JWT auth middleware. Verifies an HS256 token and injects Principal::User.

use crate::auth::jwt;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use ferrum_core::{Error, Principal};

pub async fn require_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(ApiError(Error::Unauthorized))?;

    let claims = jwt::verify(state.config.jwt_secret.as_bytes(), token)
        .map_err(|_| ApiError(Error::Unauthorized))?;

    req.extensions_mut().insert(Principal::User {
        id: claims.sub,
        email: claims.email,
        roles: claims.roles,
    });
    Ok(next.run(req).await)
}
```

- [ ] **Step 2: Write the handlers**

Create `crates/http/src/auth/handlers.rs`:

```rust
//! /auth handlers: setup, login, me.

use crate::auth::{jwt, password, users};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Json, Extension};
use ferrum_core::{Error, Principal, ValidationErrors};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserView {
    pub id: uuid::Uuid,
    pub email: String,
    pub roles: Vec<String>,
}

fn validate_password(pw: &str) -> Result<(), ApiError> {
    if pw.len() < 8 {
        return Err(ApiError(Error::Validation(ValidationErrors::field(
            "password",
            "must be at least 8 characters",
        ))));
    }
    Ok(())
}

/// POST /auth/setup — create the first admin. Self-closes once any user exists.
pub async fn setup(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<(StatusCode, Json<UserView>), ApiError> {
    if users::count(&state.pool).await.map_err(internal)? > 0 {
        return Err(ApiError(Error::Conflict("setup already completed".into())));
    }
    validate_password(&body.password)?;
    let hash = password::hash(&body.password).map_err(|e| internal(anyhow_from(e)))?;
    let roles = vec!["admin".to_string()];
    let user = users::insert(&state.pool, &body.email, &hash, &roles)
        .await
        .map_err(map_insert_err)?;
    Ok((
        StatusCode::CREATED,
        Json(UserView {
            id: user.id,
            email: user.email,
            roles: user.roles,
        }),
    ))
}

/// POST /auth/login — verify creds, return a signed JWT.
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<Json<Value>, ApiError> {
    let found = users::find_by_email(&state.pool, &body.email)
        .await
        .map_err(internal)?;

    // Always run a verify to keep timing roughly constant whether or not the
    // user exists (mitigates user enumeration).
    let ok = match &found {
        Some(u) => password::verify(&body.password, &u.password_hash),
        None => {
            // Dummy verify against a fixed hash; result discarded.
            let _ = password::verify(&body.password, DUMMY_HASH);
            false
        }
    };

    let user = match (ok, found) {
        (true, Some(u)) => u,
        _ => return Err(ApiError(Error::Unauthorized)),
    };

    let ttl = state.config.jwt_ttl_secs;
    let token = jwt::sign(
        state.config.jwt_secret.as_bytes(),
        user.id,
        &user.email,
        &user.roles,
        ttl,
    )
    .map_err(|e| internal(anyhow_from(e)))?;

    let expires_at = chrono::Utc::now().timestamp() + ttl;
    Ok(Json(json!({ "token": token, "expires_at": expires_at })))
}

/// GET /auth/me — echo the current principal.
pub async fn me(Extension(principal): Extension<Principal>) -> Json<UserView> {
    let Principal::User { id, email, roles } = principal;
    Json(UserView { id, email, roles })
}

/// A precomputed Argon2id hash of a throwaway password, used for constant-ish
/// timing on the missing-user login path. (Hash of "dummy-password-x".)
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$RdescudvJCsgt3ub+b+dWRWJTmaaJObG";

fn internal(e: impl Into<anyhow::Error>) -> ApiError {
    ApiError(Error::Internal(e.into()))
}

fn anyhow_from<E: std::fmt::Display>(e: E) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

/// Map a unique-violation (duplicate email) to 409, else internal.
fn map_insert_err(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(db) = &e {
        if db.code().as_deref() == Some("23505") {
            return ApiError(Error::Conflict("email already exists".into()));
        }
    }
    ApiError(Error::Internal(e.into()))
}
```

> The `DUMMY_HASH` constant must be a valid Argon2id PHC string or `password::verify` returns early. Step 3 regenerates a guaranteed-valid one.

- [ ] **Step 3: Regenerate a valid DUMMY_HASH**

The literal above is illustrative and may not parse. Generate a real one:

Run:
```bash
cat > /tmp/genhash.rs <<'EOF'
fn main() {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;
    let salt = SaltString::generate(&mut OsRng);
    let h = Argon2::default().hash_password(b"dummy-password-x", &salt).unwrap();
    println!("{h}");
}
EOF
echo "Use the project to print one instead:"
```

Simpler: add a temporary test in `handlers.rs` that prints a hash, or compute inline. Replace `DUMMY_HASH` by running this one-off unit test added temporarily to `password.rs`:

```rust
    #[test]
    fn print_dummy_hash() {
        println!("DUMMY={}", super::hash("dummy-password-x").unwrap());
    }
```

Run: `cargo test -p ferrum-http print_dummy_hash -- --nocapture`
Copy the printed PHC string into `DUMMY_HASH` in `handlers.rs`, then delete the temporary `print_dummy_hash` test.
Expected: a string starting `$argon2id$v=19$...`.

- [ ] **Step 4: Wire the auth router**

Replace `crates/http/src/auth/mod.rs` with:

```rust
//! Authentication: password hashing, JWT, user store, and /auth routes.

pub mod handlers;
pub mod jwt;
pub mod password;
pub mod users;

use crate::state::AppState;
use axum::routing::{get, post};
use axum::Router;

/// Unauthenticated auth routes (setup, login).
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/setup", post(handlers::setup))
        .route("/auth/login", post(handlers::login))
}

/// Authenticated auth routes (me).
pub fn protected_router() -> Router<AppState> {
    Router::new().route("/auth/me", get(handlers::me))
}
```

- [ ] **Step 5: Mount in build_router + swap middleware**

Replace `crates/http/src/routes/mod.rs` lines 1–23 (the imports + `build_router`) with:

```rust
use crate::auth;
use crate::middleware::auth::require_auth;
use crate::state::AppState;
use axum::routing::get;
use axum::Router;
use std::path::Path;

pub mod content;
pub mod health;
pub mod schema;

pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/healthz", get(health::healthz))
        .merge(auth::public_router());

    let protected = Router::new()
        .merge(schema::router())
        .merge(content::router())
        .merge(auth::protected_router())
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    public.merge(protected).with_state(state)
}
```

- [ ] **Step 6: Export the auth router pieces (optional) + build**

No new lib export needed (auth is `pub mod`). Build the crate:

Run: `cargo build -p ferrum-http`
Expected: compiles. If `state.rs` tests from Task 8 weren't committed, they compile now too.

- [ ] **Step 7: Run all http unit tests**

Run: `cargo test -p ferrum-http`
Expected: all PASS (password, jwt, role_authz, error mappings).

- [ ] **Step 8: Commit (folds in Task 8 if deferred)**

```bash
git add crates/http/src/middleware/auth.rs crates/http/src/auth/ crates/http/src/routes/mod.rs crates/http/src/state.rs crates/http/src/lib.rs
git commit -m "feat(http): require_auth middleware + /auth setup/login/me routes"
```

---

## Task 10: Fix authz-deny → Forbidden in content + schema routes

**Files:**
- Modify: `crates/http/src/routes/content.rs:32-37`
- Modify: `crates/http/src/routes/schema.rs` (if it has its own `ensure`)

- [ ] **Step 1: Check schema routes for an ensure() helper**

Run: `grep -n "Action::\|authz.can\|Unauthorized" crates/http/src/routes/schema.rs`
Expected: shows whether schema.rs does its own authz check. Apply the same fix there if so.

- [ ] **Step 2: Change content.rs `ensure`**

In `crates/http/src/routes/content.rs`, change the `ensure` body:

```rust
async fn ensure(state: &AppState, principal: &Principal, action: Action, ct: &str) -> Result<(), ApiError> {
    if !state.authz.can(principal, action, ct).await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}
```

(Only the `Error::Unauthorized` → `Error::Forbidden` change.)

- [ ] **Step 3: Apply same change in schema.rs if it has an authz check**

If Step 1 found `Error::Unauthorized` returned on `!authz.can(...)` in `schema.rs`, change it to `Error::Forbidden` identically.

- [ ] **Step 4: Build**

Run: `cargo build -p ferrum-http`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/routes/content.rs crates/http/src/routes/schema.rs
git commit -m "fix(http): authz denial returns 403 Forbidden not 401"
```

---

## Task 11: Migrate test helper to setup→login bearer token

**Files:**
- Modify: `crates/bin/tests/common/mod.rs`

- [ ] **Step 1: Rewrite the helper**

Replace `crates/bin/tests/common/mod.rs` with:

```rust
//! Shared integration-test plumbing. Spins a real Postgres via testcontainers
//! and the ferrum router in-process, hitting it via reqwest.

use ferrum_http::{build_router, AppConfig, AppState, NoopSink, RoleAuthz};
use ferrum_schema::{SchemaRegistry, SchemaService, MIGRATOR};
use sqlx::PgPool;
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres as PgImage;

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
    pub schemas: SchemaService,
    /// Bearer token for the seeded admin user (set by `spawn`).
    pub token: String,
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
            schemas: schemas.clone(),
            authz: Arc::new(RoleAuthz),
            events: Arc::new(NoopSink),
            config: AppConfig {
                jwt_secret: JWT_SECRET.into(),
                jwt_ttl_secs: 3600,
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

        let base_url = format!("http://{addr}");
        let client = reqwest::Client::new();

        // First-run setup → creates admin user.
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
        let token = login["token"].as_str().expect("token in login response").to_string();

        Self {
            base_url,
            pool,
            client,
            schemas,
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
```

- [ ] **Step 2: Run the existing integration suite**

Run: `cargo test -p ferrum --test integration_smoke`
Expected: PASS — proves the setup→login→bearer flow works end-to-end and existing `.admin()` call sites still authenticate.

- [ ] **Step 3: Run the full integration suite**

Run: `cargo test -p ferrum`
Expected: all existing integration tests PASS with the new auth. (Any test that asserted `x-api-key`-specific behavior would need updating, but call sites use `app.admin(...)`, which is now bearer-based.)

- [ ] **Step 4: Commit**

```bash
git add crates/bin/tests/common/mod.rs
git commit -m "test(bin): migrate test helper to setup+login bearer auth"
```

---

## Task 12: Bin config + main.rs

**Files:**
- Modify: `crates/bin/src/config.rs`
- Modify: `crates/bin/src/main.rs`

- [ ] **Step 1: Update config test first**

In `crates/bin/src/config.rs`, replace the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_jwt_secret() {
        std::env::set_var("DATABASE_URL", "postgres://x");
        std::env::set_var("FERRUM_JWT_SECRET", "short");
        let err = Config::from_env().unwrap_err();
        assert!(err.to_string().contains("at least 32"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ferrum --lib rejects_short_jwt_secret`
Expected: FAIL — `FERRUM_JWT_SECRET` not read yet.

- [ ] **Step 3: Update the Config struct + loader**

In `crates/bin/src/config.rs`:

(a) In `struct Config`, replace `pub admin_key: String,` with:

```rust
    pub jwt_secret: String,
    pub jwt_ttl_secs: i64,
```

(b) In `from_env`, replace the `admin_key` block (lines reading `FERRUM_ADMIN_KEY`) with:

```rust
        let jwt_secret = std::env::var("FERRUM_JWT_SECRET")
            .context("FERRUM_JWT_SECRET must be set")?;
        if jwt_secret.len() < 32 {
            return Err(anyhow!("FERRUM_JWT_SECRET must be at least 32 characters"));
        }
        let jwt_ttl_secs = std::env::var("FERRUM_JWT_TTL_SECS")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(86400);
```

(c) In the returned `Ok(Self { .. })`, replace `admin_key,` with:

```rust
            jwt_secret,
            jwt_ttl_secs,
```

- [ ] **Step 4: Update main.rs**

In `crates/bin/src/main.rs`:

(a) Change the import line:

```rust
use ferrum_http::{build_router, mount_studio, AppConfig, AppState, NoopSink, RoleAuthz};
```

(b) In the `AppState { .. }` construction, change `authz`:

```rust
        authz: Arc::new(RoleAuthz),
```

(c) Replace the `config: AppConfig { .. }` block:

```rust
        config: AppConfig {
            jwt_secret: cfg.jwt_secret.clone(),
            jwt_ttl_secs: cfg.jwt_ttl_secs,
            page_size_max: cfg.page_size_max,
        },
```

- [ ] **Step 5: Run config test + build the binary**

Run: `cargo test -p ferrum --lib rejects_short_jwt_secret`
Expected: PASS.

Run: `cargo build -p ferrum`
Expected: compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/bin/src/config.rs crates/bin/src/main.rs
git commit -m "feat(bin): FERRUM_JWT_SECRET config, RoleAuthz wiring; drop admin key"
```

---

## Task 13: Integration tests for auth endpoints

**Files:**
- Create: `crates/bin/tests/integration_auth.rs`

- [ ] **Step 1: Write the tests**

Create `crates/bin/tests/integration_auth.rs`:

```rust
mod common;
use common::{TestApp, TEST_EMAIL, TEST_PASSWORD};

#[tokio::test]
async fn setup_is_self_closing() {
    let app = TestApp::spawn().await;
    // spawn() already ran setup once; a second setup must 409.
    let resp = app
        .client
        .post(app.url("/auth/setup"))
        .json(&serde_json::json!({ "email": "second@example.test", "password": "another-pw-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "conflict");
}

#[tokio::test]
async fn login_good_credentials() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": TEST_EMAIL, "password": TEST_PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["token"].as_str().is_some());
    assert!(body["expires_at"].as_i64().is_some());
}

#[tokio::test]
async fn login_wrong_password_401() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": TEST_EMAIL, "password": "totally-wrong" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn login_unknown_email_401() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": "nobody@example.test", "password": "whatever-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn me_requires_token() {
    let app = TestApp::spawn().await;
    let resp = app.client.get(app.url("/auth/me")).send().await.unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn me_returns_principal_with_token() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.get(app.url("/auth/me")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["email"], TEST_EMAIL);
    assert_eq!(body["roles"][0], "admin");
}

#[tokio::test]
async fn protected_route_rejects_missing_token() {
    let app = TestApp::spawn().await;
    // /api/<type> is behind require_auth; no token → 401.
    let resp = app.client.get(app.url("/api/article")).send().await.unwrap();
    assert_eq!(resp.status(), 401);
}
```

- [ ] **Step 2: Run the auth integration tests**

Run: `cargo test -p ferrum --test integration_auth`
Expected: 7 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/bin/tests/integration_auth.rs
git commit -m "test(bin): integration tests for setup/login/me"
```

---

## Task 14: Docs + docker-compose

**Files:**
- Modify: `docker-compose.yml`
- Modify: `README.md`

- [ ] **Step 1: Inspect current docker-compose env**

Run: `grep -n "FERRUM_ADMIN_KEY\|FERRUM_" docker-compose.yml`
Expected: shows the `FERRUM_ADMIN_KEY` line(s) to replace.

- [ ] **Step 2: Swap the env var in docker-compose.yml**

Replace each `FERRUM_ADMIN_KEY` reference with `FERRUM_JWT_SECRET` (same value source / default). For the demo default, generate at runtime as documented; keep any `${...}` substitution pattern already used.

- [ ] **Step 3: Update README**

In `README.md`:
- Replace the `FERRUM_ADMIN_KEY` override snippet with `FERRUM_JWT_SECRET`:
  ```sh
  export FERRUM_JWT_SECRET=$(openssl rand -hex 32)
  ```
- Replace the backend run snippet's `FERRUM_ADMIN_KEY` line with `FERRUM_JWT_SECRET`.
- Add a short "First-run setup" note after the Docker section:
  ```md
  On first boot the users table is empty. Create the initial admin:

  ```sh
  curl -X POST http://localhost:8080/auth/setup \
    -H 'content-type: application/json' \
    -d '{"email":"admin@example.com","password":"change-me-please"}'
  ```

  Then log in to get a JWT:

  ```sh
  curl -X POST http://localhost:8080/auth/login \
    -H 'content-type: application/json' \
    -d '{"email":"admin@example.com","password":"change-me-please"}'
  ```

  Send `Authorization: Bearer <token>` on subsequent API requests.
  The setup endpoint returns 409 once an admin exists.
  ```

- [ ] **Step 4: Verify full workspace builds + tests**

Run: `cargo build --workspace && cargo test --workspace`
Expected: all green. (Integration tests spin testcontainers Postgres.)

- [ ] **Step 5: Commit**

```bash
git add docker-compose.yml README.md
git commit -m "docs: first-run setup flow, swap admin key for JWT secret"
```

---

## Self-Review Notes

**Spec coverage check:**
- Email/password + HS256 JWT access-only → Tasks 6, 9. ✓
- Argon2id → Task 5. ✓
- Multi-role RBAC, hardcoded map, `role_allows` → Tasks 2, 8. ✓
- Default `admin` role → Task 9 (setup inserts `{admin}`). ✓
- Remove `FERRUM_ADMIN_KEY`, first-run setup → Tasks 9, 12. ✓
- `/auth/setup`, `/auth/login`, `/auth/me` → Task 9. ✓
- `_users` table → Task 4. ✓
- 401 vs 403 (Forbidden) → Tasks 3, 10. ✓
- No-enumeration login timing → Task 9 (dummy verify). ✓
- Test helper migration → Task 11. ✓
- Docs/docker → Task 14. ✓

**Notes / known footguns surfaced for the implementer:**
- `Error::Conflict(String)` already exists and maps to 409 — only `Forbidden` is new.
- `DUMMY_HASH` must be a valid PHC string; Task 9 Step 3 generates a real one.
- Task 8 + Task 9 may need to be compiled/committed together because removing `admin_key` from `AppConfig` breaks `middleware/auth.rs` until `require_auth` lands.
- `sqlx::migrate!` embeds migrations at compile time — adding the `.sql` file is sufficient; no registration code.
