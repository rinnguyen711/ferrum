# Auto-generated OpenAPI docs for the public API

**Date:** 2026-06-04
**Status:** Approved design, pre-implementation

## Goal

Give external API clients an accurate, browsable reference for the HTTP
API. The spec is generated **at request time** from the live content-type
registry, so the dynamic `/api/:type` endpoints document the *real* fields
of each content type and never go stale. Served publicly by default, with a
config flag to disable it in production.

## Why runtime generation (not a build-time file)

The content surface is dynamic: `/api/:type` and `/api/:type/:id` take their
shape from content types defined at runtime via `/admin/content-types`. A
static, checked-in `openapi.json` would drift the moment a content type is
created or patched. Generating on each request from
`state.schemas.registry().list()` keeps the spec correct by construction.

This mirrors the **Strapi v4 documentation plugin** (runtime scan of
content-types + routes → OpenAPI), rather than the Strapi v5
`strapi openapi generate` CLI (build-time static file). We deliberately drop
Strapi's plugin-override machinery (`registerOverride`,
`mutateDocumentation`, `excludeFromGeneration`) as YAGNI — this project has
no plugin ecosystem.

## Scope

In scope:
- `GET /openapi.json` — OpenAPI 3.1 document built live from `AppState`.
- `GET /docs` — Swagger UI page (CDN-loaded) pointed at `/openapi.json`.
- Config flag to enable/disable both endpoints.

Out of scope (explicitly deferred):
- Override/mutate hooks (no plugins).
- Per-content-type "exclude from docs" opt-out (revisit if needed).
- Codegen from handler annotations (utoipa). Static routes are hand-written.
- A persisted/checked-in spec artifact.

## Architecture

New module `crates/http/src/openapi/`:

```
openapi/
  mod.rs     # axum handlers + router(); the /docs HTML
  spec.rs    # build(&AppState) -> serde_json::Value (the OpenAPI doc)
  schema.rs  # field_to_schema(&Field) + content-type -> components
  static_paths.rs  # hand-written paths/components for /admin/*, /auth/*, /healthz
```

### Spec assembly (`spec.rs`)

`pub async fn build(state: &AppState) -> serde_json::Value` produces an
OpenAPI 3.1 document:

- `openapi: "3.1.0"`
- `info`: `{ title, version, description }` — `version` from new config
  `api_version` (env `FERRUM_API_VERSION`, default `"0.1.0"`).
- `servers`: single entry from config `public_base_url` (env
  `FERRUM_PUBLIC_URL`, default `"/"`).
- `components.securitySchemes.bearerAuth`: HTTP bearer / JWT. Applied to all
  protected paths via per-operation `security`.
- `paths` = static block (merged from `static_paths.rs`) + dynamic block
  (generated per content type).
- `components.schemas` = static component schemas + one response schema `T`
  and one request schema `TInput` per content type.

### Dynamic paths + schemas (`schema.rs`)

For each `ContentType` from `state.schemas.registry().list().await`:

**Component schemas** (Strapi-style split of response vs request):
- `T` (response): system columns `id` (uuid), `created_at`,
  `updated_at` (date-time) + every stored field mapped via
  `field_to_schema`. `required` lists `id`, timestamps, and every field with
  `required = true`.
- `TInput` (request body for POST/PUT): the same user fields **without**
  system columns; `required` lists fields with `required = true`. Read-only
  system fields never appear in request bodies.

**Paths:**
- `GET /api/{name}` — list. Query params: `page`, `pageSize` (integer),
  `sort` (string), `populate` (string). Response
  `{ data: [T], meta: { page, pageSize, total } }`.
- `POST /api/{name}` — create. Request body `TInput`. Response `T` (201).
- `GET /api/{name}/{id}` — fetch one (with optional `populate`). Response `T`.
- `PUT /api/{name}/{id}` — replace. Body `TInput`. Response `T`.
- `DELETE /api/{name}/{id}` — 204.

All `/api/*` operations carry `security: [{ bearerAuth: [] }]` and a shared
`401`/`403`/`404` response set referencing a static `Error` component.

### `field_to_schema(&Field) -> serde_json::Value`

The core type mapping. Source of truth is `FieldKind` in
`crates/core/src/field.rs`.

