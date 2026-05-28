# rustapi — Core Headless CMS Framework (v1 Design)

**Date:** 2026-05-28
**Status:** Approved for implementation planning
**Scope:** v1 — core headless CMS API in Rust. Strapi-like dynamic content modeling, but server-only (no admin UI).

---

## 1. Goals & Non-Goals

### Goals (v1)

- Define content types at runtime via HTTP API; persist to Postgres as real tables (table-per-type).
- Auto-generate CRUD REST endpoints per content type.
- Support primitive field types: `string`, `text`, `integer`, `float`, `boolean`, `datetime`.
- Single static admin API key for access control.
- Pagination + sort on list endpoints (no filter operators yet).
- Architecture leaves clean seams for the post-v1 roadmap (relations, RBAC, drafts, i18n, webhooks, admin UI, plugins, GraphQL, multi-tenancy).

### Non-Goals (v1)

- No admin UI.
- No GraphQL.
- No relations, JSON, enum, media, or i18n fields.
- No user-level auth / RBAC.
- No filter operators on list endpoints.
- No drafts/publishing.
- No webhooks, plugins, OpenAPI/Swagger autogen, metrics endpoint.

---

## 2. Architecture

Single binary, layered components, organized as a Cargo workspace of focused crates with one-way dependencies.

```
HTTP (axum + tower)
  └─ Auth middleware (API key check → Principal)
  └─ Routes
       /admin/content-types        (schema CRUD)
       /api/:type                  (entry list, create)
       /api/:type/:id              (entry get, update, delete)
       /healthz
  └─ Handlers
       └─ SchemaRegistry  (Arc<RwLock<HashMap<String, ContentType>>>)
       └─ SqlBuilder      (pure functions → (sql, bind plan))
       └─ Authz trait     (AlwaysAllow in v1)
       └─ EventSink trait (NoopSink in v1)
       └─ sqlx::PgPool
            └─ Postgres
                 ├─ _content_types     (metadata)
                 └─ ct_<type_name>     (one per content type)
```

### 2.1 Crate Layout

- `rustapi-core` — domain types: `ContentType`, `Field`, `FieldKind`, `Principal`, `Action`, `Event`, `Error`. No I/O.
- `rustapi-sql` — SQL builder: DDL and DML. Pure functions returning `(String, Vec<BoundValue>)`. No sqlx dependency. Owns identifier validation and the `table_name(&str) -> String` choke point.
- `rustapi-schema` — `SchemaRegistry`, schema CRUD use cases, DDL migration execution. Depends on `core`, `sql`, `sqlx`.
- `rustapi-http` — axum app, handlers, middleware, DTOs, error → response mapping. Depends on `core`, `schema`, `sql`.
- `rustapi-bin` — `main`, config loading, wiring, integration tests.

Dependency rule (enforced by Cargo): each crate only depends on those lower in the list. `sql` must not depend on `sqlx`, so it stays unit-testable without a database.

### 2.2 Extensibility Seams (baked in for v1)

These are present in v1 but trivially used; phases 2+ extend them without invasive refactor:

1. **`FieldKind` enum is `#[non_exhaustive]`** — new kinds (relation, enum, json, media) added without breaking match arms in dependents.
2. **`Field.kind_meta: serde_json::Value`** — empty `{}` for primitives; phase 2 uses for relation target, enum values, etc.
3. **`Filter` enum in `rustapi-sql`** — only `Filter::None` variant in v1. `SqlBuilder::select` already takes `&Filter`. Phase 2 adds operator variants.
4. **`Principal` extractor** — middleware always inserts `Principal::Admin` into request extensions in v1. Handlers read principal, never the raw key. Phase 3 produces other variants from JWT.
5. **`Authz` trait + `AlwaysAllow` impl** — wired through `AppState`. Phase 3 swaps in `RbacAuthz`.
6. **System columns as `Vec<SystemColumn>` constant** — `id`, `created_at`, `updated_at` listed in code, not hardcoded in SQL strings. Phase 4 adds `published_at`, `status`.
7. **`Event` enum + `EventSink` trait + `NoopSink` impl** — every handler emits domain events (`EntryCreated`, `EntryUpdated`, `EntryDeleted`, `SchemaCreated`, `SchemaUpdated`, `SchemaDeleted`). Phase 4 plugs in `WebhookSink`.
8. **`SchemaRegistry::reload_from_db(&pool)`** public method — used at boot in v1. Phase 7 calls it from a `LISTEN`/`NOTIFY` task.
9. **`table_name(ct: &str) -> String`** — single function all per-type table references go through. Phase 7 (multi-tenant) changes this one function to namespace by tenant.

