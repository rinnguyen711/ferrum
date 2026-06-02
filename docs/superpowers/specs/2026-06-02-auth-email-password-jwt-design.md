# Auth & Authz — Slice 1: Email/Password + JWT + RBAC

**Date:** 2026-06-02
**Status:** Approved, ready for implementation plan

## Goal

Replace the static `RUSTAPI_ADMIN_KEY` shared-secret with real user
authentication (email/password → JWT) and role-based authorization. This is
slice 1 of a larger auth effort; later methods (basic auth, OAuth) and
features (refresh, logout, user CRUD) build on the foundation laid here.

## Decisions

| Topic | Decision |
|---|---|
| Login method (slice 1) | Email + password |
| Session | Stateless **HS256 JWT**, access-only, TTL ~24h, no refresh token |
| Password hash | **Argon2id** (PHC string) |
| Authz model | RBAC, **multi-role per user** (`Vec<String>`), **hardcoded** role→Action map |
| Default role | `admin` |
| Bootstrap | **Remove `RUSTAPI_ADMIN_KEY`** → first-run `POST /auth/setup` (self-closing) |
| Endpoints (slice 1) | `/auth/setup`, `/auth/login`, `/auth/me` |

## Architecture

Auth lives as a **module inside `crates/http`** (not a new crate) — it needs
`AppState`, `ApiError`, and `PgPool`, all of which already live in http;
keeping it there avoids circular-dependency gymnastics.

```
crates/http/src/auth/
  mod.rs        — router wiring /auth/* (setup, login → public; me → protected)
  handlers.rs   — setup, login, me handlers
  jwt.rs        — HS256 encode/decode, Claims struct
  password.rs   — Argon2id hash + verify
  users.rs      — DB queries (find_by_email, insert, count)
```

Core type changes in `crates/core/src/principal.rs`:

- Replace `Principal::Admin` with:
  ```rust
  Principal::User { id: Uuid, email: String, roles: Vec<String> }
  ```
- Add pure fn `role_allows(role: &str, action: Action) -> bool` (unit-testable,
  no DB).

## Data Model

New migration `crates/schema/migrations/0002_users.sql`:

```sql
CREATE TABLE IF NOT EXISTS _users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,          -- argon2id PHC string
    roles         TEXT[] NOT NULL DEFAULT '{}',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS _users_email_lower ON _users (lower(email));
```

- `_` prefix matches existing `_content_types` system-table convention.
- `roles TEXT[]` enables multi-role; first admin seeded with `{admin}`.
- Email stored as-entered; uniqueness enforced case-insensitively via the
  `lower(email)` unique index.
- Argon2id PHC string self-describes salt + params → single column.
- Roles are free strings in the DB; only roles known to `role_allows` grant
  any permission. Unknown role → no permissions.

## Authorization

Replace `AlwaysAllow` with `RoleAuthz` implementing the existing `Authz`
trait. Hardcoded role→Action map:

| Role | Actions |
|---|---|
| `admin` | SchemaRead, SchemaWrite, ContentRead, ContentWrite |
| `editor` | ContentRead, ContentWrite |
| `viewer` | ContentRead |

`RoleAuthz::can(principal, action, _ct)`:

- `Principal::User { roles, .. }` → `true` if **any** role grants the action
  (union semantics), by calling `role_allows` per role.
- `content_type` argument ignored this slice (no per-type rules yet — future).

Only `admin` is exercised in slice 1 (sole seeded role); `editor`/`viewer`
are defined ahead of the user-CRUD slice.

## Endpoints

### `POST /auth/setup` — public, self-closing

- Body: `{ "email": string, "password": string }`
- Guard: `SELECT count(*) FROM _users`; if `> 0` → `409 conflict` (`setup_closed`).
- Validate password (min length 8).
- Hash password (Argon2id), insert user with `roles = {admin}`.
- Response `201 { id, email, roles }`. No auto-login — client then calls
  `/auth/login`.

