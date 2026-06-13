# GraphQL Surface Alongside REST

Date: 2026-06-12
Status: design — approved, pending spec review

## Goal

A GraphQL endpoint that is a **second front door to the same engine** — same
runtime content-type registry, same read/write internals, same authz choke
point. Full CRUD parity with the REST surface. Not a re-fork of the content
handler.

## Context

rustapi exposes content over REST today: per content type list (with
filter/sort/paginate/populate), get-one, create, put, delete, plus single-types
and JWT/token auth. Content types are **defined at runtime** — created and
edited through the admin UI, stored in `_content_types`, cached in
`SchemaRegistry`. The REST routes and the OpenAPI spec are both generated from
that registry at runtime (`crates/http/src/openapi/schema.rs` walks the
registry and emits a spec).

GraphQL must follow the same runtime-schema model: the GraphQL schema is built
from the content-type registry and rebuilt when the registry changes — no
restart, no codegen.

## Core tension and resolution

GraphQL libraries are usually macro/compile-time (types known at build). Our
content types are not known until runtime. Resolution: use **async-graphql's
`dynamic` schema API** (`async_graphql::dynamic::{Schema, Object, Field,
TypeRef, ...}`), building the schema at runtime from `SchemaRegistry`, mirroring
how `openapi/schema.rs` builds the OpenAPI spec from the same source.

## Crate boundaries

All GraphQL code lives in the **`http` crate only**. `core`, `sql`, and
`schema` stay GraphQL-unaware — no new edges, boundaries intact. The schema
rebuild is triggered from the http schema *routes* (where role-cache reload is
already triggered), not from inside the `schema` crate.

## Module layout

New module `crates/http/src/graphql/`:

```
graphql/
  mod.rs        # GqlRegistry (RwLock<dynamic::Schema>) + rebuild_from_registry
  build.rs      # ContentType registry → dynamic Schema (Objects, Inputs, Query, Mutation)
  scalars.rs    # FieldKind → GraphQL type mapping (mirrors openapi/schema.rs)
  resolve.rs    # query + mutation resolvers → call shared content internals
  handler.rs    # POST/GET /api/graphql axum handlers + GraphiQL playground
```

Dependencies (workspace): `async-graphql` + `async-graphql-axum`.

## Schema generation (`build.rs` + `scalars.rs`)

Walk `SchemaRegistry.list()` exactly as `openapi/schema.rs` does. For each
`ContentType`:

- an output `Object` named in PascalCase (`Article`) with system fields
  (`id: UUID!`, `created_at: DateTime!`, `updated_at: DateTime!`) plus one
  GraphQL field per content field;
- an `InputObject` (`ArticleInput`) with the writable fields (no id/timestamps),
  required fields non-null;
- a list-envelope object (`ArticleList { data: [Article!]!, meta: Meta! }`) with
  shared `Meta { page, pageSize, total }`.

`FieldKind` → GraphQL type, reusing the same mapping logic as
`openapi/schema.rs::field_to_schema` (keep the two in sync; both are the
single source for the field model):

| FieldKind | GraphQL type |
|---|---|
| String, Text, Slug | `String` |
| Integer | `Int` |
| Float | `Float` |
| Boolean | `Boolean` |
| Datetime | custom scalar `DateTime` |
| Uuid | custom scalar `UUID` |
| Email, Url | `String` |
| Enum | GraphQL enum built from `enum_meta().values` (fallback `String` if empty) |
| Json | custom scalar `JSON` |
| Relation | object ref to target type; list if `ManyToMany`, single otherwise |
| Media | object ref to a `Media` object; list if `multiple`, single otherwise |