---

## 3. Data Model

### 3.1 Internal metadata table: `_content_types`

| Column        | Type           | Notes                                                |
|---------------|----------------|------------------------------------------------------|
| `id`          | `UUID PK`      | `gen_random_uuid()` default                          |
| `name`        | `TEXT UNIQUE`  | snake_case, validated `^[a-z][a-z0-9_]{0,62}$`       |
| `display_name`| `TEXT`         | human-readable label                                 |
| `fields`      | `JSONB`        | array of field defs (see 3.2)                        |
| `created_at`  | `TIMESTAMPTZ`  | `now()` default                                      |
| `updated_at`  | `TIMESTAMPTZ`  | `now()` default; bumped on PATCH                     |

Created by internal migration at boot. Never touched directly by user routes.

### 3.2 Field definition (JSON)

```json
{
  "name": "title",
  "kind": "string",
  "required": true,
  "unique": false,
  "default": null,
  "max_length": 255,
  "kind_meta": {}
}
```

- `name`: `^[a-z][a-z0-9_]{0,62}$`. Must not collide with reserved names.
- `kind`: one of `string | text | integer | float | boolean | datetime` (v1).
- `required`: bool. Maps to `NOT NULL`.
- `unique`: bool. Maps to `UNIQUE` constraint.
- `default`: JSON value or null. Coerced per kind; emitted as SQL `DEFAULT`.
- `max_length`: only meaningful for `string`. Default 255. Must be in `[1, 10_000]`; out-of-range values rejected with 422. For unbounded strings use `text`.
- `kind_meta`: reserved JSONB for future kinds. Must be `{}` in v1; rejected otherwise.

**Reserved field names**: `id`, `created_at`, `updated_at`, plus a denylist of SQL keywords (`select`, `from`, `where`, `table`, `order`, `group`, `having`, `user`, `null`, `true`, `false`, `default`, `primary`, `foreign`, `index`).

### 3.3 Per-type table: `ct_<name>`

System columns always present:

- `id` `UUID` PK, default `gen_random_uuid()`
- `created_at` `TIMESTAMPTZ NOT NULL DEFAULT now()`
- `updated_at` `TIMESTAMPTZ NOT NULL DEFAULT now()` (bumped by handler on update)

User columns mapped from `FieldKind`:

| kind       | SQL type                       |
|------------|--------------------------------|
| `string`   | `VARCHAR(max_length)`          |
| `text`     | `TEXT`                         |
| `integer`  | `BIGINT`                       |
| `float`    | `DOUBLE PRECISION`             |
| `boolean`  | `BOOLEAN`                      |
| `datetime` | `TIMESTAMPTZ`                  |

Constraints applied: `NOT NULL` if `required`, `UNIQUE` if `unique`, `DEFAULT <val>` if `default` set (literal-encoded per kind).

### 3.4 Naming + safety

- Table name: `ct_<name>` (returned by `table_name(&str) -> String`).
- Column name: field name verbatim (already validated by regex).
- All identifiers passed through `quote_ident(s) -> String` which re-validates against the regex and double-quotes the result. Any `format!` of identifiers outside `quote_ident` is a bug.
- All values bound via `$N` parameters; never string-interpolated.

---

## 4. HTTP API

All routes require header `X-Api-Key: <admin_key>` in v1. Missing/wrong → 401 `unauthorized`.

### 4.1 Schema management

| Method | Path                            | Body                                       | Returns                |
|--------|---------------------------------|--------------------------------------------|------------------------|
| GET    | `/admin/content-types`          | —                                          | `[ContentType]`        |
| GET    | `/admin/content-types/:name`    | —                                          | `ContentType`          |
| POST   | `/admin/content-types`          | `{name, display_name, fields[]}`           | 201 `ContentType`      |
| PATCH  | `/admin/content-types/:name`    | `{display_name?, add_fields?, drop_fields?}` | `ContentType`         |
| DELETE | `/admin/content-types/:name?confirm=true` | —                                | 204                    |

