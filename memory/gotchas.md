# Gotchas

Bugs, edge cases, surprises.

## Side effects in REST wrappers miss the GraphQL write path
- `content::create_entry`/`update_entry`/`delete_entry` are SHARED core fns. REST axum wrappers (`routes/content.rs`) AND GraphQL resolvers (`graphql/resolve.rs` create_field/update_field/delete_field) both call them.
- Audit recording was first added only in the REST wrappers → GraphQL content writes emitted webhook events but wrote NO audit row. A silent, complete blind spot for the whole `/api/graphql` write surface. Caught only in final review, not by tests.
- Fix: `entry_label`/`audit_content` made `pub(crate)` in content.rs; resolvers call `audit_content(.., RequestContext::default(), ..)` (GraphQL carries no IP/UA middleware ctx → renders "—"). `update_field` had been discarding the diff (`let (entry, _changes)`) — now keeps it.
- **Lesson:** any cross-cutting side effect (audit, events, cache-bust) added at a REST handler must also be wired into the GraphQL resolvers that share the same core fn — or pushed down into the core fn itself.

## CSV export = formula injection + RFC-4180
- Hand-rolled CSV (`format!` + strip commas) is unsafe: a cell starting `= + - @` runs as a spreadsheet formula; `"`/newline corrupts rows. Labels are attacker-influenced (entry titles, login emails).
- Use the `csv` crate (already a dep) for quoting + a `csv_safe()` guard that prefixes `'` on leading `= + - @ \t \r`. See `routes/audit.rs::export`. Content export already used the `csv` crate — copy that.

## Two settings sidebars in shell.tsx
- `ui/src/components/shell.tsx` has TWO separate settings nav renders: a `groups` array in `SettingsPanel` (~line 182) AND hardcoded buttons in `UsersPanel` (~line 258). Adding a settings nav item means wiring BOTH. The audit nav landed in `groups` but `UsersPanel` still showed it as a disabled "Coming soon" button → screen unreachable from that panel until both were fixed.

## Webhook backend: PATCH not PUT
- Route `/admin/webhooks/:id` is **PATCH**, not PUT
- Frontend `api/webhooks.ts` originally called PUT — would 405 silently
- Fixed in this session; `setWebhookEnabled` carries full body (name/url/events) because PATCH handler requires all fields

## Webhook CSS classes were missing
- `rs-events`, `rs-event-row`, `rs-event-api`, `rs-events-head`, `rs-kv`, `rs-kv-head`, `rs-kv-row`, `rs-kv-add` had no CSS rules
- TSX referenced them but styles.css had no definitions → native browser rendering (unstyled checkboxes, stacked inputs)
- Added all rules in `ui/src/styles.css` after line ~389

## GET single webhook endpoint added
- `GET /admin/webhooks/:id` now exists (`crates/sql/src/webhooks.rs::get_webhook`, handler `get_one` in `crates/http/src/routes/webhooks.rs`)
- WebhookDetail.tsx uses it directly

## Dev server on 5173 returns 426
- During this session, port 5173 returned "Upgrade Required" (not real Vite)
- A non-Vite node process had taken the port
- Use `lsof -nP -i:5173` to diagnose; start Vite on alternate port if needed

## `insert_deliveries` returns `u64` rows affected
- Useful for logging how many webhooks were queued for a given event
- Returns 0 if no enabled webhooks subscribe to that event

## `useResource.refetch()` is synchronous (returns void)
- Triggers a re-fetch via nonce increment; don't `await` it

## Component field: `_component_fields` lives in `kind_meta`, not top-level
- `field._component_fields` is always undefined
- Backend injects sub-fields as `field.kind_meta._component_fields`
- FieldInput.tsx reads: `(field.kind_meta._component_fields as Field[]) ?? field._component_fields ?? []`

## jsonb binding: use `serde_json::Value` directly, NOT `sqlx::types::Json`
- `sqlx::types::Json(v)` sends as `json` text type → Postgres rejects assignment to `jsonb` columns
- Bind `serde_json::Value` directly; sqlx maps it to `jsonb`
- Same applies to null binds: use `Option::<serde_json::Value>::None` for Component/Json/RichText kinds

## Component save fails silently if sub-fields include Media or Relation
- `validate_component_instance` called `BoundValue::from_json(FieldKind::Media, v)` → TypeMismatch error
- Fix: skip `from_json` validation for Media and Relation kinds inside component instance validation

## Dev backend :8080 — compose stack, creds unknown to agent
- Runs via docker compose: `rustapi-ferrum-1` + `rustapi-postgres-1` (DB internal-only,
  `postgres://ferrum:ferrum@postgres:5432/ferrum`). Post-rename (2026-07-09): compose SERVICE is now `ferrum`, but the on-disk dir is still `rustapi` so the compose PROJECT prefix stays `rustapi-`. Any running pre-rename stack still uses old `rustapi` DB creds until recreated.