### `POST /auth/login` — public

- Body: `{ "email": string, "password": string }`
- Look up user by `lower(email)`. Verify password (Argon2id).
- On missing email **or** wrong password → identical `401 invalid_credentials`
  (no user enumeration). Run an Argon2id verify even when the user is missing
  (constant-ish timing, dummy hash).
- On success → sign JWT with claims `{ sub: id, email, roles, iat, exp }`.
- Response `200 { token, expires_at }`.

### `GET /auth/me` — protected

- Returns `{ id, email, roles }` derived from the request `Principal`.

## Middleware

Replace `require_admin_key` with `require_auth`:

- Read `Authorization: Bearer <jwt>`.
- Decode + verify HS256 against `jwt_secret`. Missing / malformed / expired /
  bad-signature → `401 unauthorized`.
- Inject `Principal::User { id, email, roles }` built from the verified claims.
  No DB hit — signed claims are trusted within their TTL.
- Mounted at the same `.route_layer` position in `build_router`.
- `/auth/setup` and `/auth/login` mount on the **public** (unauthenticated)
  router; `/auth/me` and all existing admin/content routes sit behind
  `require_auth`.

## Configuration

`crates/bin/src/config.rs` and `AppConfig` (`crates/http/src/state.rs`):

- **Remove** `admin_key` / `RUSTAPI_ADMIN_KEY`.
- **Add** `jwt_secret` from `RUSTAPI_JWT_SECRET` — required, min 32 chars
  (same validation pattern the old admin key used).
- **Add** `jwt_ttl_secs` (default `86400`), optionally from
  `RUSTAPI_JWT_TTL_SECS`.
- `main.rs`: construct `AppState` with `authz: Arc::new(RoleAuthz)` instead of
  `AlwaysAllow`, and the new JWT config fields.

## Error Handling

Extend `rustapi_core::Error` and the `ApiError` → HTTP mapping:

- `Unauthorized` → `401` (reuse; update message — no longer "API key").
- **New** `Forbidden` → `403` (authz deny). Current code returns
  `Unauthorized`/401 on authz deny — fix `ensure()` in
  `routes/content.rs` (and schema routes) to map authz failure to `Forbidden`.
- **New** `Conflict` → `409` (setup-when-users-exist, duplicate email).
- Validation failures use the existing `422` path.

## Testing

**Unit:**
- `role_allows` truth table (admin/editor/viewer × all four actions, plus
  unknown role → all false).
- JWT round-trip: sign → verify → claims match; tampered/expired token rejected.
- Argon2id: hash then verify true; wrong password verify false.

**Integration (`crates/bin/tests/`, testcontainers Postgres):**
- Setup happy path → 201; second setup → 409 `setup_closed`.
- Login good creds → 200 + token; bad password → 401; unknown email → 401
  (identical body).
- `/auth/me` with valid token → 200; without token → 401.
- A protected existing route: 401 without token, 200 with admin token.

**Migrate existing tests:** all current integration tests authenticate with
`x-api-key`. Update the shared helper in `crates/bin/tests/common/mod.rs` to:
run setup → login → return a `Bearer` token + header. Centralizing this fixes
the majority of call sites with one change.

## Migration / Breaking-Change Impact

- `docker-compose.yml` + `README.md`: drop `RUSTAPI_ADMIN_KEY`, add
  `RUSTAPI_JWT_SECRET`, document the first-run `/auth/setup` step.
- Content seed path (`seed_if_empty`) is unaffected — user setup is separate
  from content seeding.
- UI: studio currently renders from mock data (API wiring still TBD per
  README). No UI auth work in this slice — studio login screen is a future
  slice.

## Out of Scope (Future Slices)

Refresh tokens · logout / token revocation · user CRUD endpoints · basic auth ·
OAuth · password reset · per-content-type authorization rules · studio login UI.