PATCH semantics (v1):

- `add_fields`: append-only. If table has rows and field is `required` with no `default`, request rejected with 422.
- `drop_fields`: list of field names. Drops the column (cascading).
- Rename is unsupported in v1 (do drop + add).
- Changing a field's `kind` is unsupported in v1.

DELETE: requires `?confirm=true`. Drops `ct_<name>` table + metadata row. Hard delete; no soft delete in v1.

### 4.2 Content CRUD

| Method | Path                | Body                              | Returns                                            |
|--------|---------------------|-----------------------------------|----------------------------------------------------|
| GET    | `/api/:type`        | —                                 | `{data: [entry...], meta: {page, pageSize, total}}`|
| GET    | `/api/:type/:id`    | —                                 | entry                                              |
| POST   | `/api/:type`        | entry fields (flat object)        | 201 entry                                          |
| PUT    | `/api/:type/:id`    | entry fields (full replace)       | entry                                              |
| DELETE | `/api/:type/:id`    | —                                 | 204                                                |

List query params:

- `page` — 1-indexed, default 1, min 1.
- `pageSize` — default 25, max `RUSTAPI_PAGE_SIZE_MAX` (default 100).
- `sort` — `field:asc` or `field:desc`. Field must be a defined user field or `id|created_at|updated_at`. Else 422 `validation_failed`.

Entry JSON: flat object, keys = field names, values = JSON primitives. System columns (`id`, `created_at`, `updated_at`) included on responses. On POST/PUT request bodies, system columns are ignored if present (not an error) — the server controls them.

### 4.3 Misc

- `GET /healthz` — 200 if DB ping succeeds; 503 otherwise.

### 4.4 Errors

JSON shape:

```json
{ "error": { "code": "validation_failed", "message": "...", "details": { ... } } }
```

Codes: `unauthorized`, `not_found`, `validation_failed`, `conflict`, `unsupported`, `internal`.

| Error variant            | HTTP status |
|--------------------------|-------------|
| `unauthorized`           | 401         |
| `not_found`              | 404         |
| `validation_failed`      | 422         |
| `conflict`               | 409         |
| `unsupported`            | 400         |
| `internal`               | 500         |

DB error mapping in handler: unique violation → `conflict`; check/not-null/foreign-key violations → `validation_failed`; everything else → `internal` (logged with full chain).

---

## 5. Schema Engine + DDL Flow

### 5.1 Boot sequence

1. Load config from env. Refuse to boot on missing required vars.
2. Connect `sqlx::PgPool`.
3. Run internal migrations (`sqlx::migrate!()`) → ensures `_content_types` exists with current shape.
4. `SchemaRegistry::reload_from_db(&pool)` → populate `Arc<RwLock<HashMap<String, ContentType>>>`.
5. Build axum router with `AppState { registry, pool, config, authz, events }`.
6. Bind, serve.

### 5.2 Create content type

1. Validate payload: name regex, display_name non-empty, ≥1 user field, no duplicate field names, no reserved names, each field self-valid, `kind_meta == {}`.
2. Acquire `registry.write()` lock.
3. Begin DB transaction.
4. `INSERT INTO _content_types (id, name, display_name, fields) VALUES (...)`.
5. Execute `CREATE TABLE ct_<name> (...)` built by `rustapi-sql::ddl::create_table(&ct)`.
6. Commit. On any error: rollback, release lock, return mapped error.
7. Insert into in-memory registry map.
8. Emit `Event::SchemaCreated { name }`.
9. Return 201.

### 5.3 Patch content type

- Validate add_fields/drop_fields against current schema.
- Acquire write lock. Begin TX.
- For each `add_field`: `ALTER TABLE ct_<name> ADD COLUMN ... [DEFAULT ...] [NOT NULL]`.
- For each `drop_field`: `ALTER TABLE ct_<name> DROP COLUMN ...`.
- Update `_content_types.fields` JSONB + bump `updated_at`.
- Commit. Update in-memory entry. Emit `Event::SchemaUpdated`.

### 5.4 Delete content type

