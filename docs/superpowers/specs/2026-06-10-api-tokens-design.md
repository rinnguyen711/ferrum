# API Tokens

Date: 2026-06-10
Status: approved for implementation

## Context

All rustapi endpoints require a user JWT today. External consumers (frontend
apps, static site generators, server-side renderers) have no way to access
content without embedding user credentials. API tokens provide a safe,
revocable, scoped credential designed to be embedded in client code.

## Goals

- Admins create named tokens with explicit action scopes and an optional expiry.
- Tokens authenticate via `Authorization: Bearer <token>` — same header as JWT,
  no client-side changes needed.
- Raw token shown once on creation; only the hash is stored.
- Tokens are revocable at any time from the admin UI.

## Data Model

Migration `0007_api_tokens.sql`:

```sql
CREATE TABLE _api_tokens (
  id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
  name         TEXT        NOT NULL,
  token_hash   TEXT        NOT NULL UNIQUE,  -- SHA-256 hex of raw token
  scopes       TEXT[]      NOT NULL,          -- e.g. {"content:read"}
  expires_at   TIMESTAMPTZ,                   -- NULL = no expiry
  last_used_at TIMESTAMPTZ,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

**Token format:** `rat_` prefix + 32 random bytes as lowercase hex = 68 chars
total. The `rat_` prefix makes tokens identifiable in logs and secret scanners.
Only the SHA-256 hash is persisted; the raw token is never stored or retrievable
after the creation response.

## Scopes

Scopes map 1:1 to the existing `Action` enum:

| Scope | Action |
|-------|--------|
| `content:read` | `Action::ContentRead` |
| `content:write` | `Action::ContentWrite` |
| `schema:read` | `Action::SchemaRead` |
| `schema:write` | `Action::SchemaWrite` |
| `user:read` | `Action::UserRead` |
| `user:write` | `Action::UserWrite` |

A token must have at least one scope. A typical read-only delivery token carries
only `content:read`.

## Backend

### `crates/core` — `Principal`

Add a new variant:

```rust
Principal::ApiToken {
    id: Uuid,
    scopes: Vec<String>,
}
```

`Principal::kind()` returns `"api_token"` for this variant.

### `crates/core` — `role_allows` / authz

`RoleAuthz` in `crates/http/src/state.rs` gets a new match arm:

```rust
Principal::ApiToken { scopes, .. } => {
    let required = action_to_scope(action); // e.g. Action::ContentRead → "content:read"
    scopes.iter().any(|s| s == required)
}
```

`action_to_scope` is a pure function in the same file.

### `crates/sql` — `api_tokens` module

Four operations:

- `insert_token(name, hash, scopes, expires_at) -> ApiToken`
- `list_tokens() -> Vec<ApiToken>` (no hash column in result)
- `delete_token(id)`
- `lookup_by_hash(hash) -> Option<ApiToken>` — used by middleware; updates
  `last_used_at` in the same query via `RETURNING` after an `UPDATE`.

### `crates/http` — `require_auth` middleware

Current flow: extract Bearer → verify JWT → inject `Principal::User`.

New flow:

1. Extract Bearer value.
2. If the value contains exactly 2 dots → treat as JWT (existing path, unchanged).
3. Otherwise → SHA-256 hash the value → call `lookup_by_hash`.
   - Hit + not expired → inject `Principal::ApiToken { id, scopes }`.
   - Hit + expired → 401 `token_expired`.
   - Miss → 401 `unauthorized`.

JWT path has zero extra DB calls. Token path costs one `UPDATE … RETURNING`.

### `crates/http` — admin routes

Mounted inside the existing protected router. All three require
`Action::UserWrite` (admin-only — same gate as user management):

```
GET    /api/admin/tokens        list all tokens (id, name, scopes, expires_at, last_used_at, created_at)
POST   /api/admin/tokens        create token → { token: "<raw>", ...metadata }
DELETE /api/admin/tokens/:id    revoke (hard delete)
```

`POST` response includes the raw token exactly once. Subsequent `GET` responses
never include the hash or raw value.

## Admin UI

### Navigation

New entry "API Tokens" under Settings nav, routes to `/settings/api-tokens`.
Sits alongside "Media" in the settings section of the rail.

### Token list page (`/settings/api-tokens`)

- Page header: "API Tokens" + "Create token" primary button.
- Table columns: Name | Scopes | Expires | Last used | Created | (Revoke action).
- Empty state: "No API tokens yet. Create one to allow external access."
- Expired tokens shown with a muted "Expired" badge in the Expires column.

### Create token modal

Fields:
- **Name** — text input, required.
- **Scopes** — checkboxes: `content:read`, `content:write`, `schema:read`,
  `schema:write`, `user:read`, `user:write`. At least one required.
- **Expires** — optional date picker. Leave blank for no expiry.

On submit → `POST /api/admin/tokens`. On success → modal transitions to a
one-time reveal screen:

> "Your token has been created. Copy it now — it won't be shown again."
> `[rat_abc123…]  [Copy]`
> `[Done]`

Clicking Done or closing the modal dismisses it. List refreshes.

### Revoke

Each row has a "Revoke" ghost button. Click → confirm modal ("Revoke token
'name'? This cannot be undone.") → `DELETE /api/admin/tokens/:id` → row
removed. No edit flow — to change scopes, revoke and create a new token.

## Error handling

| Condition | Response |
|-----------|----------|
| Token not found | 401 |
| Token expired | 401 |
| Token lacks required scope | 403 (same as insufficient role) |
| Create with no scopes | 422 validation error |
| Create with duplicate name | allowed (name is not unique) |

## Testing

Integration tests in `crates/bin/tests/integration_api_tokens.rs`:

1. Create `content:read` token → `GET /api/article` → 200.
2. Create `content:read` token → `POST /api/article` → 403.
3. Create `content:read` + `content:write` token → both read and write → 200.
4. Create token with past `expires_at` → any request → 401.
5. Delete token → subsequent request with that token → 401.
6. Unknown token string → 401.
7. JWT auth still works unchanged (regression).
8. `last_used_at` is set after successful token auth.