- Admin login is NOT the README example (`admin@example.com` / `change-me-please` fails)
- For browser testing: ask user for creds. Do NOT insert users/data into this DB (user rejected)
- Vite proxy hardcoded to `:8080` in `ui/vite.config.ts` — can't point dev UI at alternate backend
- Many leftover `postgres:11-alpine` testcontainers from test runs — ignore them

## ContentList full-page loader hides popovers on refetch
- `entries.loading` flips on every refetch; original guard swapped whole view for `LoadingState`,
  unmounting open popovers. Guard is now `entries.loading && !entries.data` — keep it that way

## `.rs-btn--ghost.is-active` existed only in design CSS
- App stylesheet lacked it until filters work; design-system classes aren't guaranteed ported —
  check `ui/src/styles.css` before using any `rs-` class from the mockup

## sqlx migrate! needs schema-crate rebuild (2026-06-12)
New `crates/schema/migrations/NNNN_*.sql` is embedded at COMPILE time by
`sqlx::migrate!("./migrations")` in `crates/schema/src/lib.rs`. Building only a
downstream crate (`cargo build -p ferrum-http`) leaves `schema` cached → MIGRATOR
lacks the new file → tests 500 with Postgres `42703 column "..." does not exist`.
Fix: `touch crates/schema/src/lib.rs && cargo build -p ferrum-schema` (or build
--workspace) after adding a migration.

## Integration tests flake on cold parallel runs (2026-06-12)
`crates/bin/tests/` spin parallel testcontainers; a suite can fail once on a cold
full-workspace run then pass isolated / on the next back-to-back run. Re-run the
suite alone before treating a failure as a real regression.

## GraphQL relation/media must be scalar ids, not object refs (2026-06-13)
A relation field can target a **Single** content type (REST validation only checks the target exists, not its
kind). build.rs surfaces only Collection types as GraphQL objects → an object-ref to a Single target is a
dangling type → `async_graphql::dynamic::Schema::finish()` returns Err. Because main.rs builds the schema with
`?`, this **crashes boot** for any DB with a Collection→Single relation; on CRUD, `rebuild_gql` swallows it and
**silently freezes** the schema. Fix shipped: type Relation/Media as `UUID` scalar (list for m2m/multiple) in
scalars.rs `base_type_name`. Don't revert to object refs without handling unsurfaced targets.

## GraphQL nested-populate: two bugs only integration caught (2026-06-13)
Resolvers aren't exercised by SDL/unit tests — only `crates/bin/tests/graphql.rs` runs them. Two bugs slipped past compile + unit review:
1. **look_ahead `.field("data")` returns the `data` field itself, not its children.** `selection_fields()` on it yielded `["data"]`, so the derived populate string was empty → relations resolved null. Fix: descend one more level via `SelectionField::selection_set()` (`flat_map`). Both list (`articles→data→{fields}`) and get-one (`article→{fields}`) pass a Lookahead positioned AT the field whose sub-selection holds the entry, so both need the descent.
2. **Media must NOT go through the populate arg.** `parse_populate` (populate.rs) rejects media field names ("unknown populate field") — media is auto-embedded by the storage layer on every read. `populate_arg` must match only `FieldKind::Relation`, not Media. Including Media → whole query 400s (BAD_USER_INPUT).
Lesson: for GraphQL dynamic resolvers, write the integration test before trusting compile-green — the resolver logic has no unit coverage.

## GraphQL one-level populate: required deep relation errors, not null (2026-06-13)
Relation/media output fields are objects, populated ONE level from the selection set. A selected relation's own sub-relations aren't populated. Nullable deep relation → null (fine). But a `required` deep relation (`T!`/`[T!]!`) selected at depth 2+ has no populated value → GraphQL non-null violation → nulls the containing object (not a clean null). Documented v1 limitation; clients shouldn't select beyond one relation level. Full multi-level populate deferred.

## GraphQL endpoint 503s before first content type (2026-06-13)
`/api/graphql` returns 503 until `GqlRegistry` has a schema. Boot builds it; tests must create a content type
(triggers `rebuild_gql`) before issuing GraphQL ops. Empty registry still builds a valid schema (Query/Mutation
get an `_empty: Boolean` placeholder — async-graphql rejects a root type with zero fields).

## async-graphql 7.x needs axum 0.8; workspace is axum 0.7 (2026-06-13)
async-graphql-axum >=7.0.14 targets axum 0.8 → its extractors won't satisfy the 0.7 router (E0277 on get/post).
Pinned `async-graphql-axum = "=7.0.13"` (last 7.x on axum 0.7). Core `async-graphql` stays 7.2.1. Bumping past
this is blocked until the whole workspace migrates axum 0.7→0.8.