- Confirm via `?confirm=true`; otherwise 422 `validation_failed` with code detail `confirm_required`.
- Write lock, TX, `DROP TABLE ct_<name>`, `DELETE FROM _content_types WHERE name=$1`. Commit.
- Remove from registry. Emit `Event::SchemaDeleted`.

### 5.5 Concurrency

- Schema mutations serialize on `registry.write()`.
- Entry CRUD takes `registry.read()` (cheap) only long enough to clone the `ContentType` it needs, then drops the guard before DB I/O.
- A schema deletion racing with an in-flight entry request will result in the DB query failing (table dropped) → mapped to `not_found` or `internal`. Acceptable for v1.

### 5.6 DDL failure recovery

If Postgres rejects DDL (e.g. adding `NOT NULL` column with no default to a populated table), the transaction rolls back; the registry is never updated; the client receives 422 with the PG error detail surfaced under `error.details.db`.

---

## 6. Content CRUD Flow + Validation

### 6.1 POST `/api/:type`

1. Middleware: API key valid → insert `Principal::Admin` into extensions.
2. Handler reads `:type` → `registry.read().get(type)` → 404 if unknown.
3. `authz.can(&principal, Action::Create, type)` → 403 if denied (v1: always Ok).
4. Parse body as `serde_json::Map`. Strip system columns (`id`, `created_at`, `updated_at`) silently.
5. Validate against `ContentType.fields`:
   - Unknown keys (after strip) → 422.
   - Missing required field with no default → 422.
   - Coerce each value to expected `FieldKind`:
     - `string`/`text`: JSON string; for `string`, len ≤ `max_length`.
     - `integer`: JSON integer (i64 range).
     - `float`: JSON number (accepts integers).
     - `boolean`: JSON bool.
     - `datetime`: JSON string, RFC3339-parsed.
   - Type mismatch → 422 with field name + expected kind.
6. `rustapi-sql::dml::insert(&ct, &values) -> (String, Vec<BoundValue>)`.
7. Execute via `sqlx::query_with(sql, args_from(bounds))` returning the new row.
8. Map row → JSON via `ContentType` (column → kind drives `try_get`).
9. Emit `Event::EntryCreated { type, id }`.
10. 201 with entry JSON.

### 6.2 PUT `/api/:type/:id`

Full replace. Same validation as POST. `UPDATE ct_<name> SET col=$1,... , updated_at=now() WHERE id=$N RETURNING *`. 404 if no rows.

### 6.3 GET list `/api/:type`

- Parse `page`, `pageSize`, `sort`.
- Validate sort field against schema + system columns.
- `SELECT * FROM ct_<name> [WHERE <Filter::None → omitted>] ORDER BY <quoted col> <dir> LIMIT $1 OFFSET $2`.
- `SELECT count(*) FROM ct_<name>` for total. (Two queries; phase-2 optimization may combine.)
- Return `{data: [...], meta: {page, pageSize, total}}`.

### 6.4 GET one / DELETE

Trivial, by id. DELETE returns 204. Both emit corresponding events.

### 6.5 Dynamic binding

`rustapi-sql` exposes a `BoundValue` enum:

```rust
pub enum BoundValue {
    Null,
    Str(String),
    I64(i64),
    F64(f64),
    Bool(bool),
    DateTime(chrono::DateTime<chrono::Utc>),
}
```

`rustapi-http` has a helper `bind_all(query, values) -> Query` that walks the vec and calls `.bind()` per variant. This is the only place `sqlx` knows about `BoundValue`.

### 6.6 Row → JSON

Use `sqlx::postgres::PgRow` with `try_get::<T, _>(column_name)` where `T` is chosen per `FieldKind` from the schema (not from PG type OIDs). System columns handled directly.

---

## 7. Config, Errors, Observability, Testing

### 7.1 Config (env-first)

| Var                      | Required | Default            | Notes                              |
|--------------------------|----------|--------------------|------------------------------------|
| `DATABASE_URL`           | yes      | —                  | Postgres connection string         |
| `RUSTAPI_ADMIN_KEY`      | yes      | —                  | ≥32 chars; refuse boot otherwise   |
| `RUSTAPI_BIND`           | no       | `0.0.0.0:8080`     | listen address                     |
| `RUSTAPI_LOG`            | no       | `info`             | `tracing` filter                   |
| `RUSTAPI_PAGE_SIZE_MAX`  | no       | `100`              | hard cap on `pageSize` query param |

