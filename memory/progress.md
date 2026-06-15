# Progress

What's done, what's in progress.

## Audit log — SHIPPED, merged to local main 2026-06-13 (branch: feat/audit-log, not pushed)

Who-changed-what-when across the workspace. Spec/plan in `docs/superpowers/{specs,plans}/2026-06-13-audit-log*`.

### Architecture
- Dedicated `AuditSink` trait in `crates/http/src/state.rs` (parallel to `EventSink`, NOT routed through it). Default `NoopAuditSink`; real `DbAuditSink` in `crates/bin/src/audit_sink.rs`.
- **Fire-and-forget**: `record()` spawns a task; insert failure is logged, never breaks the user request. → tests must poll (`common::wait_for_audit`), never assert immediately.
- Core types in `crates/core/src/audit.rs`: `AuditEntry` (builder), `Actor`/`ActorKind`, `RequestContext`, `FieldChange`, `category_for(action)`. Actor = typed + denormalized label (survives deletion).
- `_audit_log` table = migration `0012_audit_log.sql` (4 indexes). 90-day prune worker, env `AUDIT_RETENTION_DAYS` (default 90).
- Request context (IP / raw UA / request-id) via `crates/http/src/reqctx.rs` middleware on the merged router (covers public routes so login can read it).

### Recorded actions (scope = everything stateful)
content create/update/delete/publish/unpublish (REST handlers in `routes/content.rs` AND GraphQL resolvers in `graphql/resolve.rs`), schema create/update/delete (`routes/schema.rs`), auth.login/login_failed (`auth/handlers.rs`), token.create/revoke (`api_tokens.rs`), webhook.create (`webhooks.rs`), role.change (`roles.rs`), user.invite/suspend (`users.rs`, suspend gated on `blocked:true`), settings.update (`media.rs put_settings`). Action→category mapping in `category_for`; unknown prefix → "settings".

### Read API + UI
- `GET /api/admin/audit` (filters: category/status/actor_id/target/q + pagination), `/stats`, `/export` (CSV). Admin-gated via `ensure_admin` (`authz.can(.., UserWrite, "")`), in protected router tree. SQL in `crates/sql/src/audit.rs` (dynamic WHERE is `$N`-placeholder-only, injection-safe; search bound as param).
- UI: `ui/src/screens/AuditLog.tsx` (ported from `design/rustapi/audit.jsx`), `ui/src/api/audit.ts`, shared `ui/src/components/StatCard.tsx` (extracted from Dashboard). Nav `to:"/settings/audit"` in BOTH settings sidebars (`shell.tsx` SettingsPanel groups + UsersPanel). 9 integration tests in `crates/bin/tests/audit.rs`, all green.

### Deferred (per spec, not bugs)
GeoIP location (shows "—"), 2FA events (no 2FA in codebase), per-entry History tab.

## Webhooks feature (branch: feat/webhooks)

### Done
- `DbEventSink` + background retry worker (`crates/bin/src/webhook_worker.rs`)
- Integration tests: delivery, retries, HMAC, cascade (`crates/bin/tests/webhooks.rs`)
- Concurrent delivery via `FOR UPDATE SKIP LOCKED`
- Admin UI: webhook list page (`ui/src/screens/Webhooks.tsx`)
- Admin UI: create webhook page (`ui/src/screens/WebhookEditor.tsx`)
- Admin UI: webhook detail page with delivery log (`ui/src/screens/WebhookDetail.tsx`)
- CSS for webhook editor classes (rs-events, rs-kv, rs-event-row, etc) in `ui/src/styles.css`
- Richer backend delivery logs (structured tracing: enqueue, attempt, success w/ latency, retry, permanent failure)
- Inline enable/disable toggle on list + detail pages
- Row click → detail page navigation
- Delivery log moved from inline list expand → detail page

### Done (continued)
- GET `/admin/webhooks/:id` endpoint added; WebhookDetail uses direct fetch
- Removed Send test button from WebhookDetail
- Component fields render sub-fields in Create/Edit Entry (was empty before)
- FieldConfigModal: component uid is now a dropdown of existing components
- CreateComponentModal simplified to 2 fields: Name + Category (datalist suggestions, auto-derives API ID)

### Done (continued)
- `rustapi migrate` subcommand: interactive Postgres→Rustapi migration CLI
  - `crates/bin/src/migrate/` (map, inspect, prompt, apply, mod)
  - Deps: clap 4, dialoguer 0.11, indicatif 0.17
  - 6 integration tests in `crates/bin/tests/migrate.rs`
  - Known limitation: `copy_rows` binds all values as `Option<String>` — text/int work, complex types fail silently per row
  - Test with: `cargo run --bin rustapi -- migrate --source <url> --target <url>`

