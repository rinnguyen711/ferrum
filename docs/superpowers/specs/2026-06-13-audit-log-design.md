# Audit Log — Design

**Date:** 2026-06-13
**Status:** Approved (design), pending implementation plan
**Pairs with:** users + roles + webhooks (already shipped)

## Goal

An immutable record of every stateful action across the workspace — who changed
what, when. Surfaced as a Settings ▸ Administration screen (`design/ferrum/audit.jsx`
is the source-of-truth mockup) plus a read API.

## Decisions (locked)

| Question | Choice |
|---|---|
| Actor capture | Dedicated `AuditSink` (DB-backed) in `AppState`, called alongside the existing `events.emit` at each write handler and directly from the login handler. The webhook `EventSink` is left untouched. Actor + request-context + changes are gathered at the handler (all already in scope). |
| Actor shape | Typed actor + denormalized label (`actor_type`, `actor_id` nullable, `actor_label` snapshot) — survives deletion. |
| Scope | Everything stateful: content + schema + users + roles + webhooks + tokens + settings + auth (login/login_failed). |
| Diff detail | Changed-fields diff: `[{field, from, to}]`. Create = new values; delete = note/prior; update = changed fields only. |
| Retention | 90-day prune (background job, `AUDIT_RETENTION_DAYS=90`). Matches mockup copy. |
| Context fidelity | IP + raw User-Agent string + request-id. Location deferred (show "—", no GeoIP dependency). |
| 2FA action | Dropped — no 2FA exists in codebase, nothing to emit. |
| UI | Full screen ported from `audit.jsx`: stat cards, category tabs, search/actor/status filters, expandable rows, CSV export. |

## Approach (chosen: A, refined)

Add a dedicated `AuditSink` (DB-backed) to `AppState`, mirroring the existing
`EventSink` wiring. Each write handler calls `state.audit.record(entry)`
alongside its existing `events.emit(...)`; the login handler calls it directly.
`Actor`, `RequestContext`, and the changed-fields diff are gathered at the
handler, where `principal` (or attempted email) and before/after state are
already in scope.

**Why dedicated `AuditSink` over wrapping `Event`:** the login path has no
`Event`/`EventSink` at all, and the webhook `EventSink::emit` signature would
otherwise have to change at every call site. A separate sink keeps webhooks
untouched and gives one clear capture call per handler.