### 7.2 Errors

One `Error` enum in `rustapi-core` using `thiserror`. Variants: `NotFound`, `Validation(ValidationErrors)`, `Conflict(String)`, `Unsupported(String)`, `Db(sqlx::Error)`, `Internal(anyhow::Error)`. `IntoResponse` lives in `rustapi-http` and maps variants to the HTTP shape in §4.4. `Db` is matched on PG error code to refine into `Conflict` or `Validation` where possible.

### 7.3 Observability

- `tracing` + `tracing-subscriber` with JSON formatter.
- `tower-http::TraceLayer` for HTTP spans (method, path, status, latency_ms).
- Custom span at handler entry with `type` and `principal.kind`.
- No metrics endpoint or OpenTelemetry export in v1.

### 7.4 Testing strategy

- **Unit tests** colocated with each crate:
  - `rustapi-sql`: golden-string tests for every DDL/DML builder (no DB).
  - `rustapi-core`: validation tests for `ContentType`, `Field`, `BoundValue` coercion.
- **Integration tests** in `rustapi-bin/tests/`:
  - Postgres via `testcontainers-rs` (or `DATABASE_URL` from env in CI).
  - Each test isolates with `CREATE SCHEMA test_<uuid>; SET search_path TO test_<uuid>` so suites can run in parallel.
  - Coverage targets the full flows: create type → POST entry → GET → PATCH (add field) → POST entry with new field → GET → DELETE entry → DELETE type.
- TDD: write the integration test for each route first, then implement the handler.

---

## 8. Roadmap (post-v1)

### Phase 2 — Richer fields + relations + filters

- New `FieldKind`: `enum`, `json`, `uuid`, `email`, `url`, `slug`, `media_ref` (URL string).
- Relations: one-to-one, one-to-many, many-to-many. Schema declares `kind_meta: {target, kind, inverse?}`. FK columns + join tables auto-managed. `?populate=field1,field2` on GET.
- Filter operators: `$eq`, `$ne`, `$gt`, `$gte`, `$lt`, `$lte`, `$contains`, `$in`, `$null`. Built into the existing `Filter` enum and `select` builder. Whitelisted per field.

### Phase 3 — Auth & RBAC

- Users table, `argon2` password hashes, JWT issuance, refresh tokens.
- Roles: `admin`, `editor`, `public` + custom; per-content-type permission matrix; per-field read/write masks.
- Existing `Principal`/`Authz` seams replace `AlwaysAllow` with `RbacAuthz`. Admin key remains as bootstrap escape hatch (env-gated).

### Phase 4 — Drafts/publishing, i18n, webhooks

- Add system columns `published_at`, `status` (`draft|published`). System-columns list updated; migration adds them to existing tables.
- Locale opt-in per type; per-locale rows linked by `default_locale_id`.
- `WebhookSink` implementation of `EventSink`. Outbound HTTP POSTs with signed payloads.

### Phase 5 — Admin UI

- Separate crate `rustapi-admin` serving SPA assets (or SSR via `askama`/`maud`) under `/admin/ui`.
- Schema designer, entry editor, user/role mgmt, media browser.
- Talks to the existing `/admin/*` API.

### Phase 6 — Plugins, media, GraphQL

- Plugins: Wasm via `wasmtime`, or dynamic Rust crates via `libloading`. Decision deferred. Extend `EventSink` + add a request-time `Hook` trait.
- Media: `Storage` trait (local FS, S3). Image transforms via `image`.
- GraphQL: dynamic schema via `async-graphql` reading from `SchemaRegistry`.

### Phase 7 — Multi-tenancy + horizontal scale

- Tenant isolation via the `table_name` choke point: either schema-per-tenant or prefixed names.
- Read replicas; cross-instance schema sync via `LISTEN`/`NOTIFY` → `SchemaRegistry::reload_from_db()`.
- OpenAPI autogen, OpenTelemetry export, Prometheus metrics.

### Out of scope, permanently

- No-code workflows / Zapier-style automation builders.
- Embedded "CMS as SDK" in user apps — stays a standalone server.

---

## 9. Open Questions

None at sign-off. Implementation plan will surface concrete sub-tasks per crate.