| FieldKind | OpenAPI |
|-----------|---------|
| `String`, `Text` | `{type: string}` (+ `maxLength` from `effective_max_length`) |
| `Integer` | `{type: integer, format: int64}` |
| `Float` | `{type: number, format: double}` |
| `Boolean` | `{type: boolean}` |
| `Datetime` | `{type: string, format: date-time}` |
| `Uuid` | `{type: string, format: uuid}` |
| `Email` | `{type: string, format: email}` |
| `Url` | `{type: string, format: uri}` |
| `Slug` | `{type: string, pattern: <slug regex>}` |
| `Enum` | `{type: string, enum: [values]}` from `enum_meta()` |
| `Json` | `{}` (any JSON value) |
| `Relation` | many_to_one/one_to_one → `{type: string, format: uuid}`; many_to_many → `{type: array, items: {…uuid}}` |
| `Media` | single → `{type: string, format: uuid}`; multiple → `{type: array, items: {…uuid}}` |

`default` (when non-null) is emitted as the schema `default`. `FieldKind` is
`#[non_exhaustive]`, so the match has a catch-all emitting `{}` (any) to stay
forward-compatible if a kind is added before the mapping is updated.

### Static paths (`static_paths.rs`)

A hand-written `serde_json::Value` block covering the stable routes:
`/healthz`, `/auth/setup`, `/auth/login`, `/auth/me`,
`/admin/content-types*`, `/admin/users*`, `/admin/media/*`. Includes a
shared `Error` component schema. These handlers are fixed-shape; the literal
is the single place to update if one changes. (Accepted drift risk — chosen
over utoipa annotations to avoid a new dep and per-handler churn.)

### Handlers + router (`mod.rs`)

- `async fn openapi_json(State(state)) -> Json<Value>` → `spec::build(&state)`.
- `async fn docs_ui() -> Html<&'static str>` → a self-contained Swagger UI
  page loading the bundle from a CDN and fetching `/openapi.json`.
- `pub fn router() -> Router<AppState>` → both routes, **no auth layer**.

### Wiring (`routes/mod.rs`)

Docs routes are **public** (no `require_auth`), gated by config:

```rust
let mut public = Router::new()
    .route("/healthz", get(health::healthz))
    .merge(auth::public_router());
if state.config.docs_enabled {
    public = public.merge(openapi::router());
}
```

New config field `docs_enabled: bool` on `AppConfig` (env
`FERRUM_DOCS_ENABLED`, default `true`; set `false`/`0` to disable in prod).
Also new: `api_version: String`, `public_base_url: String`.

## Security note

The spec exposes every content type's name and field structure. Because docs
default to **public**, deployments that treat their schema as sensitive must
set `FERRUM_DOCS_ENABLED=false`. This is documented alongside the env var.
The flag controls registration of the routes themselves — when off, both
`/openapi.json` and `/docs` return 404.

## Error handling

- `spec::build` is pure assembly over already-loaded registry data; it does
  no I/O and cannot fail. Returns `Value` directly.
- Registry read uses the existing async `list()`; an empty registry yields a
  valid spec with only the static paths.
- Unknown/future `FieldKind` → `{}` (permissive) rather than a panic.

## Testing

- **Unit (`schema.rs`):** `field_to_schema` for every `FieldKind`, incl.
  enum values, relation cardinalities, single vs multiple media, defaults,
  maxLength. One table-driven test over all kinds.
- **Unit (`spec.rs`):** build over a registry with a representative content
  type; assert `paths` contains `/api/{name}`, components contain `T` and
  `TInput`, `TInput` omits `id`/timestamps, `info.version` reflects config.
- **Integration (`crates/bin/tests`):** boot app, create a content type via
  `/admin/content-types`, then `GET /openapi.json` and assert the new path +
  schema appear (proves runtime freshness). Assert `/docs` returns 200 HTML.
- **Integration toggle:** with `docs_enabled = false`, assert `/openapi.json`
  and `/docs` return 404.
- **Validity:** parse the generated JSON and assert it is a structurally
  well-formed OpenAPI 3.1 doc (required top-level keys present).

## References

- Strapi v4 documentation plugin (runtime scan model): https://docs.strapi.io/cms/plugins/documentation
- Strapi v5 OpenAPI CLI (build-time model, not chosen): https://docs.strapi.io/cms/api/openapi