### Done (continued)
- Content entry import/export (CSV) — merged to main 2026-06-12
  - `GET /admin/content-types/:name/entries/export?ids=<uuid1>,<uuid2>` — comma-separated, auth via fetch+blob
  - `POST /admin/content-types/:name/entries/import` — multipart CSV, upsert by id, per-row errors non-fatal, 1000-row limit
  - `select_by_ids_sql` in `crates/sql/src/dml.rs`, `bind_one_for_import` in `crates/schema/src/bind.rs`
  - `row_to_csv_record`, `csv_row_to_body` in `crates/http/src/entry.rs`
  - 8 integration tests in `crates/bin/tests/import_export.rs`
  - UI: Export CSV button in bulkbar, Import CSV button in toolbar with result banner (`ui/src/screens/ContentList.tsx`)
  - Known limitations: no WriteHook/event emission, no component field validation, M2M/multi-media rejected per-row, partial failures non-transactional

## Content Manager + Builder UX session (2026-06-12)

### Done — all uncommitted on main working tree, awaiting user browser verify
- **Content list filters** — `ui/src/screens/FiltersMenu.tsx` popover (field/op/value rules, AND-only),
  server-side via existing `crates/http/src/filter.rs` parser; `listEntries` gained `filters` pairs opt;
  one rule auto-seeded on popover open; 300ms debounce; m2m/json/media excluded.
- **Builder unified save UX** — `ComponentDraft` + `BuilderDraft` union in `ui/src/builder/draftModel.ts`;
  `BuilderDraftContext` saves/discards both kinds; floating `SaveBar` (`ui/src/builder/SaveBar.tsx`,
  sticky bottom, renders only when dirty) replaced panel Save + ComponentEditor header Save;
  `SaveConfirmModal` (field-drop confirm) moved shell → SaveBar; ComponentEditor rewritten onto context
  (gains dirty badge, beforeunload, nav guards; fixed silent loss of field `default` values on re-save).
- **Content list server ops** — real pagination (25/50/100, sliding 5-window, clamp after delete),
  server-side search (`filters[<title>][$containsi]`), sort toggle (`updated_at:desc` ↔ `<title>:asc`),
  enum-status tabs server-filtered (count only on active tab), bulk publish/unpublish/delete
  (sequential, failure notice, styled delete confirm). Client `filtered` memo deleted.
- Specs + plans in `docs/superpowers/specs/` and `docs/superpowers/plans/` (3 × 2026-06-12 files).
- Verified: `pnpm typecheck`, `cargo clippy`, full `cargo test --workspace` green.