Rejected:
- **B (Tower middleware recording method/path/status):** loses domain semantics
  (can't distinguish publish from edit), can't produce a field diff, can't
  express login outcomes cleanly.
- **Wrapping `Event` in an envelope through `EventSink`:** more invasive (touches
  the webhook sink + every emit signature) and login still needs a side path.

## Section 1 — Data model & migration

`crates/schema/migrations/0012_audit_log.sql` — table `_audit_log`:

| column | type | notes |
|---|---|---|
| `id` | UUID PK | `gen_random_uuid()` |
| `action` | TEXT NOT NULL | dotted key: `entry.publish`, `auth.login`, `role.change`… |
| `category` | TEXT NOT NULL | `content`/`auth`/`settings`/`perm`; derived from action, stored for fast tab filter + index |
| `status` | TEXT NOT NULL | `success`/`failed`, CHECK constrained |
| `actor_type` | TEXT NOT NULL | `user`/`api_token`/`system` |
| `actor_id` | UUID NULL | nullable (system / unknown-user failed login) |
| `actor_label` | TEXT NOT NULL | denormalized snapshot (email or token name) |
| `target_type` | TEXT NULL | `article`/`user`/`token`/`webhook`/`role`/`session`/`settings` |
| `target_id` | TEXT NULL | uuid-or-string (sessions use email) |
| `target_label` | TEXT NULL | display label (entry title, user name…) |
| `changes` | JSONB NULL | `[{field, from, to}]` |
| `note` | TEXT NULL | freeform (e.g. "Wrong password.") |
| `ip` | TEXT NULL | client IP |
| `user_agent` | TEXT NULL | raw UA string |
| `request_id` | TEXT NULL | per-request id |
| `created_at` | TIMESTAMPTZ NOT NULL | `now()` |

Indexes:
- `(created_at DESC)` — default list
- `(category, created_at DESC)` — tab filter
- `(actor_id, created_at DESC)` — actor filter
- `(target_type, target_id, created_at DESC)` — per-target history

Prune: `DELETE WHERE created_at < now() - INTERVAL '90 days'` (interval from env).

> **Note:** new migration not applied until the `schema` crate rebuilds — see
> memory `sqlx-migrate-rebuild`.

## Section 2 — Capture flow

**New `crates/core` types:**
- `Actor { kind: ActorKind, id: Option<Uuid>, label: String }` — built from
  `Principal`. `User` → email label; `ApiToken` → token-name label; unknown
  failed login → `system`.
- `ActorKind` enum: `User`, `ApiToken`, `System`.
- `RequestContext { ip: Option<String>, user_agent: Option<String>, request_id: Option<String> }`.
- `FieldChange { field: String, from: String, to: String }`.
- `AuditEntry { action: String, category: String, status: String, actor: Actor,
  target_type: Option<String>, target_id: Option<String>, target_label:
  Option<String>, changes: Vec<FieldChange>, note: Option<String>, ctx:
  RequestContext }` — the rich record a handler hands to the sink.

**`AuditSink` trait + DB impl** (`crates/http` state, mirrors `EventSink`):
- `pub trait AuditSink { async fn record(&self, entry: AuditEntry); }`
- `Arc<dyn AuditSink>` in `AppState`, default `NoopAuditSink`.
- DB impl inserts one `_audit_log` row, **fire-and-forget** (spawn + log on
  error) — an audit write never blocks or fails the user action.
- Webhook `EventSink` is untouched.

**Request context middleware** (Tower layer): sets `request_id` (uuid), reads
`X-Forwarded-For`/peer IP and `User-Agent`, stores them in a request extension.
Handlers read it alongside `Principal`.

**Wiring per source:**
- *Content/schema* — enrich existing emit sites with actor + ctx + changes.
  Update diff uses before/after; WriteHook `before_write` carries the prior
  record — confirm during implementation and reuse it.
- *Auth* (`login` handler) — emit `auth.login` (success) / `auth.login_failed`
  (status=failed, note). No `EventSink` today; call audit sink directly.
- *Admin* (tokens, webhooks, roles, users, settings) — add emit at each
  mutating handler.

## Section 3 — Read API

`GET /api/admin/audit` (admin-only):
- `category`, `status`, `actor_id`, `target_type`+`target_id`, `q` (search over
  actor_label / target_label / action), `page`, `per_page` (25/50, default 25).
- Response: `{ rows: [AuditRow], total, page, per_page }`, ordered `created_at DESC`.

`GET /api/admin/audit/stats` — 4 cards: `events_logged` (90d), `sign_ins` +
`failed_attempts`, `content_changes`, `failed_actions`. Single query, conditional
aggregates.

`GET /api/admin/audit/export` — CSV of the current filter set (reuse import/export
CSV pattern, auth via fetch+blob).

Category tab badges: `GROUP BY category` count query.

SQL in new `crates/sql/src/audit.rs`: `insert_audit`, `query_audit` (dynamic
WHERE), `audit_stats`, `audit_category_counts`, `prune_audit`.

## Section 4 — Admin UI

New `ui/src/screens/AuditLog.tsx`, ported from `design/ferrum/audit.jsx`:
- Nav entry "Audit logs" under Settings ▸ ADMINISTRATION (after Roles) + route.
- Header + "Export log" primary button (fetch+blob CSV download).
- 4 `StatCard`s from `/audit/stats`.
- Category tabs (All/Content/Authentication/Settings/Permissions) with counts →
  `category` param.
- Toolbar: search (300ms debounce → `q`), Actor filter popover (`actor_id`),
  Success/Failed segment (`status`).
- Table: Time(rel) · Actor(avatar) · Action(category-tinted icon+verb) ·
  Target(type pill+label) · Context(status dot + IP). Row-expand → detail grid
  (timestamp, actor, IP, device=raw UA, request id) + changes diff + note.
- Server pagination (25/50), reuse ContentList pager pattern.

**CSS:** port `rs-audit-*` classes from `design/ferrum/styles.css` into
`ui/src/styles.css`, using DESIGN.md tokens (no hardcoded hex a token covers;
the `AUDIT_CATS` category colors are the one allowed literal set).

**Trim vs mockup:** Location → "—" (no GeoIP); device → raw UA string; 2FA action
absent.

## Section 5 — Testing & error handling

**Tests** (`crates/bin/tests/audit.rs`, ephemeral-Postgres harness via
`common/mod.rs`):
- content create/update/delete/publish each writes the correct row
- update captures field-level diff
- login success + login failure rows
- admin actions (token / webhook / role / user) write rows
- query filters: category, status, actor, target, q
- pagination
- stats aggregates
- prune drops rows older than 90 days

**Error handling:**
- audit insert failure never breaks the user action (fire-and-forget, tracing log)
- failed-login still writes (status=failed)
- missing actor → `actor_type='system'`, null id
- read endpoints admin-gated; non-admin → 403

## Crate boundaries

- `core` — `Actor`, `ActorKind`, `RequestContext`, `AuditEnvelope`, extends `Event` usage (no new deps)
- `sql` — `audit.rs` (insert/query/stats/prune)
- `http` — `AuditSink` trait + DB impl, request-context middleware, read routes
- `bin` — wires the DB audit sink into `AppState`, prune worker, integration tests

One-way dependency flow preserved.

## Out of scope (v1)

- GeoIP location lookup
- 2FA events (no 2FA in codebase)
- per-entry "History" tab on the content detail view (global screen only for now)
- configurable retention beyond a single `AUDIT_RETENTION_DAYS` env
- audit of read/list actions (writes + auth only)
