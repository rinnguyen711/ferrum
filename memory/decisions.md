# Decisions

Why we chose X over Y.

## Content list filtering: server-side, not client-side (2026-06-12)
- Mockup (`design/rustapi/content.jsx`) filters client-side; real app sends Strapi-style
  `filters[field][$op]=value` and refetches.
- Why: backend parser (`crates/http/src/filter.rs`) already complete + tested; client-side wrong
  past one page. User confirmed.
- Uniform 300ms debounce on the whole serialized pair list (JSON-string dep) instead of
  per-editor-type immediate/debounced split — simpler, selects feel instant anyway.

## Builder save: floating SaveBar, not panel button (2026-06-12)
- Deviates from design mockup (panel-header Save) at user request ("panel Save feels odd").
- Why: panel button was cramped, disabled-grey most of the time (violates DESIGN.md "one point of
  energy"), far from edited content; component editor had its own second Save → two primaries.
- Chosen: single draft context (`BuilderDraft = Draft | ComponentDraft`) + sticky bottom bar that
  exists only when dirty (Shopify/GitHub-settings pattern). Alternatives rejected: dirty-aware
  header Save (user passed), hide-panel-button-on-component-routes (kept inconsistency).
- Component dirty check: compare both sides through same draft→wire transform
  (`componentToUpdate(seedFromComponent(snapshot))`) — round-trip stable, no false positives.

## Status tab counts: active tab only (2026-06-12)
- With server pagination, per-tab client counts are wrong past one page; counting server-side
  costs one query per tab per refetch. Dropped inactive counts (YAGNI) — active tab shows
  `meta.total`.

## No UI unit-test infra (standing)
- `ui/` has no vitest/jest; scripts are dev/build/typecheck only. Verification = typecheck +
  manual browser. Pure logic (e.g. `serializeFilters`, `opsFor`) kept exported for future tests.
  Adding a test runner deliberately deferred.

## GraphQL surface (2026-06-13)
- **Runtime dynamic schema, not macro/codegen.** Content types are runtime data; used async-graphql's
  `dynamic` API to build the schema from SchemaRegistry at runtime, mirroring how openapi/schema.rs builds
  the OpenAPI spec. Codegen/macro types rejected (would need rebuild per content-type change).
- **Rebuild on registry change, cached in RwLock** (`GqlRegistry`, mirrors RoleRegistry) — vs rebuild-per-request
  or build-once-at-boot. Hooked into the http schema *routes* (create/patch/delete), not the schema crate
  (keeps crate boundaries). Rebuild is non-fatal: logs + keeps old schema on error.
- **Resolvers reuse REST internals, not a refork.** Extracted shared content CRUD fns from routes/content.rs;
  both surfaces call them → identical authz (`authz.can`), write-hooks, events, validation. No GraphQL authz bypass.
- **AppState injected per-request, NOT baked into cached Schema.** AppState owns GqlRegistry → baking a clone in
  would be cyclic/stale. Handler does `request.data(state).data(principal)`; resolvers read via `ctx.data`.
- **Relation/media fields = scalar UUID id(s) in v1, not nested objects.** Object-ref typing produced dangling
  type refs when a relation targets a Single type (Singles not surfaced as GraphQL objects) → `Schema::finish()`
  fails → boot crash + silent CRUD schema-freeze. Scalar-id typing always builds + matches the deferred
  populate=None limitation honestly. Nested-object population deferred (would need union/conditional typing).
- **async-graphql-axum pinned =7.0.13.** >=7.0.14 jumped to axum 0.8; workspace is axum 0.7. Bridge until an
  axum 0.7→0.8 workspace migration. async-graphql core stays 7.2.1.
- **filters arg = JSON scalar round-tripped to the REST filter string** (`filters_to_raw_query`), reusing the
  battle-tested `filter::parse` instead of forking a structured entry point. Values percent-encoded (parse decodes).

## Custom roles (2026-06-12)
- **Permissions stored per-(content_type, verb), enforced via coarse Action.** Verbs (find/findOne/create/
  update/delete/publish) collapse onto existing `Action` enum so no content route's authz was rewritten — the
  single choke point `Authz::can(principal, action, content_type)` already threaded `content_type` (was unused).
  `publish`→`ContentWrite` (no separate publish gate exists).
- **In-memory RoleRegistry cache, reload on CRUD** (vs per-request DB query) — mirrors SchemaRegistry pattern,
  keeps authz off hot DB path, honors design's "no per-request lookups" note.
- **System roles (admin/editor/viewer) fully locked** — seeded is_system, keep hardcoded `role_allows`
  short-circuit so behavior is identical until a custom role is edited; can't edit/delete via API/UI.
- **User detail: persist only confirmed + blocked** (user's explicit minimal-scope pick). Dropped
  full_name/username (would be fake — no columns). Provider/2FA/LastActive shown static (no auth infra).