## All DB/integration tests live in crates/bin/tests/ (2026-06-12)
Not in sql/http crates. `sql`/`http` have only pure-logic `#[cfg(test)]` units.
DB-touching tests use the shared `crates/bin/tests/common/mod.rs` `TestApp` harness
(real Postgres + in-process router over reqwest, seeds an admin). `common/mod.rs`
constructs `AppState` — any new AppState field/authz-ctor change must update it too.

## Keyset pagination — 3 bugs unit tests missed, caught by integration + final review (2026-06-16)
1. **Datetime/Uuid keyset seek bound as text → 422 `operator does not exist: timestamp with time zone < text` (PG 42883).** read_sort_value stringified DateTime→rfc3339/Uuid→string as `BoundValue::Str`; bind.rs binds Str as OID 25 (text); seek `("created_at",id) < ($1,$2::uuid)` casts only id, so $1=text vs timestamptz column mismatches. BROKE THE DEFAULT created_at sort. Unit tests + the first integration test used integer `views` (binds I64) and dodged it. Fix: read_sort_value returns NATIVE BoundValue::DateTime/Uuid, json_to_bound rebuilds native from token (rfc3339→DateTime, parse_str→Uuid) — sqlx then binds correct OID, NO sql cast needed. LESSON: a keyset/seek integration test MUST exercise a datetime/uuid sort, not just integer.
2. **Ordering seam between page 1 (offset) and page 2+ (keyset).** First keyset page used select_list_status (offset, no cursor yet) which ordered `col dir` only; keyset pages order `(col,id) dir`. With dup sort values the boundary row order disagreed → dup at seam. Fix: add `,"id" dir` to select_list_status too.
3. **First page never returned nextCursor.** Original `keyset_mode && rows.len()==page_size` gate: first page has no cursor → keyset_mode false → no nextCursor → paging stopped after page 1. Final design: `?cursor=first` sentinel sets keyset_mode; nextCursor emitted when `keyset_mode && full page`.
- Also: non-scalar sort col (Json/RichText/Component — all pass is_sortable since is_stored_column()=true for them) reached read_sort_value → tried try_get::<String> on jsonb → 500. Fix: is_keyset_sortable guard returns 422 BEFORE the DB query.
- META: per-task spec+quality reviews caught smaller stuff; the WHOLE-BRANCH final review (over full main..HEAD diff) caught Critical #1 that every per-task review missed — no single task's diff exposed the datetime path. Keep the final cross-cutting review.

## Bench branch is 79 commits behind main; builds rustapi from working tree (2026-06-16)
bench/docker-compose.bench.yml `build: context: ..` → builds rustapi image from the CHECKED-OUT working tree, NOT a pinned commit. So to bench current code, the working tree must have it. The bench/rustapi-vs-strapi branch is old (had its own tracked memory/ + a `seed` config field main lacks). Merging main→bench: clean (no conflicts) BUT the merge DELETED working-tree memory/progress.md, gotchas.md, INDEX.md — they were tracked on bench, `git rm --cached` on main → merge resolves as delete. memory/ is gitignored so the keyset writes (made on main checkout, never committed) were clobbered. Recovered via `git show HEAD^1:memory/<f>.md` (old base) + re-append. LESSON: memory/ writes live only in the working tree of whatever branch is checked out; switching/merging branches can wipe them. Commit-free memory + branch ops = data loss risk.

## Bench disproved "count(*) was S1's killer" — it's the OFFSET scan (2026-06-17)
Re-ran rustapi bench after keyset merge. S1 (random deep OFFSET, rndPage(3999)) with `withCount=false` was STILL ~39s p95 (fixed) / ~42s (ramp) — NO improvement vs old 22s, actually worse (machine/Docker variance). So dropping count did NOT fix deep random-offset. Earlier diagnosis ("per-page count(*) was the dominant cost, Strapi skips it") was WRONG for S1 — the deep OFFSET scan+discard (~100k rows/request) is the real cost; count was secondary. Keyset is the ONLY fix for deep paging: S4 keyset sequential walk = 1201ms p95 fixed vs S1 39049ms = ~32x faster, flat at depth. Lesson: random-jump-to-deep-page (S1) is unfixable cheaply (no system solves it — Strapi's old 788ms was suspicious, likely its own count-free default + warm cache + smaller effective scan, NOT a magic offset fix); sequential deep paging (S4) is what keyset fixes. withCount=false still helps normal list throughput (skips full count) but not the offset cliff. Strapi side NOT re-run this round (rustapi-only); S1-vs-Strapi comparison still from old report.
