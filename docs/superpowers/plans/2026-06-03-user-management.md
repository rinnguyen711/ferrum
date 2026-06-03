# User Management (CRUD + Users Screen) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Admin-only user CRUD endpoints plus a studio Users screen (list + editor) that manages email, roles, and passwords, with backend-unsupported features shown as disabled placeholders.

**Architecture:** New `/admin/users` routes (axum) mirror the existing `/admin/content-types` pattern, behind `require_auth` + a new `UserRead`/`UserWrite` authz check. A `RoleAuthz` user can only reach them as `admin`. The UI adds `/users` list + editor screens ported from `design/rustapi/users.jsx`, wired through new `endpoints.ts` calls; lockout guards (no self-delete, no self-demote) live in the handlers.

**Tech Stack:** Rust/Axum 0.7 + sqlx (backend), Argon2id (password rehash), React 18 + TS + react-router-dom (UI, typecheck only), cargo test + playwright smoke.

**Spec:** `docs/superpowers/specs/2026-06-03-user-management-design.md`

**Verification commands:**
- Backend unit: `cargo test -p rustapi-core` / `cargo test -p rustapi-http`
- Backend integration: `cargo test -p rustapi --test integration_users` (needs Docker)
- UI: `cd ui && pnpm typecheck`

---

## File Structure

- `crates/core/src/principal.rs` — **modify**: add `Action::UserRead`/`UserWrite`; extend `role_allows`.
- `crates/http/src/auth/users.rs` — **modify**: add `list`, `create`, `update`, `delete` store fns + a `UserView` serializer.
- `crates/http/src/routes/users.rs` — **create**: `/admin/users` handlers + router.
- `crates/http/src/routes/mod.rs` — **modify**: merge `users::router()` into protected routes.
- `crates/bin/tests/integration_users.rs` — **create**: CRUD + guard + authz tests.
- `ui/src/api/types.ts` — **modify**: `User`, `NewUser`, `PatchUser`.
- `ui/src/api/endpoints.ts` — **modify**: `listUsers`, `createUser`, `updateUser`, `deleteUser`.
- `ui/src/roles.ts` — **create**: static `ROLES` + helpers mirroring backend.
- `ui/src/screens/Users.tsx` — **create**: list screen.
- `ui/src/screens/UserEditor.tsx` — **create**: create/edit screen.
- `ui/src/App.tsx` — **modify**: `/users`, `/users/new`, `/users/:id` routes.
- `ui/src/components/shell.tsx` — **modify**: Users nav entry in `rs-rail-foot` (admin-only).
- `ui/src/styles.css` — **modify**: port role/user CSS from `design/rustapi/styles.css`.

---

## Task 1: Core — `UserRead` / `UserWrite` actions

**Files:**
- Modify: `crates/core/src/principal.rs`

- [ ] **Step 1: Write failing tests**

In `crates/core/src/principal.rs`, inside the existing `#[cfg(test)] mod tests`, add:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustapi-core principal`
Expected: FAIL — `Action::UserRead`/`UserWrite` do not exist.

- [ ] **Step 3: Add the variants**

In `enum Action`, after `ContentWrite,` add:

```rust
    UserRead,
    UserWrite,
```

`role_allows` already returns `true` for `admin` on every action (the `"admin" => true` arm), and the wildcard `_ => false` covers editor/viewer for the new actions, so no change to the match body is needed. (Verify the `admin => true` arm is present; if `admin` enumerates actions explicitly instead, add the two new ones.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustapi-core principal`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/principal.rs
git commit -m "feat(core): UserRead/UserWrite actions"
```

---

## Task 2: Store — list / create / update / delete

**Files:**
- Modify: `crates/http/src/auth/users.rs`

- [ ] **Step 1: Add the store functions**

In `crates/http/src/auth/users.rs`, after `insert_first_admin` (before `find_by_email` is fine too — keep related fns together), add:

```rust
/// All users, newest first. Excludes password hashes from the caller's concern
/// (the hash is on UserRow but handlers serialize via UserView).
pub async fn list(pool: &PgPool) -> Result<Vec<UserRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (Uuid, String, String, Vec<String>)>(
        "SELECT id, email, password_hash, roles FROM _users ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, email, password_hash, roles)| UserRow { id, email, password_hash, roles })
        .collect())
}

/// Insert a user (admin-created). Distinct from `insert_first_admin`, which is
/// guarded for the empty-table setup flow.
pub async fn create(
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
    Ok(UserRow { id, email, password_hash, roles })
}

