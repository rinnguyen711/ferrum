# Roles Screen (Read-only) + Logo Swap Design

**Date:** 2026-06-03
**Status:** Approved, ready for implementation plan

## Goal

Ship the "Roles" item in the Users & Permissions panel as a read-only screen
that shows the three built-in roles, their capability matrix, and the users
assigned to each. Also replace the inline-SVG sidebar logo with `logo.png`.

## Why read-only

The backend RBAC is a hardcoded Rust map (`rustapi_core::role_allows`): three
fixed roles (`admin`/`editor`/`viewer`) with coarse actions (content R/W, schema
R/W, user R/W). There is no roles table, no per-content-type granularity, and no
way to define custom roles. A fully editable, DB-backed roles system (matching
`design/rustapi/roles.jsx`) is a large separate slice. This slice ships the
screen honestly against the current backend and defers the authz rewrite.

## Data source

Pure client-side. Zero new backend:
- Roles and capabilities come from the existing `ui/src/roles.ts` (`ROLES`,
  `CAPS`, `capsFor`) — already a mirror of `role_allows`.
- User counts and members come from the existing `listUsers()` endpoint, grouped
  client-side by `roles[]`.

## Roles Screen

Two screens, ported from `design/rustapi/roles.jsx` but stripped to read-only
and to the backend's coarse capabilities:

### `screens/Roles.tsx` (list)
- Table over `ROLES`. Columns: Role (color dot + name), Description, Users
  (count + a small avatar stack derived from `listUsers()`).
- No "Create role" button, no per-row duplicate/delete (those require editable
  DB roles — out of scope). Row click → `/roles/:key`.

### `screens/RoleDetail.tsx` (read-only detail)
- Bare layout (topbar hidden, flush scroll) like the user/entry editors.
- Header: role name + a "Read-only" / "System role" meta pill; back button to
  `/roles`.
- Capability matrix: the five coarse capabilities from `CAPS`
  (Read content, Write content, Read schema, Write schema, Manage users),
  rendered as a read-only ✓/✗ grid via `capsFor(key)`.
- Members: users whose `roles[]` includes this key (from `listUsers()`), each
  linking to `/users/:id`. Empty state when none.
- A note: "Roles are defined in the API and cannot be edited here yet."
- Unknown `:key` → "Role not found" with a back link.

## Routing & Navigation

- New protected routes in `App.tsx`: `/roles` (list) and `/roles/:key` (detail),
  inside the `RequireAuth` + `Layout` group.
- `Layout.sectionFromPath`: `/roles` maps to the `"users"` section so the Users &
  Permissions rail item stays active and the secondary panel renders.
- `Layout` bare-layout match: add `/roles/:key` (alongside `/users/new`,
  `/users/:id`, `/content/:type/:id`) so the detail renders full-bleed.
- `UsersPanel` (in `shell.tsx`): the "Roles" item becomes active —
  `guardedNavigate("/roles")`, active when the path starts with `/roles`.
  "Audit logs" and "Single sign-on" stay disabled placeholders.

## Logo Swap

- Copy `logo.png` (repo root) to `ui/public/logo.png` (Vite serves `public/` at
  the base root).
- Replace the inline SVG in `RailLogo` (`shell.tsx`) with an `<img>` whose `src`
  is `` `${import.meta.env.BASE_URL}logo.png` `` so it resolves under the
  `/studio/` base path. Keep the `rs-logo` wrapper and sizing (~22–28px square),
  `alt="Rustapi"`.

## Error / Empty Handling

- Roles are static — no fetch can fail for the role data itself.
- User counts/members come from `listUsers()`. On load error, show counts as "—"
  and an empty members list; the screen still renders the roles.
- Unknown role key on the detail route → "Role not found" + back link.

## Testing

- `cd ui && pnpm typecheck` clean; `pnpm build` succeeds.
- Playwright smoke (admin logged in): Users & Permissions → Roles → list shows
  three roles with user counts → open `admin` → capability grid all ✓, members
  list includes the admin → the sidebar logo renders as an `<img>` (logo.png),
  not the old inline SVG.

## Out of Scope (Future Slices)

Create / edit / delete roles · per-content-type permission matrix · custom roles
· DB-backed dynamic authorization (replacing `role_allows`) · audit logs · SSO.