### In progress / next
- Manual browser verification of all three features (user; agent blocked — no dev admin creds).
- Improvement backlog surveyed: Preview API button (rs-api-note 

## GraphQL surface session (2026-06-13)

### Done — MERGED to main (merge commit dcaf539, --no-ff bundle of branch feat/graphql-surface, 13 commits + 2 doc commits)
- **GraphQL API alongside REST**, full CRUD parity, http crate only (core/sql/schema stay GraphQL-unaware).
  - `crates/http/src/graphql/`: `scalars.rs` (FieldKind→async-graphql dynamic TypeRef, mirrors openapi/schema.rs),
    `build.rs` (runtime `dynamic::Schema` from SchemaRegistry — Collection types only; per-type Object/Input/List
    envelope/enums; shared UUID/DateTime/JSON scalars + Meta; `_empty` placeholder when registry empty),
    `resolve.rs` (resolvers delegating to shared content fns; reads AppState+Principal from ctx.data),
    `handler.rs` (`POST /api/graphql` + GraphiQL GET gated by docs_enabled), `mod.rs` (`GqlRegistry` RwLock schema cache, mirrors RoleRegistry).
  - `routes/content.rs` refactored: REST handler cores extracted to `pub(crate)` `list_entries/get_entry/create_entry/update_entry/delete_entry`
    (+ `authorize_collection` shared preamble); both REST and GraphQL call them → same authz/hooks/events/validation.
  - `AppState.gql: GqlRegistry`; built at boot (main.rs), rebuilt on content-type create/patch/delete (`rebuild_gql` in routes/schema.rs, non-fatal).
  - Deps: async-graphql 7.2.1 (dynamic-schema, chrono); async-graphql-axum **pinned =7.0.13** (>=7.0.14 needs axum 0.8, workspace is 0.7). Toolchain bumped 1.88→1.89 (async-graphql 7 MSRV).
  - 6 integration tests `crates/bin/tests/graphql.rs`. Full workspace 531 tests green, clippy+fmt clean.
- Spec + plan in `docs/superpowers/{specs,plans}/2026-06-12-graphql-surface*`.

### Nested populate — MERGED to main (merge af904b3, branch feat/graphql-nested-populate, 9 commits) 2026-06-13
- Relation/media fields now OBJECTS (was scalar UUID), populated ONE level from the GraphQL selection set, reusing REST's batched populate (no N+1). Inputs stay scalar UUID id(s).
- Every content type (incl. Single) registered as a GraphQL object → relation targets never dangle. Media object = real AssetView fields (id/file_name/mime_type/size_bytes-as-JSON/...).
- 16 graphql integration tests. See gotchas.md for the 2 bugs integration caught (look_ahead .field descent, media-not-in-populate).

### v1 limitations still deferred (see decisions.md + gotchas.md)
- Multi-level nested populate (depth 2+ relations null, required deep relation errors). Per-relation args. DataLoader (not needed at one level).
- Single-types excluded from GraphQL root fields. No subscriptions. Type-level authz only (row-level is roadmap #3 — the last unshipped roadmap item).

### Roadmap status (docs/superpowers/specs/2026-06-07-extensibility-roadmap.md)
- #1 write hooks ✓, #2 injectable routers ✓, #4 async events (webhooks) ✓, #5 components ✓.
- #3 row/field-level authz = ONLY unshipped roadmap item. Other backlog: Preview API button, i18n, audit log, scheduled publish.

## Custom roles + role/user editor redesign session (2026-06-12)

### Done — MERGED to main (commit 9ddf160, --no-ff bundle of branch feat/custom-roles, 13 commits)
- **Custom roles, full stack** — DB-backed roles with per-(content_type, action) permissions.
  - `core`: `verb_to_action` + `PERM_VERBS` (find/findOne→ContentRead, create/update/publish→ContentWrite, delete→ContentDelete).
  - migration `0010_roles.sql`: `_roles` + `_role_permissions`, seeds system roles admin/editor/viewer (is_system).
  - `sql/roles.rs`: list/get/upsert/delete/set_permissions/load_all.
  - `http`: `RoleRegistry` (RwLock cache, mirrors SchemaRegistry, reload_from_db); `RoleAuthz` rewritten to
    hold `Arc<RoleRegistry>` — system roles keep hardcoded `role_allows`, custom roles resolve per-type from cache.
    `AppState` gained `roles` field. `/admin/roles` CRUD (`routes/roles.rs`), gated UserWrite, system-locked, cache-reload on mutate.
  - `bin`: registry hydrated at boot. 9 integration tests `crates/bin/tests/integration_roles.rs`.
  - UI: server-driven Roles list, `RoleEditor.tsx` (permission matrix), Users/UserEditor rewired to live roles, RoleDetail deleted.
- **Role editor redesign** to match design: two-col body + right rail (Role/Members cards), tabs, collapsible
  permission cards (chevron, tinted icon square, `api::`/`plugin::` scope), 2-col verb checkbox grid, 2-col fields,
  bold labels, color field + Members tab removed (rail already shows members).
- **User detail redesign** (full stack, minimal field set): migration `0011_user_status.sql` adds `_users.confirmed`,
  `_users.blocked`. `UserRow`→FromRow struct w/ shared COLS const, update() takes confirmed/blocked. UI: tabs
  (Account/Role&perms/API), Confirmed+Blocked toggles (persist), right rail PROFILE/SECURITY/ACTIVITY, status pill.
  Honest gaps (no infra): full_name/username dropped, Provider=Email/2FA=Disabled/LastActive omitted (static display).

### Verified
- `cargo test --workspace` green (integration suites flake on cold parallel runs — pass isolated), clippy clean, `cargo fmt --check` clean, UI `pnpm typecheck`+`pnpm build` clean.

### Not done
- Browser verification still pending (no dev admin creds; won't seed user's dev DB).
- main not pushed to remote; branch feat/custom-roles not deleted.

## Schema-as-code: TOML sync on startup (2026-06-15, branch feat/schema-as-code-toml)
Declarative content-type + component definitions in TOML, synced to DB at boot. Specs/plans under docs/superpowers/.
- **Config:** `RUSTAPI_SCHEMA_DIR` (dir of *.toml, merged) or `RUSTAPI_SCHEMA_FILE`; `RUSTAPI_SCHEMA_SYNC=additive`(default)|`full`. `RUSTAPI_SEED` + demo seed (bin/seed.rs, author/article/category) REMOVED — ships empty.
- **Engine:** `crates/schema/src/sync.rs` — `parse_schema`/`load_desired`→`ParsedSchema{content_types, components}`; pure `plan_sync`/`plan_components` diffs; `order_creates` (rel deps) / `order_drops` (reverse, FK-safe); `sync_from_path(&SchemaService, &ComponentService, path, mode)` applies **components before content types**. Fail-fast: any error aborts boot.
- **Managed lock:** synced types get `options.managed=true` (`ContentType::managed()`); components get a `managed` bool column (migration `0013_component_managed.sql`, `ComponentStore`/`ComponentService` create/update gained `managed` param). HTTP rejects edit/delete on managed (409) in routes/schema.rs + routes/components.rs. UI greys them out: SchemaEditor + ComponentEditor (badge + disabled save/delete/fields; D&P toggle + display_name also locked). `managedType`/`managedComponent` in ui/src/api/types.ts.
- **Modes:** additive = create + add fields, never drop; un-manages DB types dropped from TOML. full = also drops absent types/fields (FK-ordered) + components (aborts if component still referenced — two-pass needed).
- **Rename unsupported** (documented). Draft & Publish settable from TOML via `options={draft_publish=true}`.
- Fixtures: `examples/schema/blog/` (author.toml, post.toml, seo.toml component). Integration: `crates/schema/tests/sync_it.rs`.
- 34 commits ahead of main; branch NOT merged/pushed (left as-is per user).