/// Update selected fields. `None` arguments are left unchanged. Returns the
/// updated row, or `None` if no user has that id. `updated_at` bumped.
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    email: Option<&str>,
    password_hash: Option<&str>,
    roles: Option<&[String]>,
) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, (Uuid, String, String, Vec<String>)>(
        "UPDATE _users SET \
           email = COALESCE($2, email), \
           password_hash = COALESCE($3, password_hash), \
           roles = COALESCE($4, roles), \
           updated_at = now() \
         WHERE id = $1 \
         RETURNING id, email, password_hash, roles",
    )
    .bind(id)
    .bind(email)
    .bind(password_hash)
    .bind(roles)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, email, password_hash, roles)| UserRow { id, email, password_hash, roles }))
}

/// Delete by id. Returns true if a row was removed.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM _users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p rustapi-http`
Expected: compiles. (`list`/`create`/`update`/`delete` are unused until Task 3 — that's a warning, not an error; Task 3 lands immediately after.)

- [ ] **Step 3: Commit**

```bash
git add crates/http/src/auth/users.rs
git commit -m "feat(http): user store list/create/update/delete"
```

---

## Task 3: Handlers + router — `/admin/users`

**Files:**
- Create: `crates/http/src/routes/users.rs`
- Modify: `crates/http/src/routes/mod.rs`

- [ ] **Step 1: Create the handlers + router**

Create `crates/http/src/routes/users.rs`:

```rust
//! /admin/users/* handlers (admin-only user management).

use crate::auth::{password, users};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use rustapi_core::{Action, Error, Principal, ValidationErrors};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list).post(create))
        .route("/admin/users/:id", axum::routing::patch(update).delete(remove))
}

#[derive(Serialize)]
struct UserView {
    id: Uuid,
    email: String,
    roles: Vec<String>,
}

impl From<users::UserRow> for UserView {
    fn from(u: users::UserRow) -> Self {
        UserView { id: u.id, email: u.email, roles: u.roles }
    }
}

#[derive(Deserialize)]
struct CreateBody {
    email: String,
    password: String,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Deserialize)]
struct UpdateBody {
    email: Option<String>,
    password: Option<String>,
    roles: Option<Vec<String>>,
}

/// Authz gate. Denial → 403.
async fn ensure(state: &AppState, principal: &Principal, action: Action) -> Result<(), ApiError> {
    if !state.authz.can(principal, action, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

fn principal_id(p: &Principal) -> Uuid {
    match p {
        Principal::User { id, .. } => *id,
    }
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

/// Map a unique-violation (duplicate email) to 409.
fn map_db_err(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(db) = &e {
        if db.code().as_deref() == Some("23505") {
            return ApiError(Error::Conflict("email already exists".into()));
        }
    }
    ApiError(Error::Internal(e.into()))
}

async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<UserView>>, ApiError> {
    ensure(&state, &principal, Action::UserRead).await?;
    let rows = users::list(&state.pool)
        .await
        .map_err(|e| ApiError(Error::Internal(e.into())))?;
    Ok(Json(rows.into_iter().map(UserView::from).collect()))
}

async fn create(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<UserView>), ApiError> {
    ensure(&state, &principal, Action::UserWrite).await?;
    validate_password(&body.password)?;
    let hash = password::hash(&body.password)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!("{e}"))))?;
    let row = users::create(&state.pool, &body.email, &hash, &body.roles)
        .await
        .map_err(map_db_err)?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

async fn update(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<UserView>, ApiError> {
    ensure(&state, &principal, Action::UserWrite).await?;

    // Lockout guard: cannot remove your own admin role.
    if id == principal_id(&principal) {
        if let Some(new_roles) = &body.roles {
            if !new_roles.iter().any(|r| r == "admin") {
                return Err(ApiError(Error::Conflict(
                    "cannot remove your own admin role".into(),
                )));
            }
        }
    }

    let hash = match &body.password {
        Some(pw) if !pw.is_empty() => {
            validate_password(pw)?;
            Some(
                password::hash(pw)
                    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!("{e}"))))?,
            )
        }
        _ => None,
    };

    let row = users::update(
        &state.pool,
        id,
        body.email.as_deref(),
        hash.as_deref(),
        body.roles.as_deref(),
    )
    .await
    .map_err(map_db_err)?
    .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(row.into()))
}

async fn remove(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::UserWrite).await?;

    // Lockout guard: cannot delete your own account.
    if id == principal_id(&principal) {
        return Err(ApiError(Error::Conflict(
            "cannot delete your own account".into(),
        )));
    }

    let removed = users::delete(&state.pool, id)
        .await
        .map_err(|e| ApiError(Error::Internal(e.into())))?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(Error::NotFound))
    }
}
```

- [ ] **Step 2: Mount the router**

In `crates/http/src/routes/mod.rs`, add `pub mod users;` alongside the other route modules, and merge it into the `protected` router. Change:

```rust
    let protected = Router::new()
        .merge(schema::router())
        .merge(content::router())
        .merge(auth::protected_router())
```

to:

```rust
    let protected = Router::new()
        .merge(schema::router())
        .merge(content::router())
        .merge(users::router())
        .merge(auth::protected_router())
```

And add the module declaration near `pub mod content;`:

```rust
pub mod users;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p rustapi-http`
Expected: compiles, store-fn warnings gone.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/routes/users.rs crates/http/src/routes/mod.rs
git commit -m "feat(http): /admin/users CRUD with lockout guards"
```

---

## Task 4: Backend integration tests

**Files:**
- Create: `crates/bin/tests/integration_users.rs`

- [ ] **Step 1: Write the tests**

Create `crates/bin/tests/integration_users.rs`:

```rust
mod common;
use common::{TestApp, TEST_EMAIL, TEST_PASSWORD};

/// Create a second user via the admin API and return its id.
async fn create_user(app: &TestApp, email: &str, password: &str, roles: &[&str]) -> String {
    let resp = app
        .admin(app.client.post(app.url("/admin/users")))
        .json(&serde_json::json!({ "email": email, "password": password, "roles": roles }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "create_user should 201");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

/// Log in and return a bearer token.
async fn token_for(app: &TestApp, email: &str, password: &str) -> String {
    let body: serde_json::Value = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn list_includes_seeded_admin() {
    let app = TestApp::spawn().await;
    let resp = app.admin(app.client.get(app.url("/admin/users"))).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|u| u["email"] == TEST_EMAIL));
}

#[tokio::test]
async fn create_then_list_and_dup_conflict() {
    let app = TestApp::spawn().await;
    create_user(&app, "ed@example.test", "editor-pw-123", &["editor"]).await;

    // duplicate email → 409
    let dup = app
        .admin(app.client.post(app.url("/admin/users")))
        .json(&serde_json::json!({ "email": "ed@example.test", "password": "another-123", "roles": ["editor"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup.status(), 409);
}

#[tokio::test]
async fn create_short_password_422() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/users")))
        .json(&serde_json::json!({ "email": "x@example.test", "password": "short", "roles": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn update_password_then_login_works() {
    let app = TestApp::spawn().await;
    let id = create_user(&app, "rot@example.test", "first-pw-123", &["viewer"]).await;

    let patch = app
        .admin(app.client.patch(app.url(&format!("/admin/users/{id}"))))
        .json(&serde_json::json!({ "password": "second-pw-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), 200);

    // old password fails, new password works
    let old = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": "rot@example.test", "password": "first-pw-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(old.status(), 401);

    let new = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": "rot@example.test", "password": "second-pw-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(new.status(), 200);
}

#[tokio::test]
async fn update_roles_reflected() {
    let app = TestApp::spawn().await;
    let id = create_user(&app, "promote@example.test", "promote-123", &["viewer"]).await;
    let resp = app
        .admin(app.client.patch(app.url(&format!("/admin/users/{id}"))))
        .json(&serde_json::json!({ "roles": ["editor"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["roles"][0], "editor");
}

#[tokio::test]
async fn delete_user_then_404() {
    let app = TestApp::spawn().await;
    let id = create_user(&app, "gone@example.test", "gone-pw-123", &[]).await;
    let del = app.admin(app.client.delete(app.url(&format!("/admin/users/{id}")))).send().await.unwrap();
    assert_eq!(del.status(), 204);
    // second delete → 404
    let again = app.admin(app.client.delete(app.url(&format!("/admin/users/{id}")))).send().await.unwrap();
    assert_eq!(again.status(), 404);
}

#[tokio::test]
async fn self_delete_blocked_409() {
    let app = TestApp::spawn().await;
    // find the admin's own id
    let list: serde_json::Value = app
        .admin(app.client.get(app.url("/admin/users")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let me = list.as_array().unwrap().iter().find(|u| u["email"] == TEST_EMAIL).unwrap();
    let my_id = me["id"].as_str().unwrap();
    let resp = app.admin(app.client.delete(app.url(&format!("/admin/users/{my_id}")))).send().await.unwrap();
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn self_demote_blocked_409() {
    let app = TestApp::spawn().await;
    let list: serde_json::Value = app
        .admin(app.client.get(app.url("/admin/users")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let me = list.as_array().unwrap().iter().find(|u| u["email"] == TEST_EMAIL).unwrap();
    let my_id = me["id"].as_str().unwrap();
    let resp = app
        .admin(app.client.patch(app.url(&format!("/admin/users/{my_id}"))))
        .json(&serde_json::json!({ "roles": ["viewer"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn non_admin_forbidden() {
    let app = TestApp::spawn().await;
    create_user(&app, "editor2@example.test", "editor2-pw-123", &["editor"]).await;
    let token = token_for(&app, "editor2@example.test", "editor2-pw-123").await;

    let resp = app
        .client
        .get(app.url("/admin/users"))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p rustapi --test integration_users`
Expected: all PASS (needs Docker).

- [ ] **Step 3: Commit**

```bash
git add crates/bin/tests/integration_users.rs
git commit -m "test(bin): /admin/users CRUD + guard + authz integration tests"
```

---

## Task 5: UI — types + endpoints + roles

**Files:**
- Modify: `ui/src/api/types.ts`
- Modify: `ui/src/api/endpoints.ts`
- Create: `ui/src/roles.ts`

- [ ] **Step 1: Add types**

In `ui/src/api/types.ts`, append:

```ts
export interface User {
  id: string;
  email: string;
  roles: string[];
}

export interface NewUser {
  email: string;
  password: string;
  roles: string[];
}

export interface PatchUser {
  email?: string;
  password?: string;
  roles?: string[];
}
```

- [ ] **Step 2: Add endpoints**

In `ui/src/api/endpoints.ts`, add the `User`-related imports to the existing `import type { ... } from "./types";` line (add `User`, `NewUser`, `PatchUser`), then append:

```ts
export function listUsers(): Promise<User[]> {
  return apiFetch<User[]>("/admin/users");
}

export function createUser(body: NewUser): Promise<User> {
  return apiFetch<User>("/admin/users", { method: "POST", body });
}

export function updateUser(id: string, body: PatchUser): Promise<User> {
  return apiFetch<User>(`/admin/users/${encodeURIComponent(id)}`, { method: "PATCH", body });
}

export function deleteUser(id: string): Promise<void> {
  return apiFetch<void>(`/admin/users/${encodeURIComponent(id)}`, { method: "DELETE" });
}
```

- [ ] **Step 3: Create the roles module**

Create `ui/src/roles.ts`:

```ts
/** Mirrors the backend role→permission map (rustapi_core::role_allows). Display
 * only; the server is authoritative. */
export interface Role {
  key: string;
  name: string;
  color: string;
  desc: string;
}

export const ROLES: Role[] = [
  { key: "admin", name: "Admin", color: "#D14D2B", desc: "Full access to content, schema, and users." },
  { key: "editor", name: "Editor", color: "#2B6CD1", desc: "Read and write content entries." },
  { key: "viewer", name: "Viewer", color: "#52525B", desc: "Read-only access to content." },
];

export function roleOf(key: string): Role {
  return ROLES.find((r) => r.key === key) ?? { key, name: key, color: "#52525B", desc: "Unknown role." };
}

/** Capability matrix per role, derived from role_allows for display. Order
 * matches CAPS below. */
export const CAPS = ["Read content", "Write content", "Read schema", "Write schema", "Manage users"];

export function capsFor(key: string): boolean[] {
  switch (key) {
    case "admin":
      return [true, true, true, true, true];
    case "editor":
      return [true, true, false, false, false];
    case "viewer":
      return [true, false, false, false, false];
    default:
      return [false, false, false, false, false];
  }
}
```

- [ ] **Step 4: Verify typecheck (passes — no consumers broken yet)**

Run: `cd ui && pnpm typecheck`
Expected: PASS (new exports, no removed symbols).

- [ ] **Step 5: Commit**

```bash
git add ui/src/api/types.ts ui/src/api/endpoints.ts ui/src/roles.ts
git commit -m "feat(ui): user API client + roles module"
```

---

## Task 6: UI — Users list screen

**Files:**
- Create: `ui/src/screens/Users.tsx`

- [ ] **Step 1: Create the list screen**

Create `ui/src/screens/Users.tsx`:

```tsx
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import { Avatar } from "../components/shell";
import { useResource } from "../hooks/useResource";
import { listUsers } from "../api/endpoints";
import { ROLES, roleOf } from "../roles";
import { shortId } from "../util";

function Checkbox({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <span className={"rs-check" + (checked ? " is-on" : "")} onClick={onChange} role="checkbox" aria-checked={checked}>
      {checked && <Icons.check size={12} />}
    </span>
  );
}

export function Users() {
  const navigate = useNavigate();
  const users = useResource(() => listUsers(), []);
  const [query, setQuery] = useState("");
  const [roleFilter, setRoleFilter] = useState("all");

  const rows = (users.data ?? []).filter((u) => {
    if (roleFilter !== "all" && !u.roles.includes(roleFilter)) return false;
    if (query && !u.email.toLowerCase().includes(query.toLowerCase())) return false;
    return true;
  });

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Users</h1>
          <p className="rs-cm-sub">{(users.data ?? []).length} members</p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => navigate("/users/new")}>
          <Icons.plus size={16} /> Add user
        </button>
      </div>

      <div className="rs-rolebar">
        {ROLES.map((r) => (
          <button
            key={r.key}
            className={"rs-rolebar-item" + (roleFilter === r.key ? " is-active" : "")}
            style={{ ["--chip" as string]: r.color }}
            onClick={() => setRoleFilter(roleFilter === r.key ? "all" : r.key)}
            title={r.desc}
          >
            <span className="rs-rolebar-dot" />
            <strong>{r.name}</strong>
            <span>{(users.data ?? []).filter((u) => u.roles.includes(r.key)).length}</span>
          </button>
        ))}
      </div>

      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input placeholder="Search email" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <button className="rs-btn rs-btn--ghost" data-placeholder title="Coming soon" disabled>
          <Icons.external size={15} /> Export
        </button>
      </div>

      {users.loading && <div className="rs-empty">Loading…</div>}
      {users.error && <div className="rs-empty">Failed to load users.</div>}

      {!users.loading && !users.error && (
        <div className="rs-table-wrap">
          <table className="rs-table">
            <thead>
              <tr>
                <th className="rs-col-id">ID</th>
                <th>User</th>
                <th>Roles</th>
                <th className="rs-col-2fa" data-placeholder title="Coming soon">2FA</th>
                <th className="rs-col-act"></th>
              </tr>
            </thead>
            <tbody>
              {rows.map((u) => (
                <tr key={u.id} onClick={() => navigate(`/users/${u.id}`)}>
                  <td className="rs-col-id rs-mono">{shortId(u.id)}</td>
                  <td>
                    <span className="rs-user-cell">
                      <Avatar name={u.email} initials={u.email.slice(0, 2).toUpperCase()} color="#52525B" size={34} />
                      <span className="rs-user-id">
                        <strong>{u.email}</strong>
                      </span>
                    </span>
                  </td>
                  <td>
                    {u.roles.length === 0 && <span className="rs-cell-muted">—</span>}
                    {u.roles.map((rk) => {
                      const r = roleOf(rk);
                      return (
                        <span key={rk} className="rs-role-pill" style={{ ["--chip" as string]: r.color }}>
                          {r.name}
                        </span>
                      );
                    })}
                  </td>
                  <td className="rs-col-2fa" data-placeholder>
                    <span className="rs-2fa is-off"><span className="rs-2fa-dash" /> Off</span>
                  </td>
                  <td className="rs-col-act" onClick={(e) => e.stopPropagation()}>
                    <button className="rs-row-btn" onClick={() => navigate(`/users/${u.id}`)}>
                      <Icons.edit size={16} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {rows.length === 0 && <div className="rs-empty">No users match.</div>}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Typecheck (will fail until App route added in Task 8)**

Run: `cd ui && pnpm typecheck`
Expected: PASS for `Users.tsx` itself (it imports only existing symbols). If the only error is "Users is declared but never used" it appears once routed in Task 8 — no error expected here since the file exports a used-on-import component. Proceed.

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/Users.tsx
git commit -m "feat(ui): Users list screen"
```

---

## Task 7: UI — User editor screen

**Files:**
- Create: `ui/src/screens/UserEditor.tsx`

- [ ] **Step 1: Create the editor**

Create `ui/src/screens/UserEditor.tsx`:

```tsx
import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Icons } from "../components/icons";
import { useResource } from "../hooks/useResource";
import { listUsers, createUser, updateUser, deleteUser } from "../api/endpoints";
import { ApiError } from "../api/client";
import { ROLES, CAPS, capsFor } from "../roles";

export function UserEditor() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const isNew = !id || id === "new";

  // For edit, load the user from the list (no single-get endpoint this slice).
  const users = useResource(() => listUsers(), []);
  const existing = isNew ? null : (users.data ?? []).find((u) => u.id === id) ?? null;

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [roles, setRoles] = useState<string[]>(["editor"]);
  const [tab, setTab] = useState<"account" | "permissions">("account");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [hydrated, setHydrated] = useState(false);

  // Hydrate form once the user loads (edit mode).
  if (!isNew && existing && !hydrated) {
    setEmail(existing.email);
    setRoles(existing.roles.length ? existing.roles : ["viewer"]);
    setHydrated(true);
  }

  const toggleRole = (key: string) => {
    setRoles((rs) => (rs.includes(key) ? rs.filter((r) => r !== key) : [...rs, key]));
  };

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      if (isNew) {
        await createUser({ email, password, roles });
      } else {
        const patch: { email?: string; password?: string; roles?: string[] } = { email, roles };
        if (password) patch.password = password;
        await updateUser(id!, patch);
      }
      navigate("/users");
    } catch (e) {
      if (e instanceof ApiError) {
        if (e.fieldErrors.length) setError(e.fieldErrors[0].message ?? e.message);
        else setError(e.message);
      } else setError("Something went wrong.");
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (isNew || !id) return;
    if (!window.confirm("Delete this user? This cannot be undone.")) return;
    setBusy(true);
    setError(null);
    try {
      await deleteUser(id);
      navigate("/users");
    } catch (e) {
      if (e instanceof ApiError) setError(e.message);
      else setError("Something went wrong.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rs-editor">
      <div className="rs-editor-bar">
        <button className="rs-back" onClick={() => navigate("/users")}>
          <Icons.arrowLeft size={18} />
        </button>
        <div className="rs-editor-titlewrap">
          <h1>{isNew ? "Add a user" : email || "User"}</h1>
        </div>
        <div className="rs-editor-actions">
          {!isNew && (
            <button className="rs-btn rs-btn--ghost rs-danger" disabled={busy} onClick={remove}>
              <Icons.trash size={15} /> Delete
            </button>
          )}
          <button className="rs-btn rs-btn--primary" disabled={busy || !email || (isNew && !password)} onClick={save}>
            <Icons.check size={15} /> {isNew ? "Create user" : "Save user"}
          </button>
        </div>
      </div>

      {error && <div className="rs-login-error" style={{ margin: "0 24px" }}>{error}</div>}

      <div className="rs-editor-body">
        <div className="rs-editor-main">
          <div className="rs-editor-tabs">
            {([["account", "Account"], ["permissions", "Role & permissions"]] as const).map(([k, l]) => (
              <button key={k} className={"rs-etab" + (tab === k ? " is-active" : "")} onClick={() => setTab(k)}>
                {l}
              </button>
            ))}
          </div>

          {tab === "account" && (
            <div className="rs-fields">
              <label className="rs-field">
                <span className="rs-field-label">Email</span>
                <input className="rs-input rs-mono" type="email" value={email} placeholder="name@company.com"
                  onChange={(e) => setEmail(e.target.value)} />
              </label>
              <label className="rs-field">
                <span className="rs-field-label">{isNew ? "Password" : "Reset password"}</span>
                <input className="rs-input" type="password" value={password}
                  placeholder={isNew ? "At least 8 characters" : "Leave blank to keep current"}
                  onChange={(e) => setPassword(e.target.value)} />
              </label>
            </div>
          )}

          {tab === "permissions" && (
            <div className="rs-fields">
              <div className="rs-field">
                <span className="rs-field-label">Roles</span>
                <div className="rs-perm-grid">
                  {ROLES.map((r) => (
                    <button key={r.key} className={"rs-role-radio" + (roles.includes(r.key) ? " is-on" : "")}
                      onClick={() => toggleRole(r.key)} type="button">
                      <span className="rs-radio-dot" />
                      <span className="rs-role-radio-text">
                        <strong><span className="rs-rolebar-dot" style={{ ["--chip" as string]: r.color }} />{r.name}</strong>
                        <span>{r.desc}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </div>
              <div className="rs-field">
                <span className="rs-field-label">Capabilities (read-only)</span>
                <div className="rs-cap">
                  {CAPS.map((c, i) => {
                    // union of selected roles' capabilities
                    const on = roles.some((rk) => capsFor(rk)[i]);
                    return (
                      <div className="rs-cap-row" key={c}>
                        <span>{c}</span>
                        <span className={"rs-cap-mark " + (on ? "is-on" : "is-off")}>
                          {on ? <Icons.check size={13} /> : <Icons.x size={12} />}
                        </span>
                      </div>
                    );
                  })}
                </div>
              </div>
              <div className="rs-field" data-placeholder title="Coming soon">
                <span className="rs-field-label">Two-factor authentication</span>
                <span className="rs-cell-muted">Coming soon</span>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS for the file (imports resolve). Proceed (routing wired in Task 8).

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/UserEditor.tsx
git commit -m "feat(ui): User editor (create/edit/delete)"
```

---

## Task 8: UI — routes + nav entry

**Files:**
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/components/shell.tsx`

- [ ] **Step 1: Add the routes**

In `ui/src/App.tsx`, add the screen imports near the other screen imports:

```ts
import { Users } from "./screens/Users";
import { UserEditor } from "./screens/UserEditor";
```

Inside the authed `<Route element={<RequireAuth>...}>` group (next to the `settings` route), add:

```tsx
          <Route path="users" element={<Users />} />
          <Route path="users/new" element={<UserEditor />} />
          <Route path="users/:id" element={<UserEditor />} />
```

- [ ] **Step 2: Add the admin-only nav entry**

In `ui/src/components/shell.tsx`, the sidebar footer (`rs-rail-foot`, ~line 103) holds the Settings button. Add a Users button beside it, shown only to admins. First add an import for the claims helper near the top of the file:

```ts
import { getClaims } from "../auth";
```

Then, inside the `Sidebar` component (where `items`/`isActive` are defined), compute admin status:

```ts
  const isAdmin = (getClaims()?.roles ?? []).includes("admin");
```

In the `rs-rail-foot` block, immediately before the Settings button, add:

```tsx
        {isAdmin && (
          <button
            data-tip="Users"
            className={"rs-rail-btn" + (location.pathname.startsWith("/users") ? " is-active" : "")}
            onClick={() => builder.guardedNavigate("/users")}
          >
            <Icons.user size={20} />
          </button>
        )}
```

> If `builder.guardedNavigate` is the navigation used by the Settings button, use
> it here too (matches the unsaved-changes guard). Confirm by reading the
> existing Settings button in the same block.

- [ ] **Step 3: Full typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/App.tsx ui/src/components/shell.tsx
git commit -m "feat(ui): /users routes + admin-only nav entry"
```

---

## Task 9: UI — port CSS

**Files:**
- Modify: `ui/src/styles.css`

- [ ] **Step 1: Identify which classes are missing**

Run:
```bash
for cls in rs-rolebar rs-rolebar-item rs-rolebar-dot rs-role-pill rs-perm-grid rs-role-radio rs-radio-dot rs-role-radio-text rs-cap rs-cap-row rs-cap-mark rs-2fa rs-2fa-dash rs-user-cell rs-user-id rs-check rs-field rs-field-label rs-danger; do
  n=$(grep -c "\.$cls" ui/src/styles.css)
  echo "$cls: $n"
done
```
Expected: a list showing which classes already exist (n>0) and which are missing (n=0).

- [ ] **Step 2: Port missing rules from the design stylesheet**

For each class reported as `0` in Step 1, copy its rule(s) from `design/rustapi/styles.css` into `ui/src/styles.css`. Read the design stylesheet to extract them:

```bash
grep -n "rs-rolebar\|rs-role-pill\|rs-perm-grid\|rs-role-radio\|rs-cap\|rs-2fa\|rs-user-cell\|rs-user-id\|rs-check\|rs-field-label\|rs-danger" design/rustapi/styles.css
```

Copy the matching rule blocks verbatim into `ui/src/styles.css` (append to the end). Only copy rules for classes that Step 1 reported as missing — do not duplicate existing ones. If a class exists in neither stylesheet (e.g. `rs-check`), add a minimal rule:

```css
.rs-check {
  width: 16px; height: 16px; border: 1px solid var(--border, #d4d4d8);
  border-radius: 4px; display: inline-flex; align-items: center;
  justify-content: center; cursor: pointer;
}
.rs-check.is-on { background: var(--accent); border-color: var(--accent); color: #fff; }
.rs-danger { color: #c0392b; }
```

- [ ] **Step 3: Build to confirm CSS is valid + bundle emits**

Run: `cd ui && pnpm build`
Expected: `tsc -b` + `vite build` succeed; bundle in `ui/dist`.

- [ ] **Step 4: Commit**

```bash
git add ui/src/styles.css
git commit -m "style(ui): port role/user CSS for Users screens"
```

---

## Task 10: Manual smoke verification

**Files:** none (verification only)

- [ ] **Step 1: Start backend (empty DB) + serve the built UI**

Run:
```bash
docker rm -f rustapi-um-pg >/dev/null 2>&1
docker run -d --name rustapi-um-pg -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=rustapi_um -p 55433:5432 postgres:16
sleep 4
export DATABASE_URL="postgres://postgres:postgres@localhost:55433/rustapi_um"
export RUSTAPI_JWT_SECRET=$(openssl rand -hex 32)
export RUSTAPI_STUDIO_DIR=$PWD/ui/dist
export RUSTAPI_SEED=false
export RUSTAPI_BIND=127.0.0.1:8098
cargo run -p rustapi
```

> If Docker is unavailable, note that the smoke was not run and rely on the
> integration tests + typecheck.

- [ ] **Step 2: Walk the flow in a browser**

Open `http://127.0.0.1:8098/studio`. Verify:
1. Create the first admin (setup form) → dashboard.
2. A "Users" icon appears in the sidebar footer (admin is logged in).
3. Click Users → the list shows the admin's own row (email, Admin role pill).
4. "Add user" → fill email + password (≥ 8) + pick a role → Create → back on the
   list, the new user appears.
5. Open the new user → change its role on the Role tab → Save → list reflects it.
6. Open the new user → Delete → confirm → it disappears from the list.
7. Open your own admin row → Delete → an error message appears (self-delete
   guard, 409); the user is not removed.

- [ ] **Step 3: Teardown + record result**

```bash
docker rm -f rustapi-um-pg >/dev/null 2>&1
```
Report which steps passed. If any failed, stop and report rather than completing.

---

## Self-Review Notes

**Spec coverage:**
- `UserRead`/`UserWrite` + role_allows → Task 1. ✓
- Store list/create/update/delete → Task 2. ✓
- `/admin/users` GET/POST/PATCH/DELETE + 409 dup + 422 pw + 404 → Task 3. ✓
- Lockout guards (self-delete, self-demote) via principal `sub` → Task 3. ✓
- Authz 403 for non-admin → Task 3 (`ensure`) + Task 4 test. ✓
- Backend tests (list/create/dup/short-pw/roles/password→login/delete/guards/403) → Task 4. ✓
- UI types + endpoints + roles module → Task 5. ✓
- Users list (email/roles/created, search, role chips, placeholders) → Task 6. ✓
- User editor (email/password/roles real; caps read-only; 2FA placeholder; delete) → Task 7. ✓
- `/users` routes + admin-only nav → Task 8. ✓
- CSS port → Task 9. ✓
- Manual smoke → Task 10. ✓

**Notes / decisions surfaced:**
- No single-user GET endpoint this slice; the editor loads via `listUsers()` and
  finds by id (small admin dataset). Acceptable; a `GET /admin/users/:id` is a
  trivial later addition if needed.
- `created_at` is in the DB and the spec's response shape, but the `UserView`
  here omits it (the list screen shows roles, not created, to match available
  columns). If the created column is wanted in the UI, add `created_at` to
  `UserView` + `User` type — noted, not built, to avoid an unused field.
- The Users list intentionally has no per-row checkbox bulk actions wired (the
  `Checkbox` component is included for parity but bulk actions are a placeholder
  per spec).

**Type consistency:** `listUsers`/`createUser`/`updateUser`/`deleteUser` (Task 5)
match Tasks 6–7 usage. `User`/`NewUser`/`PatchUser` (Task 5) match endpoint
signatures. `ROLES`/`roleOf`/`CAPS`/`capsFor` (Task 5) match Tasks 6–7. Route
paths `/users`, `/users/new`, `/users/:id` (Task 8) match `navigate(...)` calls
in Tasks 6–7. `Action::UserRead`/`UserWrite` (Task 1) match Task 3 `ensure`
calls.
```
