# User Management — CRUD + Users Screen Design

**Date:** 2026-06-03
**Status:** Approved, ready for implementation plan

## Goal

Let an admin manage users from the studio: a dedicated Users screen (list +
detail editor) backed by new admin-only user CRUD endpoints. Mirror the
reference layout in `design/rustapi/users.jsx`; surface only what the backend
supports as real, and mark everything else as a "Coming soon" placeholder.

## Should User go in the Content-Type Builder?

**No.** `_users` is a system table whose schema is fixed in Rust — auth queries
depend on `id` / `email` / `password_hash` / `roles`. The Content-Type Builder
edits user-defined `_content_types` only; it must not mutate `_users`. Custom
per-user fields are a later slice via a `UserProfile` content-type with a 1-1
relation to the user (deferred — see Out of Scope).

## Scope

- **Backend:** full admin-only user CRUD (list, create, update, delete).
- **UI:** `/users` screen (list + editor) + a sidebar nav entry; ported from
  `design/rustapi/users.jsx`, wired to the API. Features the backend lacks are
  rendered as disabled `data-placeholder` "Coming soon" controls.
- `_users` stays a locked system table. Admin sets the initial password on
  create (no email/invite infrastructure exists).

## Backend

### Authorization

Add `Action::UserRead` and `Action::UserWrite` to `rustapi_core`. In
`role_allows`: `admin` grants both; `editor`/`viewer`/unknown grant neither.
Routes call the existing `ensure(...)`/`Authz::can` path; denial → `403`
Forbidden.

### Endpoints

All under `/admin/users`, behind `require_auth` (bearer JWT) and the authz
check. `password_hash` is never returned.

| Method | Path | Body | Success |
|---|---|---|---|
| GET | `/admin/users` | — | `200 [{id, email, roles, created_at}]` |
| POST | `/admin/users` | `{email, password, roles}` | `201 {id, email, roles, created_at}` |
| PATCH | `/admin/users/:id` | `{email?, roles?, password?}` | `200 {id, email, roles, created_at}` |
| DELETE | `/admin/users/:id` | — | `204` |

Rules:
- POST validates password length (min 8, reuse the setup validator). Dup email
  → `409`. Empty `roles` allowed (a user with no permissions); UI defaults to
  one role.
- PATCH only rehashes when `password` is present and non-empty. `email`/`roles`
  updated when present. Dup email → `409`.
- **Lockout guards** (compare `:id` against the requesting principal's `sub`):
  - DELETE of self → `409` ("cannot delete your own account").
  - PATCH that removes `admin` from self → `409` ("cannot remove your own admin
    role").
- 404 when `:id` is unknown (PATCH/DELETE).

### Store (`crates/http/src/auth/users.rs`)

Add: `list(pool) -> Vec<UserRow>`, `update(pool, id, email?, password_hash?,
roles?) -> Option<UserRow>` (None = not found), `delete(pool, id) -> bool`
(false = not found), and a CRUD `create(pool, email, password_hash, roles) ->
UserRow` (distinct from the setup-only `insert_first_admin`). A `UserView`-style
serialization omits `password_hash`.

### Handlers

New module `crates/http/src/admin/users.rs` (or extend the admin routes area)
with `list`, `create`, `update`, `delete` handlers and a router wired into the
protected admin routes (alongside content-types). The principal is read from the
request `Extension<Principal>` for the lockout guards.

## UI

### Routes & navigation

- New protected routes in `App.tsx`: `/users` (list) and `/users/:id` +
  `/users/new` (editor), inside the `RequireAuth` + `Layout` group.
- Sidebar: add a "Users" item (admin concern, grouped near Settings). The item
  renders only when `getClaims()?.roles` includes `admin` (client gate; the
  server is authoritative via `403`).

### Screens

Ported from `design/rustapi/users.jsx` into real React/TS:

**`screens/Users.tsx` (list):**
- Table from `GET /admin/users`. Columns: ID, User (avatar + email), Role
  (chips from `roles[]`), Created (`relTime(created_at)`).
- Real: client-side search by email, role-filter chips, click row → editor,
  "Add user" button → `/users/new`.
- Placeholder (`data-placeholder`, disabled, "Coming soon"): Status / 2FA /
  Provider columns, bulk-action bar, Export, multi-page pager (single page now).

**`screens/UserEditor.tsx` (create + edit):**
- Account tab: email (real); password — "Set password" on create (required),
  "Reset password" on edit (sent only if filled). Username / full-name are
  omitted (no backend column).
- Role & permissions tab: role selector over the known roles
  (`admin`/`editor`/`viewer`) → real `roles[]`. Capability matrix shown
  **read-only**, derived from the role→Action map (informational). 2FA toggle is
  a disabled placeholder.
- Delete button (real) with a confirm step; self-delete shows the guard message
  returned by the backend.
- Placeholder: API tab, confirmed/blocked toggles, invite/send actions.

### Roles source

A static `ROLES` constant in the UI mirrors the backend
(`admin`/`editor`/`viewer` with display name, color, description), matching the
design's `RUSTAPI.userRoles` shape.

### API client

Add to `endpoints.ts`: `listUsers()`, `createUser(body)`, `updateUser(id,
body)`, `deleteUser(id)` and `User` / `NewUser` / `PatchUser` types in
`types.ts`.

### CSS

Port only the classes the Users screens use from `design/rustapi/styles.css`
into `ui/src/styles.css` (`rs-rolebar`, `rs-role-pill`, `rs-perm-grid`,
`rs-role-radio`, `rs-2fa`, `rs-rail-card`, `rs-cap`, etc.). No unrelated design
CSS.

## Error Handling

| Condition | Behavior |
|---|---|
| Create duplicate email | `409` → field error under email |
| Password < 8 (create) | `422` → field error under password (parsed from `details.fields[]`) |
| Self-delete | `409` → inline notice, action blocked |
| Self-demote (remove own admin) | `409` → inline notice, action blocked |
| Non-admin → `/admin/users` | `403` (RoleAuthz); UI hides the Users nav for non-admins |
| Network / 401 | Existing `ApiError` / `AuthErrorBridge` handling |

## Testing

**Backend unit:** `UserRead`/`UserWrite` rows in the `role_allows` truth table.

**Backend integration (`crates/bin/tests/`):**
- List users (admin token) → 200 with the seeded admin.
- Create → 201; duplicate email → 409; short password → 422.
- Update roles → 200, reflected on re-fetch.
- Update password → 200, then login with the new password succeeds.
- Delete a second user → 204; self-delete → 409; self-demote → 409.
- Non-admin token → 403 on each route. (Create a second non-admin user via the
  admin to obtain its token.)

**UI:** `pnpm typecheck` clean; playwright smoke — sign in as admin → open Users
→ create a user → edit its role → delete it → attempt self-delete and see the
guard.

## Out of Scope (Future Slices)

Status (confirmed/blocked/suspended) · 2FA · OAuth providers · username /
full-name fields · invite emails · per-role capability editing (the Roles matrix
screen, `design/rustapi/roles.jsx`) · export · server-side pagination ·
`UserProfile` custom fields via a 1-1 relation to `_users`.
