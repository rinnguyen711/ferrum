# UI Auth — Email/Password + JWT Design

**Date:** 2026-06-03
**Status:** Approved, ready for implementation plan

## Goal

Migrate the admin studio UI from the removed `x-api-key` shared-secret auth to
the email/password + JWT auth shipped in backend slice 1. Add a first-run
create-admin flow and a logout control. After this slice, the studio logs in
against the real backend.

## Background

Backend slice 1 (merged, `0fe6eaa`) removed `RUSTAPI_ADMIN_KEY` and the
`x-api-key` header entirely. The UI still authenticates with `x-api-key`, so
studio login is currently broken against the new backend. The UI auth layer is
well-isolated, so the migration is contained to a few seam files plus one small
backend addition.

## Decisions

| Topic | Decision |
|---|---|
| First-run setup in UI | Login screen auto-detects setup mode (create-admin vs login) |
| Setup detection | New public `GET /auth/setup` → `{ setup_required: bool }` |
| Token storage | localStorage (reuse existing pattern) |
| Logout | Button in Topbar user area; subtitle shows email, not "API key" |
| Email display | Decode JWT payload client-side (display only) |
| Testing | `pnpm typecheck` + `cargo test` + manual/playwright smoke; no new UI test infra |

## Architecture

UI auth is already isolated behind a small seam. Touch-points:

| File | Change |
|---|---|
| `ui/src/api/client.ts` | Send `Authorization: Bearer <token>` instead of `x-api-key`; update 401 message wording |
| `ui/src/auth.ts` | Rename key→token (`getToken`/`setToken`/`clearToken`); keep localStorage; add `getClaims()` JWT-payload decoder |
| `ui/src/api/endpoints.ts` | Remove `checkAuth(key)`; add `login(email, pw)`, `fetchSetupStatus()`, `setup(email, pw)` |
| `ui/src/screens/Login.tsx` | Two-mode form (login / create-admin), email + password |
| `ui/src/App.tsx` | `RequireAuth` uses `getToken()`; `AuthErrorBridge` clears token |
| `ui/src/components/shell.tsx` (Topbar) | Logout button + show logged-in email |

Backend:

| File | Change |
|---|---|
| `crates/http/src/auth/handlers.rs` | Add `setup_status` handler → `{ setup_required }` |
| `crates/http/src/auth/users.rs` | Add `any_users(pool) -> bool` (`SELECT EXISTS`) |
| `crates/http/src/auth/mod.rs` | Wire `GET /auth/setup` into `public_router()` |

The `GET` and `POST` on `/auth/setup` coexist on the public router.

## Data Flow

### App boot / protected route
`RequireAuth` checks `getToken()`. None → redirect `/login`. Present → render;
`client.ts` attaches `Authorization: Bearer <token>` to every request.

### Login screen load
1. Call `fetchSetupStatus()` → `GET /auth/setup` → `{ setup_required }`.
2. `true` → render **Create-admin** form (email, password, confirm password).
   `false` → render **Login** form (email, password).
3. While the status request is in flight: disabled inputs / spinner.
4. If the status request fails (network): show "Can't reach the API." and a
   retry affordance; default to login mode is not assumed.

### Create-admin submit
`setup(email, pw)` → `POST /auth/setup`.
- 201 → immediately call `login(email, pw)` (setup returns no token) → store
  token → navigate to `from`.
- 409 → flip to login mode, show "Admin already exists — please sign in."
- 422 (password too short) → field error under the password input.

### Login submit
`login(email, pw)` → `POST /auth/login` → `{ token, expires_at }` →
`setToken(token)` → navigate to `from`.
- 401 → "Invalid email or password."

### Logout
Clear token → navigate `/login`. Pure client-side (no backend logout endpoint
in slice 1).

### 401 on a stored-token request
Existing `AuthErrorBridge` mechanism: `client.ts` fires the registered handler,
which clears the token and redirects to `/login`. Unchanged except it now
clears the token (formerly the key).

### Email display
`auth.ts` `getClaims()` base64-decodes the JWT payload segment to read
`email`/`roles` for the Topbar. Display only — no signature verification (the
server remains authoritative). Returns `null` on a malformed/absent token.

## Error Handling

| Condition | UI behavior |
|---|---|
| Network down | Existing `ApiError(0, "network")` → "Can't reach the API." |
| Login 401 | "Invalid email or password." |
| Setup 409 | Flip to login mode + notice "Admin already exists — please sign in." |
| Setup 422 (password < 8) | Field error under password input (client.ts already parses `details.fields[]`) |
| Password confirm mismatch | Client-side validation, no request fired |

## Testing

- `pnpm typecheck` clean.
- `cargo test` — new `GET /auth/setup` integration test (empty DB → `true`;
  after setup → `false`) plus the existing suite green.
- Manual / playwright smoke against a live stack: empty DB → create-admin →
  auto-login → dashboard → logout → login → reach a protected screen.

## Out of Scope (Future Slices)

Refresh tokens / remember-me · password reset · role-aware UI (hiding actions
by role) · multi-user management screen · removing any remaining legacy "API
key" labels outside the listed files.