`FieldKind` is `#[non_exhaustive]` — unmatched kinds map to the `JSON` scalar
(permissive, matches OpenAPI's `{}` fallback). Required field → non-null
(`!`). Defaults are not expressed in the GraphQL type (GraphQL input defaults
differ from our server-applied defaults; the write path applies defaults as
today).

## Schema sync (`GqlRegistry`)

```rust
pub struct GqlRegistry { inner: Arc<RwLock<dynamic::Schema>> }

impl GqlRegistry {
    pub async fn rebuild_from_registry(&self, reg: &SchemaRegistry, ...) -> Result<...>;
    pub async fn current(&self) -> dynamic::Schema; // cheap clone for execute
}
```

Mirrors `RoleRegistry` (RwLock cache, rebuilt on mutation). New
`AppState.gql: GqlRegistry` field.

Rebuild trigger points (all in `crates/http/src/routes/schema.rs`, immediately
after the existing `SchemaService` mutation succeeds — the same pattern
`routes/roles.rs` uses for `reload_from_db`):

- `create` content type → rebuild
- `patch_one` content type → rebuild
- `delete_one` content type → rebuild

Built once at boot in `crates/bin/src/main.rs` after the schema registry is
hydrated.

Result: a content type created in the admin UI appears in the GraphQL schema
(and introspection/playground) without a restart, same as REST/OpenAPI.

## Resolvers (`resolve.rs`) — reuse, do not re-fork

Generated per content type:

- Query `articles(filters, sort, page, pageSize): ArticleList!`
- Query `article(id: UUID!): Article`
- Mutation `createArticle(data: ArticleInput!): Article!`
- Mutation `updateArticle(id: UUID!, data: ArticleInput!): Article!`
- Mutation `deleteArticle(id: UUID!): Boolean!`

Each resolver calls the **same internal functions the REST handlers call** for
list/get/create/update/delete. Where that logic currently lives inline in
`crates/http/src/routes/content.rs`, extract it into plain functions that both
surfaces call; the REST handler becomes a thin wrapper over the same function
(REST behavior unchanged — verified by the existing content/integration tests).

- GraphQL `filters`/`sort`/`page`/`pageSize` args translate into the existing
  `filter.rs` / `query.rs` structures before hitting the shared list function.
- Nested relation/media selections resolve via the existing `populate.rs` join
  logic. Resolved per parent in v1 (see N+1 note).
- Mutations go through the same write path, so **write-hooks
  (`before_write`/`after_write`) and `EventSink` fire identically** to REST.

## Auth and authz

`/api/graphql` is mounted in the **protected router, behind the existing
`require_auth` middleware**. The `Principal` is extracted by the same
middleware and injected into the async-graphql request context. Every resolver
calls the same choke point —
`state.authz.can(principal, action, content_type)` — before touching data:

- list/get → `ContentRead`
- create/update → `ContentWrite`
- delete → `ContentDelete`

No new authz logic. Token scopes and custom-role per-type permissions apply
unchanged. A denied resolver returns a GraphQL error with
`extensions.code = "FORBIDDEN"` (the query may partially succeed for other
allowed fields, per GraphQL semantics).

## Endpoint gating

- `POST /api/graphql` — always available (the GraphQL execution endpoint).
- `GET /api/graphql` — serves the GraphiQL playground **only when
  `config.docs_enabled`**, mirroring how the OpenAPI docs router is gated in
  `routes/mod.rs`.

## Error handling

Map `core::Error` variants onto GraphQL errors with a stable
`extensions.code`, parallel to REST status codes:

| core::Error | extensions.code |
|---|---|
| NotFound | `NOT_FOUND` |
| Validation | `BAD_USER_INPUT` |
| Forbidden | `FORBIDDEN` |
| Unauthorized | `UNAUTHORIZED` |
| (other/internal) | `INTERNAL` |

async-graphql aggregates these into the response `errors[]` array; partial
data is returned for fields that did resolve.

## Testing

Integration tests in `crates/bin/tests/graphql.rs` (testcontainers Postgres,
like the other suites):

- list query with filter + sort + pagination → correct data + meta
- nested relation and media selection resolve
- get-one by id (found + not-found → `NOT_FOUND`)
- each mutation: create, update, delete (and effect visible via subsequent query)
- write-hook and event fire on mutation (same assertions style as REST write tests)
- authz denial: token scope mismatch and custom-role denial → `FORBIDDEN`
- schema reflects a content type created mid-test without restart
- error-code mapping for each variant

Unit tests on `scalars.rs` field-kind → GraphQL type mapping, mirroring the
`openapi/schema.rs` test block (one assertion per kind, enum values, relation
single-vs-list, media single-vs-multiple, non-null on required).

## Explicit non-goals (v1)

- **DataLoader / N+1 batching.** v1 resolves nested relations per parent via the
  existing populate path. Acceptable at small/medium scale. Batched loaders are
  a follow-up, not v1.
- **Subscriptions.** Out of scope.
- **GraphQL-specific filtering DSL.** We reuse the existing Strapi-style filter
  model exposed as input args; no new operator surface.
- **Single-types as GraphQL root fields.** v1 covers collection content types;
  single-types can follow the same pattern in a later pass.
- **Field-level authz / row-level rules.** Same type-level authz as REST today
  (that is roadmap item #3, separate work).

## Touch points summary

- `Cargo.toml` (workspace) — add `async-graphql`, `async-graphql-axum`.
- `crates/http/Cargo.toml` — add the two deps.
- `crates/http/src/graphql/` — new module (5 files above).
- `crates/http/src/state.rs` — `AppState.gql: GqlRegistry` field.
- `crates/http/src/routes/schema.rs` — rebuild GQL schema on create/patch/delete.
- `crates/http/src/routes/mod.rs` — mount `/api/graphql` in protected router;
  playground GET gated by `docs_enabled`.
- `crates/http/src/routes/content.rs` — extract shared list/get/CRUD functions
  (REST handlers become thin wrappers).
- `crates/bin/src/main.rs` — build GQL schema once at boot after registry hydrate.
- `crates/bin/tests/graphql.rs` — integration suite.
