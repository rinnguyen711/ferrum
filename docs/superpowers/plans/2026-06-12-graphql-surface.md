# GraphQL Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a GraphQL endpoint (`/api/graphql`) with full CRUD parity to REST, built at runtime from the content-type registry, reusing the same read/write internals and authz choke point.

**Architecture:** A new `graphql` module in the `http` crate builds an `async_graphql::dynamic::Schema` at runtime by walking `SchemaRegistry` (same source the OpenAPI spec uses). The schema is cached behind a `RwLock` in `AppState.gql` and rebuilt when content types change, mirroring `RoleRegistry`. Resolvers call shared content functions extracted from `routes/content.rs`, so write-hooks, events, and authz all fire identically to REST. The endpoint sits behind the existing `require_auth` middleware.

**Tech Stack:** Rust, axum, async-graphql + async-graphql-axum (`dynamic-schema` feature), sqlx, testcontainers.

---

## Background the engineer needs

- **Content types are runtime data.** They live in `_content_types`, cached in `SchemaRegistry` (`crates/schema/src/registry.rs`). `state.schemas.registry().list().await` returns `Vec<ContentType>`; `.get(name)` returns `Option<ContentType>`. Each `ContentType` has `name` (snake_case api id), `display_name`, `kind` (`Collection` | `Single`), and `fields: Vec<Field>`.
- **Field model** (`crates/core/src/field.rs`): `Field { name, kind: FieldKind, required, unique, default, max_length, kind_meta }`. `FieldKind` is `#[non_exhaustive]`: `String, Text, Integer, Float, Boolean, Datetime, Uuid, Email, Url, Slug, Enum, Json, Relation, Media`. Helpers: `field.enum_meta()` (`-> Option<{ values: Vec<String> }>`), `field.relation_meta()` (`.cardinality: Cardinality`, `.target: String`), `field.media_meta()` (`.multiple: bool`). `Cardinality` includes `ManyToMany` (list) vs others (single).
- **The OpenAPI generator** (`crates/http/src/openapi/schema.rs`) already maps the exact same field model to a JSON-Schema fragment and PascalCases names (`schema_names`, `to_pascal`). Reuse its mapping decisions; keep the two in sync.
- **REST content handlers** (`crates/http/src/routes/content.rs`): `list`, `create`, `get_one`, `update`, `delete_one`. Today each is an axum handler that does authz (`ensure`), loads the `ContentType`, rejects `Single`, runs the DB + populate + hooks + events pipeline, and wraps the result in `Json`. This plan extracts the non-axum core of each into reusable functions.
- **Authz choke point** (`crates/http/src/state.rs`): `state.authz.can(principal, action, content_type).await -> bool`. Actions: `Action::ContentRead`, `Action::ContentWrite`, `Action::ContentDelete`.
- **Principal** is injected as an axum `Extension<Principal>` by `require_auth` (`crates/http/src/middleware/auth.rs:56`).
- **Errors**: `crates/http/src/error.rs` wraps `ferrum_core::Error` as `ApiError`. `Error` variants: `NotFound`, `Validation(ValidationErrors)`, `Forbidden`, `Unauthorized`, `Conflict`, `RelationFkViolation`, `Internal(anyhow)`, plus others. The shared functions return `Result<_, Error>` (not `ApiError`) so both surfaces can map as they like.
- **Config gating**: `state.config.docs_enabled: bool`. The OpenAPI/Swagger router is merged only when this is true (`crates/http/src/routes/mod.rs`).
- **Test harness**: `crates/bin/tests/common` provides `TestApp::spawn()` (ephemeral Postgres via testcontainers, migrations only — no seed) and `app.admin(req)` (adds admin auth), `app.url(path)`, `app.client`. Content types are created in-test by POSTing to `/admin/content-types` (see `crates/bin/tests/integration_roles.rs`).

### async-graphql dynamic API cheat-sheet

Requires `features = ["dynamic-schema"]`. Key types from `async_graphql::dynamic`:

```rust
use async_graphql::dynamic::{Schema, Object, InputObject, Field, InputValue, TypeRef, FieldFuture, FieldValue, ResolverContext, Enum};
use async_graphql::{Value as GqlValue, Error as GqlError};

// scalar refs: TypeRef::named(TypeRef::STRING), ::named_nn(...) for non-null,
//   ::named_list(...), ::named_nn_list_nn(...) for [T!]!
// object field with resolver:
let f = Field::new("title", TypeRef::named(TypeRef::STRING), |ctx: ResolverContext| {
    FieldFuture::new(async move {
        // return Ok(Some(FieldValue::value(...))) or Ok(None)
        Ok(Some(FieldValue::value(GqlValue::from("hi"))))
    })
});
// Schema::build(query_obj, Some(mutation_obj), None).register(obj).register(input)...finish()?
```

Errors: return `Err(GqlError::new("msg").extend_with(|_, e| e.set("code", "NOT_FOUND")))` from a resolver to populate `errors[].extensions.code`.

axum integration (`async-graphql-axum`): `GraphQLRequest` extractor and `GraphQLResponse` (`impl IntoResponse`). `schema.execute(req.into_inner()).await.into()`.

---

## File Structure

- `Cargo.toml` (workspace) — add `async-graphql`, `async-graphql-axum` to `[workspace.dependencies]`.
- `crates/http/Cargo.toml` — pull both deps.
- `crates/http/src/graphql/mod.rs` — `GqlRegistry` (RwLock schema cache) + module wiring.
- `crates/http/src/graphql/scalars.rs` — `FieldKind` → `TypeRef` mapping + JSON↔GqlValue helpers + custom scalars.
- `crates/http/src/graphql/build.rs` — registry → `dynamic::Schema` (objects, inputs, Query, Mutation).
- `crates/http/src/graphql/resolve.rs` — query + mutation resolver bodies calling shared content fns.
- `crates/http/src/graphql/handler.rs` — axum POST/GET handlers + GraphiQL.
- `crates/http/src/routes/content.rs` — extract shared `list_entries`/`get_entry`/`create_entry`/`update_entry`/`delete_entry` fns; REST handlers become wrappers.
- `crates/http/src/state.rs` — `AppState.gql: GqlRegistry`.
- `crates/http/src/routes/mod.rs` — mount `/api/graphql`; GraphiQL GET gated by `docs_enabled`.
- `crates/http/src/lib.rs` — `pub mod graphql;`.
- `crates/bin/src/main.rs` — build GQL schema at boot + construct `gql` field.
- `crates/bin/tests/graphql.rs` — integration suite.

Other `AppState` construction sites must add the `gql` field — grep `AppState {` across the workspace (per memory note, there are several, incl. tests). Each gets `gql: GqlRegistry::new()`.

---

## Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml` (`[workspace.dependencies]`)
- Modify: `crates/http/Cargo.toml` (`[dependencies]`)

- [ ] **Step 1: Add to workspace dependencies**

In `Cargo.toml` under `[workspace.dependencies]`, add:

```toml
async-graphql = { version = "7", features = ["dynamic-schema", "chrono"] }
async-graphql-axum = "7"
```

(Let cargo resolve the latest 7.x. `chrono` feature gives the built-in DateTime scalar interop; `dynamic-schema` enables `async_graphql::dynamic`.)

- [ ] **Step 2: Pull into the http crate**

In `crates/http/Cargo.toml` under `[dependencies]`, add:

```toml
async-graphql.workspace = true
async-graphql-axum.workspace = true
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p ferrum-http`
Expected: compiles (no usage yet, just the new deps resolve).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/http/Cargo.toml
git commit -m "build(graphql): add async-graphql + async-graphql-axum deps"
```

---

## Task 2: Extract shared content functions from REST handlers

Pull the non-axum core of each REST handler into a function taking plain args and returning `Result<_, Error>`. REST handlers become thin wrappers. This is a refactor with **no behavior change** — the existing content/integration tests are the safety net.

**Files:**
- Modify: `crates/http/src/routes/content.rs`

- [ ] **Step 1: Run the existing content tests to establish a green baseline**

Run: `cargo test -p ferrum-bin --test integration_roles` and any content suite (e.g. `cargo test -p ferrum-bin content`)
Expected: PASS (records the pre-refactor baseline; if Docker-cold, run once more — see memory note on cold-parallel flake).

- [ ] **Step 2: Add a shared `list_entries` function**

In `crates/http/src/routes/content.rs`, add (above `list`). This is the body of the current `list` handler minus axum extraction, returning the JSON envelope as a `Value`. `raw_query` is passed through for the filter parser:

```rust
/// Shared list pipeline used by both the REST handler and GraphQL resolver.
/// `raw_query` is the percent-encoded query string for the Strapi-style
/// filter parser (`filters[..]`); pass "" if none.
pub(crate) async fn list_entries(
    state: &AppState,
    principal: &Principal,
    ct_name: &str,
    params: ListParams,
    populate: Option<&str>,
    raw_query: &str,
) -> Result<Value, Error> {
    if !state.authz.can(principal, Action::ContentRead, ct_name).await {
        return Err(Error::Forbidden);
    }
    let ct = state
        .schemas
        .registry()
        .get(ct_name)
        .await
        .ok_or(Error::NotFound)?;
    if ct.kind == ferrum_core::ContentTypeKind::Single {
        return Err(Error::Validation(ferrum_core::ValidationErrors::single(
            "use /api/single-types/:name for single types",
        )));
    }
    let status = params.status.clone();
    let opts = parse_list(&ct, params, state.config.page_size_max).map_err(|e| e.0)?;
    let offset: i64 = ((opts.page - 1) as i64) * (opts.page_size as i64);
    let filter = crate::filter::parse(raw_query, &ct).map_err(|e| e.0)?;

    let publish = if ct.draft_publish() {
        match status.as_deref() {
            Some("draft") => PublishFilter::Draft,
            Some("all") => PublishFilter::All,
            _ => PublishFilter::Published,
        }
    } else {
        PublishFilter::All
    };

    let (list_sql, list_binds) = ferrum_sql::select_list_status(
        &ct.name, &filter, &opts.sort, opts.page_size as i64, offset, publish,
    )
    .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let q = bind_all(sqlx::query(&list_sql), &list_binds);
    let rows = q.fetch_all(&state.pool).await.map_err(|e| db(e).0)?;

    let mut maps: Vec<Map<String, Value>> = Vec::with_capacity(rows.len());
    for r in &rows {
        match row_to_json(&ct, r).map_err(|e| e.0)? {
            Value::Object(m) => maps.push(m),
            _ => unreachable!("row_to_json returns an object"),
        }
    }
    if let Some(raw) = populate {
        apply_populate(state, &ct, raw, &mut maps).await.map_err(|e| e.0)?;
    }
    crate::media_embed::apply_media_embed(&state.pool, &ct, &mut maps).await?;
    let data: Vec<Value> = maps.into_iter().map(Value::Object).collect();

    let (count_sql, count_binds) = ferrum_sql::count_status(&ct.name, &filter, publish)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let cq = bind_all_as(sqlx::query_as::<_, (i64,)>(&count_sql), &count_binds);
    let total: i64 = cq.fetch_one(&state.pool).await.map_err(|e| db(e).0)?.0;

    Ok(json!({
        "data": data,
        "meta": { "page": opts.page, "pageSize": opts.page_size, "total": total }
    }))
}
```

> Note: `ApiError` is a newtype over `Error` (`ApiError(Error)`); `.0` unwraps it. `parse_list`, `crate::filter::parse`, `apply_populate`, and `row_to_json` currently return `Result<_, ApiError>`, hence the `.map_err(|e| e.0)`. If any of those already return `Error`, drop the `.0`. Verify by reading their signatures before adapting.

- [ ] **Step 3: Rewrite the `list` axum handler as a wrapper**

Replace the body of the existing `list` handler with a call to `list_entries`:

```rust
async fn list(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    Query(params): Query<ListParams>,
    axum::extract::RawQuery(raw_query): axum::extract::RawQuery,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    let populate = params.populate.clone();
    let v = list_entries(
        &state, &principal, &ct_name, params,
        populate.as_deref(), raw_query.as_deref().unwrap_or(""),
    )
    .await
    .map_err(ApiError)?;
    Ok(Json(v))
}
```

- [ ] **Step 4: Repeat the extract-then-wrap pattern for `get_one`, `create`, `update`, `delete_one`**

Add `pub(crate)` shared fns, each = the current handler body minus axum extraction and `Json`/status wrapping, returning `Result<_, Error>`:

```rust
pub(crate) async fn get_entry(
    state: &AppState, principal: &Principal, ct_name: &str, id: Uuid, populate: Option<&str>,
) -> Result<Value, Error>; // returns the entry object; Error::NotFound if absent

pub(crate) async fn create_entry(
    state: &AppState, principal: &Principal, ct_name: &str, body: Map<String, Value>,
) -> Result<Value, Error>; // returns created record (with id)

pub(crate) async fn update_entry(
    state: &AppState, principal: &Principal, ct_name: &str, id: Uuid, body: Map<String, Value>,
) -> Result<Value, Error>; // returns updated record

pub(crate) async fn delete_entry(
    state: &AppState, principal: &Principal, ct_name: &str, id: Uuid,
) -> Result<(), Error>; // Error::NotFound if no row deleted
```

For each: copy the matching handler's body verbatim, change the leading `ensure(&state, &principal, ACTION, &ct_name).await?` to the inline `if !state.authz.can(...).await { return Err(Error::Forbidden); }` form, change `.map_err(ApiError)?` to `.map_err(|e| e.0)?` where the inner call returns `ApiError`, change `.map_err(db)?` to `.map_err(|e| db(e).0)?`, and return the bare value/`()` instead of `Json(...)`/`StatusCode`. Then make each axum handler call its shared fn and re-wrap:

```rust
async fn create(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    Json(body): Json<Map<String, Value>>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let record = create_entry(&state, &principal, &ct_name, body).await.map_err(ApiError)?;
    Ok((StatusCode::CREATED, Json(record)))
}
// get_one -> Ok(Json(get_entry(...)?))
// update  -> Ok(Json(update_entry(...)?))
// delete_one -> { delete_entry(...)?; Ok(StatusCode::NO_CONTENT) }
```

Keep `ensure` if other handlers (publish/unpublish/import/export) still use it; otherwise delete it.

- [ ] **Step 5: Build and run the content tests — behavior must be unchanged**

Run: `cargo build -p ferrum-http && cargo test -p ferrum-bin --test integration_roles`
Expected: PASS, same as the Step 1 baseline. Also `cargo clippy -p ferrum-http --all-targets` clean.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/routes/content.rs
git commit -m "refactor(content): extract shared entry CRUD fns for reuse by GraphQL"
```

---

## Task 3: Field-kind → GraphQL type mapping (`scalars.rs`)

**Files:**
- Create: `crates/http/src/graphql/scalars.rs`
- Test: same file (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Create `crates/http/src/graphql/scalars.rs`:

```rust
//! Maps the content-type field model to async-graphql dynamic TypeRefs,
//! mirroring the decisions in `openapi/schema.rs`. Keep the two in sync.

use async_graphql::dynamic::TypeRef;
use ferrum_core::field::{Cardinality, Field, FieldKind};

/// Custom scalar names registered on the schema.
pub const UUID_SCALAR: &str = "UUID";
pub const DATETIME_SCALAR: &str = "DateTime";
pub const JSON_SCALAR: &str = "JSON";

/// The GraphQL output type for a field (used in output objects).
/// Non-null when the field is required.
pub fn field_type_ref(field: &Field) -> TypeRef {
    let base = base_type_name(field);
    let many = is_list(field);
    match (many, field.required) {
        (true, true) => TypeRef::named_nn_list_nn(base),
        (true, false) => TypeRef::named_nn_list(base),
        (false, true) => TypeRef::named_nn(base),
        (false, false) => TypeRef::named(base),
    }
}

/// Base GraphQL type name for a field's value (before list/non-null wrapping).
/// Relation/Media return the *target object* name (PascalCase) so nested
/// selection works; resolution is wired in build.rs.
pub fn base_type_name(field: &Field) -> String {
    match field.kind {
        FieldKind::String | FieldKind::Text | FieldKind::Slug
        | FieldKind::Email | FieldKind::Url => TypeRef::STRING.to_string(),
        FieldKind::Integer => TypeRef::INT.to_string(),
        FieldKind::Float => TypeRef::FLOAT.to_string(),
        FieldKind::Boolean => TypeRef::BOOLEAN.to_string(),
        FieldKind::Datetime => DATETIME_SCALAR.to_string(),
        FieldKind::Uuid => UUID_SCALAR.to_string(),
        FieldKind::Enum => enum_type_name(field),
        FieldKind::Json => JSON_SCALAR.to_string(),
        FieldKind::Relation => crate::graphql::build::pascal(
            &field.relation_meta().map(|m| m.target).unwrap_or_default(),
        ),
        FieldKind::Media => "Media".to_string(),
        _ => JSON_SCALAR.to_string(),
    }
}

/// True when the field encodes a list (m2m relation or multiple media).
pub fn is_list(field: &Field) -> bool {
    match field.kind {
        FieldKind::Relation => field
            .relation_meta()
            .map(|m| matches!(m.cardinality, Cardinality::ManyToMany))
            .unwrap_or(false),
        FieldKind::Media => field.media_meta().map(|m| m.multiple).unwrap_or(false),
        _ => false,
    }
}

/// Enum GraphQL type name: PascalCase(content_type)+PascalCase(field) is risky
/// for collisions; we name it `<Pascal(field)>Enum` per field. build.rs must
/// register one Enum type per enum field using this same name.
pub fn enum_type_name(field: &Field) -> String {
    format!("{}Enum", crate::graphql::build::pascal(&field.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_core::field::Field;
    use serde_json::{json, Value};

    fn f(kind: FieldKind, required: bool, kind_meta: Value) -> Field {
        Field { name: "x".into(), kind, required, unique: false,
                default: Value::Null, max_length: None, kind_meta }
    }

    #[test]
    fn string_maps_to_string() {
        assert_eq!(base_type_name(&f(FieldKind::String, false, json!({}))), "String");
    }
    #[test]
    fn integer_float_bool() {
        assert_eq!(base_type_name(&f(FieldKind::Integer, false, json!({}))), "Int");
        assert_eq!(base_type_name(&f(FieldKind::Float, false, json!({}))), "Float");
        assert_eq!(base_type_name(&f(FieldKind::Boolean, false, json!({}))), "Boolean");
    }
    #[test]
    fn datetime_uuid_json_scalars() {
        assert_eq!(base_type_name(&f(FieldKind::Datetime, false, json!({}))), "DateTime");
        assert_eq!(base_type_name(&f(FieldKind::Uuid, false, json!({}))), "UUID");
        assert_eq!(base_type_name(&f(FieldKind::Json, false, json!({}))), "JSON");
    }
    #[test]
    fn relation_single_not_list_many_is_list() {
        let one = f(FieldKind::Relation, false, json!({"target":"user","cardinality":"many_to_one"}));
        assert!(!is_list(&one));
        assert_eq!(base_type_name(&one), "User");
        let many = f(FieldKind::Relation, false, json!({"target":"tag","cardinality":"many_to_many"}));
        assert!(is_list(&many));
    }
    #[test]
    fn media_single_vs_multiple() {
        assert!(!is_list(&f(FieldKind::Media, false, json!({"multiple": false}))));
        assert!(is_list(&f(FieldKind::Media, false, json!({"multiple": true}))));
    }
    #[test]
    fn enum_name_is_field_pascal_plus_enum() {
        let mut e = f(FieldKind::Enum, false, json!({"values":["a","b"]}));
        e.name = "status".into();
        assert_eq!(enum_type_name(&e), "StatusEnum");
    }
}
```

- [ ] **Step 2: Add module declarations so it compiles**

In `crates/http/src/lib.rs` add `pub mod graphql;`. Create `crates/http/src/graphql/mod.rs` with `pub mod scalars;` and `pub mod build;` (build.rs created next task — add a minimal stub now so `scalars.rs` references resolve):

`crates/http/src/graphql/mod.rs`:
```rust
pub mod build;
pub mod scalars;
```

`crates/http/src/graphql/build.rs` (stub — just the `pascal` helper scalars.rs needs):
```rust
/// PascalCase a snake_case api id (`blog_post` -> `BlogPost`). Same rule as
/// `openapi::schema::to_pascal`.
pub fn pascal(name: &str) -> String {
    name.split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                Some(first) => first.to_uppercase().chain(c).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p ferrum-http graphql::scalars`
Expected: PASS (6 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/lib.rs crates/http/src/graphql/mod.rs crates/http/src/graphql/scalars.rs crates/http/src/graphql/build.rs
git commit -m "feat(graphql): field-kind to GraphQL TypeRef mapping"
```

---

## Task 4: Build the dynamic schema from the registry (`build.rs`)

Build a `dynamic::Schema` from a `&[ContentType]` slice. Output objects, input objects, enum types, the three custom scalars, a `Media` object, plus a `Query` and `Mutation` root. Resolver bodies are filled in Task 5 — here, resolvers may return `Ok(None)` / unimplemented stubs so the **schema shape** can be unit-tested via introspection.

**Files:**
- Modify: `crates/http/src/graphql/build.rs`
- Test: same file

- [ ] **Step 1: Write the failing test (schema shape via SDL/introspection)**

Append to `crates/http/src/graphql/build.rs` a test that builds a schema from one content type and asserts the SDL contains the expected types. `dynamic::Schema` exposes `.sdl()`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ferrum_core::field::{Field, FieldKind};
    use ferrum_core::{ContentType, ContentTypeKind};
    use serde_json::{json, Value};
    use uuid::Uuid;

    fn article() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "article".into(),
            display_name: "Article".into(),
            fields: vec![
                Field { name: "title".into(), kind: FieldKind::String, required: true,
                        unique: false, default: Value::Null, max_length: None, kind_meta: json!({}) },
                Field { name: "views".into(), kind: FieldKind::Integer, required: false,
                        unique: false, default: Value::Null, max_length: None, kind_meta: json!({}) },
            ],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(), updated_at: Utc::now(),
        }
    }

    #[test]
    fn schema_has_type_query_mutation_for_collection() {
        let schema = build_schema(&[article()]).expect("build");
        let sdl = schema.sdl();
        assert!(sdl.contains("type Article"), "{sdl}");
        assert!(sdl.contains("input ArticleInput"), "{sdl}");
        assert!(sdl.contains("title: String!"), "required -> non-null: {sdl}");
        assert!(sdl.contains("views: Int"), "{sdl}");
        // Query fields: articles + article(id)
        assert!(sdl.contains("articles("), "{sdl}");
        assert!(sdl.contains("article("), "{sdl}");
        // Mutation fields
        assert!(sdl.contains("createArticle("), "{sdl}");
        assert!(sdl.contains("updateArticle("), "{sdl}");
        assert!(sdl.contains("deleteArticle("), "{sdl}");
    }

    #[test]
    fn single_type_is_skipped() {
        let mut s = article();
        s.name = "homepage".into();
        s.kind = ContentTypeKind::Single;
        let schema = build_schema(&[s]).expect("build");
        assert!(!schema.sdl().contains("homepages("), "single types excluded from v1");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p ferrum-http graphql::build`
Expected: FAIL — `build_schema` not defined.

- [ ] **Step 3: Implement `build_schema`**

In `crates/http/src/graphql/build.rs`, above the tests, add the builder. Keep `pascal` from Task 3. This builds the schema shape; resolvers are wired to `resolve.rs` functions in Task 5 — for now, reference them as `crate::graphql::resolve::*` which you will create as stubs in Task 5 Step 1. To keep this task self-contained and testable first, use inline placeholder resolvers that return `Ok(None)` and replace them in Task 5.

```rust
use crate::graphql::scalars::{
    self, base_type_name, field_type_ref, DATETIME_SCALAR, JSON_SCALAR, UUID_SCALAR,
};
use async_graphql::dynamic::{
    Enum, Field, FieldFuture, InputObject, InputValue, Object, Scalar, Schema, SchemaError, TypeRef,
};
use ferrum_core::field::FieldKind;
use ferrum_core::{ContentType, ContentTypeKind};

/// Build the full dynamic schema from the current content-type list.
/// Only Collection types get root fields; Single types are skipped in v1.
pub fn build_schema(types: &[ContentType]) -> Result<Schema, SchemaError> {
    let mut query = Object::new("Query");
    let mut mutation = Object::new("Mutation");

    // shared Meta object for list envelopes
    let meta = Object::new("Meta")
        .field(Field::new("page", TypeRef::named_nn(TypeRef::INT), meta_resolver("page")))
        .field(Field::new("pageSize", TypeRef::named_nn(TypeRef::INT), meta_resolver("pageSize")))
        .field(Field::new("total", TypeRef::named_nn(TypeRef::INT), meta_resolver("total")));

    // shared Media object (id + url, resolved from embedded media json)
    let media = Object::new("Media")
        .field(Field::new("id", TypeRef::named_nn(UUID_SCALAR), json_field_resolver("id")))
        .field(Field::new("url", TypeRef::named(TypeRef::STRING), json_field_resolver("url")));

    let mut schema_builder = Schema::build("Query", Some("Mutation"), None);

    // custom scalars
    schema_builder = schema_builder
        .register(Scalar::new(UUID_SCALAR))
        .register(Scalar::new(DATETIME_SCALAR))
        .register(Scalar::new(JSON_SCALAR))
        .register(meta)
        .register(media);

    let mut enums_registered: std::collections::HashSet<String> = Default::default();

    for ct in types.iter().filter(|c| c.kind == ContentTypeKind::Collection) {
        let type_name = pascal(&ct.name);
        let input_name = format!("{type_name}Input");
        let list_name = format!("{type_name}List");

        // output object
        let mut obj = Object::new(&type_name)
            .field(Field::new("id", TypeRef::named_nn(UUID_SCALAR), json_field_resolver("id")))
            .field(Field::new("created_at", TypeRef::named_nn(DATETIME_SCALAR), json_field_resolver("created_at")))
            .field(Field::new("updated_at", TypeRef::named_nn(DATETIME_SCALAR), json_field_resolver("updated_at")));

        let mut input = InputObject::new(&input_name);

        for field in &ct.fields {
            // register enum type once per distinct enum field name
            if field.kind == FieldKind::Enum {
                let en = scalars::enum_type_name(field);
                if enums_registered.insert(en.clone()) {
                    let mut e = Enum::new(&en);
                    for v in field.enum_meta().map(|m| m.values).unwrap_or_default() {
                        e = e.item(v);
                    }
                    schema_builder = schema_builder.register(e);
                }
            }
            let fname = field.name.clone();
            obj = obj.field(Field::new(
                &field.name,
                field_type_ref(field),
                json_field_resolver(&fname),
            ));
            // input value: same base type; non-null only if required
            let input_tr = if field.required {
                TypeRef::named_nn(base_type_name(field))
            } else {
                TypeRef::named(base_type_name(field))
            };
            input = input.field(InputValue::new(&field.name, input_tr));
        }

        // list envelope
        let envelope = Object::new(&list_name)
            .field(Field::new("data", TypeRef::named_nn_list_nn(&type_name), json_field_resolver("data")))
            .field(Field::new("meta", TypeRef::named_nn("Meta"), json_field_resolver("meta")));

        schema_builder = schema_builder.register(obj).register(input).register(envelope);

        // Query.<plural>(filters, sort, page, pageSize) and Query.<singular>(id)
        let ct_name = ct.name.clone();
        query = query.field(
            Field::new(plural(&ct.name), TypeRef::named_nn(&list_name),
                crate::graphql::resolve::list_field(ct_name.clone()))
                .argument(InputValue::new("page", TypeRef::named(TypeRef::INT)))
                .argument(InputValue::new("pageSize", TypeRef::named(TypeRef::INT)))
                .argument(InputValue::new("sort", TypeRef::named(TypeRef::STRING)))
                .argument(InputValue::new("filters", TypeRef::named(JSON_SCALAR))),
        );
        query = query.field(
            Field::new(ct.name.clone(), TypeRef::named(&type_name),
                crate::graphql::resolve::get_field(ct_name.clone()))
                .argument(InputValue::new("id", TypeRef::named_nn(UUID_SCALAR))),
        );

        // mutations
        mutation = mutation.field(
            Field::new(format!("create{type_name}"), TypeRef::named_nn(&type_name),
                crate::graphql::resolve::create_field(ct_name.clone()))
                .argument(InputValue::new("data", TypeRef::named_nn(&input_name))),
        );
        mutation = mutation.field(
            Field::new(format!("update{type_name}"), TypeRef::named_nn(&type_name),
                crate::graphql::resolve::update_field(ct_name.clone()))
                .argument(InputValue::new("id", TypeRef::named_nn(UUID_SCALAR)))
                .argument(InputValue::new("data", TypeRef::named_nn(&input_name))),
        );
        mutation = mutation.field(
            Field::new(format!("delete{type_name}"), TypeRef::named_nn(TypeRef::BOOLEAN),
                crate::graphql::resolve::delete_field(ct_name.clone()))
                .argument(InputValue::new("id", TypeRef::named_nn(UUID_SCALAR))),
        );
    }

    schema_builder.register(query).register(mutation).finish()
}

/// English-ish plural for the list query name. Keep simple: append "s",
/// "y"->"ies". Collisions are a non-issue at this scale; if a type already
/// ends in "s" we still append (articles, statuses-ish). Document as known.
pub fn plural(name: &str) -> String {
    let p = pascal(name);
    let lower = p[..1].to_lowercase() + &p[1..]; // camelCase root
    if let Some(stem) = lower.strip_suffix('y') {
        format!("{stem}ies")
    } else {
        format!("{lower}s")
    }
}
```

> The `json_field_resolver`, `meta_resolver`, and the `crate::graphql::resolve::*` field-resolver factories are defined in `resolve.rs` (Task 5). To make THIS task compile and pass its SDL tests before Task 5, add temporary stubs at the bottom of `build.rs`:
>
> ```rust
> // TEMP stubs — replaced by resolve.rs wiring in Task 5.
> fn json_field_resolver(key: &str) -> impl Fn(async_graphql::dynamic::ResolverContext) -> FieldFuture + Clone {
>     let key = key.to_string();
>     move |_| { let _ = &key; FieldFuture::new(async { Ok(None::<async_graphql::dynamic::FieldValue>) }) }
> }
> fn meta_resolver(key: &str) -> impl Fn(async_graphql::dynamic::ResolverContext) -> FieldFuture + Clone {
>     json_field_resolver(key)
> }
> ```
>
> And add matching stub factories in `resolve.rs` Step 1 of Task 5. SDL generation does not invoke resolvers, so `.sdl()` tests pass with stubs.

- [ ] **Step 4: Run to verify the shape tests pass**

Run: `cargo test -p ferrum-http graphql::build`
Expected: PASS (2 tests). If `.sdl()` differs slightly in spacing, adjust the `contains` assertions to match real output (run once, read the panic's printed SDL).

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/graphql/build.rs
git commit -m "feat(graphql): build dynamic schema (objects, inputs, query, mutation) from registry"
```

---

## Task 5: Resolvers wired to shared content functions (`resolve.rs`)

Replace the stub resolvers with real ones that read the `AppState` + `Principal` from the async-graphql context and call the Task 2 shared functions. Resolvers carry the entry's JSON `Value` down the selection set so child field resolvers read from the parent object.

**Files:**
- Create: `crates/http/src/graphql/resolve.rs`
- Modify: `crates/http/src/graphql/build.rs` (swap stub helpers for `resolve::` ones)
- Modify: `crates/http/src/graphql/mod.rs` (`pub mod resolve;`)

- [ ] **Step 1: Create `resolve.rs` with the resolver factories**

```rust
//! GraphQL resolvers. Each factory captures the content-type name and returns
//! a closure that reads AppState + Principal from the async-graphql context and
//! delegates to the shared content functions in routes::content. The resolved
//! entry/list JSON is stored as the FieldValue so child field resolvers read
//! from the parent object via `parent_value`.

use crate::routes::content;
use crate::state::AppState;
use async_graphql::dynamic::{FieldFuture, FieldValue, ResolverContext};
use async_graphql::{Error as GqlError, Value as GqlValue};
use ferrum_core::{Error, Principal};
use ferrum_schema::query::ListParams; // adjust path: ListParams is defined in crate::query
use serde_json::{Map, Value};
use uuid::Uuid;

/// Map a core::Error to a GraphQL error carrying an extensions.code parallel
/// to the REST status codes.
fn gql_err(e: Error) -> GqlError {
    let code = match &e {
        Error::NotFound => "NOT_FOUND",
        Error::Validation(_) => "BAD_USER_INPUT",
        Error::Forbidden => "FORBIDDEN",
        Error::Unauthorized => "UNAUTHORIZED",
        _ => "INTERNAL",
    };
    GqlError::new(e.to_string()).extend_with(|_, ext| ext.set("code", code))
}

fn state<'a>(ctx: &'a ResolverContext) -> &'a AppState {
    ctx.data::<AppState>().expect("AppState injected into schema data")
}
fn principal<'a>(ctx: &'a ResolverContext) -> &'a Principal {
    ctx.data::<Principal>().expect("Principal injected per request")
}

/// A field resolver that pulls `key` out of the parent JSON object.
/// Used for every scalar/relation field on output objects and Meta.
pub fn json_field_resolver(key: &str) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    let key = key.to_string();
    move |ctx: ResolverContext| {
        let key = key.clone();
        FieldFuture::new(async move {
            let parent = ctx.parent_value.as_value().cloned();
            // parent_value is a GqlValue::Object when we set FieldValue::value(json->gql)
            let obj = match parent {
                Some(GqlValue::Object(m)) => m,
                _ => return Ok(None),
            };
            match obj.get(key.as_str()) {
                Some(v) if v != &GqlValue::Null => {
                    // Nested objects/lists (relations, media, data array) are
                    // carried as owned FieldValues so their child resolvers run.
                    Ok(Some(FieldValue::value(v.clone())))
                }
                _ => Ok(None),
            }
        })
    }
}

pub fn meta_resolver(key: &str) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    json_field_resolver(key)
}

/// Convert a serde_json::Value into an async-graphql Value (deep).
fn json_to_gql(v: Value) -> GqlValue {
    GqlValue::from_json(v).unwrap_or(GqlValue::Null)
}

pub fn list_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx: ResolverContext| {
        let ct_name = ct_name.clone();
        FieldFuture::new(async move {
            let st = state(&ctx).clone();
            let pr = principal(&ctx).clone();
            // args
            let page = ctx.args.try_get("page").ok().and_then(|v| v.u64().ok());
            let page_size = ctx.args.try_get("pageSize").ok().and_then(|v| v.u64().ok());
            let sort = ctx.args.try_get("sort").ok().and_then(|v| v.string().ok().map(|s| s.to_string()));
            // filters arg is a JSON scalar -> build a raw_query string the
            // existing filter parser understands. v1: accept Strapi pairs as a
            // flat object {"field":{"$op":"value"}} and serialize to
            // filters[field][$op]=value. Reuse the same serializer the UI uses
            // conceptually; here build it inline.
            let raw_query = ctx
                .args
                .try_get("filters")
                .ok()
                .and_then(|v| v.deserialize::<Value>().ok())
                .map(filters_to_raw_query)
                .unwrap_or_default();

            let params = ListParams {
                page: page.map(|p| p as u32),
                page_size: page_size.map(|p| p as u32),
                sort,
                populate: None, // nested selection drives population (see note)
                status: None,
                ..Default::default()
            };
            let env = content::list_entries(&st, &pr, &ct_name, params, None, &raw_query)
                .await
                .map_err(gql_err)?;
            Ok(Some(FieldValue::value(json_to_gql(env))))
        })
    }
}

pub fn get_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx: ResolverContext| {
        let ct_name = ct_name.clone();
        FieldFuture::new(async move {
            let st = state(&ctx).clone();
            let pr = principal(&ctx).clone();
            let id = parse_id_arg(&ctx, "id")?;
            match content::get_entry(&st, &pr, &ct_name, id, None).await {
                Ok(v) => Ok(Some(FieldValue::value(json_to_gql(v)))),
                Err(Error::NotFound) => Ok(None),
                Err(e) => Err(gql_err(e)),
            }
        })
    }
}

pub fn create_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx: ResolverContext| {
        let ct_name = ct_name.clone();
        FieldFuture::new(async move {
            let st = state(&ctx).clone();
            let pr = principal(&ctx).clone();
            let body = input_arg(&ctx, "data")?;
            let v = content::create_entry(&st, &pr, &ct_name, body).await.map_err(gql_err)?;
            Ok(Some(FieldValue::value(json_to_gql(v))))
        })
    }
}

pub fn update_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx: ResolverContext| {
        let ct_name = ct_name.clone();
        FieldFuture::new(async move {
            let st = state(&ctx).clone();
            let pr = principal(&ctx).clone();
            let id = parse_id_arg(&ctx, "id")?;
            let body = input_arg(&ctx, "data")?;
            let v = content::update_entry(&st, &pr, &ct_name, id, body).await.map_err(gql_err)?;
            Ok(Some(FieldValue::value(json_to_gql(v))))
        })
    }
}

pub fn delete_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx: ResolverContext| {
        let ct_name = ct_name.clone();
        FieldFuture::new(async move {
            let st = state(&ctx).clone();
            let pr = principal(&ctx).clone();
            let id = parse_id_arg(&ctx, "id")?;
            content::delete_entry(&st, &pr, &ct_name, id).await.map_err(gql_err)?;
            Ok(Some(FieldValue::value(GqlValue::from(true))))
        })
    }
}

fn parse_id_arg(ctx: &ResolverContext, name: &str) -> Result<Uuid, GqlError> {
    let s = ctx.args.try_get(name).map_err(|_| gql_err(Error::Validation(
        ferrum_core::ValidationErrors::single("missing id"))))?
        .string().map_err(|_| gql_err(Error::Validation(
        ferrum_core::ValidationErrors::single("id must be a string"))))?
        .to_string();
    Uuid::parse_str(&s).map_err(|_| gql_err(Error::Validation(
        ferrum_core::ValidationErrors::single("id is not a valid uuid"))))
}

fn input_arg(ctx: &ResolverContext, name: &str) -> Result<Map<String, Value>, GqlError> {
    let v: Value = ctx.args.try_get(name)
        .map_err(|_| gql_err(Error::Validation(ferrum_core::ValidationErrors::single("missing input"))))?
        .deserialize().map_err(|_| gql_err(Error::Validation(
            ferrum_core::ValidationErrors::single("input must be an object"))))?;
    match v {
        Value::Object(m) => Ok(m),
        _ => Err(gql_err(Error::Validation(ferrum_core::ValidationErrors::single("input must be an object")))),
    }
}

/// Translate a Strapi-style filters object into the percent-decoded raw query
/// string the existing `filter::parse` consumes, e.g.
/// {"title":{"$containsi":"hi"}} -> "filters[title][$containsi]=hi".
fn filters_to_raw_query(v: Value) -> String {
    let mut parts = Vec::new();
    if let Value::Object(fields) = v {
        for (field, ops) in fields {
            if let Value::Object(opmap) = ops {
                for (op, val) in opmap {
                    let val_s = match val {
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    parts.push(format!("filters[{field}][{op}]={val_s}"));
                }
            }
        }
    }
    parts.join("&")
}
```

> Path checks before coding: confirm `ListParams` import path (it is `crate::query::ListParams` per content.rs `use crate::query::{parse_list, ListParams}` — adjust the `use` accordingly; the `ferrum_schema::query` path above is a placeholder, fix it). Confirm `ListParams` field names and that it derives `Default` (if not, construct it explicitly without `..Default::default()`). Confirm `GqlValue::from_json` exists in the async-graphql version resolved (it does in 7.x as `Value::from_json`). Confirm `ctx.args.try_get(..).deserialize()` is available; if the API differs, use `.value()` + manual conversion.

- [ ] **Step 2: Swap build.rs to use the real resolvers; remove temp stubs**

In `crates/http/src/graphql/build.rs`, delete the TEMP stub `json_field_resolver`/`meta_resolver` and import the real ones: `use crate::graphql::resolve::{json_field_resolver, meta_resolver};`. Add `pub mod resolve;` to `mod.rs`.

- [ ] **Step 3: Build**

Run: `cargo build -p ferrum-http`
Expected: compiles. Fix any API-shape mismatches flagged by the compiler (arg accessors, `FieldValue` construction).

- [ ] **Step 4: Re-run schema shape tests (still pass with real resolvers)**

Run: `cargo test -p ferrum-http graphql`
Expected: PASS (scalars + build SDL tests; resolvers aren't exercised by SDL).

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/graphql/resolve.rs crates/http/src/graphql/build.rs crates/http/src/graphql/mod.rs
git commit -m "feat(graphql): resolvers delegating to shared content CRUD"
```

---

## Task 6: GqlRegistry cache + AppState field

**Files:**
- Modify: `crates/http/src/graphql/mod.rs`
- Modify: `crates/http/src/state.rs`

- [ ] **Step 1: Add `GqlRegistry` to `mod.rs`**

```rust
use crate::state::AppState;
use async_graphql::dynamic::Schema;
use ferrum_core::ContentType;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod build;
pub mod handler;
pub mod resolve;
pub mod scalars;

/// Cached dynamic GraphQL schema, rebuilt on content-type CRUD. Mirrors
/// RoleRegistry. The schema is built with AppState + the schema data injected,
/// but AppState contains GqlRegistry itself — so we inject AppState per request
/// in the handler (cheap clone) rather than baking it into the schema. The
/// cached Schema therefore has NO data baked in; data is supplied at execute
/// time via Request::data.
#[derive(Clone, Default)]
pub struct GqlRegistry {
    inner: Arc<RwLock<Option<Schema>>>,
}

impl GqlRegistry {
    pub fn new() -> Self { Self::default() }

    /// Rebuild from the given content types. Call at boot and after any
    /// content-type create/patch/delete.
    pub async fn rebuild(&self, types: &[ContentType]) -> Result<(), async_graphql::dynamic::SchemaError> {
        let schema = build::build_schema(types)?;
        *self.inner.write().await = Some(schema);
        Ok(())
    }

    /// Current schema clone for execution. None if not built yet.
    pub async fn current(&self) -> Option<Schema> {
        self.inner.read().await.clone()
    }
}
```

> Decision recorded here: the cached `Schema` has NO `AppState` baked in (would be a cyclic ownership / staleness problem since AppState owns the GqlRegistry). Instead the handler injects `AppState` and `Principal` into each `Request` via `.data(...)`. Resolvers read them with `ctx.data::<AppState>()`. Update the resolver `state()`/`principal()` helpers accordingly (they already use `ctx.data`).

- [ ] **Step 2: Add the field to AppState**

In `crates/http/src/state.rs`, add to the `AppState` struct (near `roles`):

```rust
pub gql: crate::graphql::GqlRegistry,
```

- [ ] **Step 3: Build (expect errors at AppState construction sites)**

Run: `cargo build --workspace`
Expected: FAIL — every `AppState { .. }` literal now misses `gql`. Note the list (memory: several sites incl. tests).

- [ ] **Step 4: Add `gql: GqlRegistry::new()` (or `Default::default()`) at every construction site**

Grep and fix: `grep -rn "AppState {" crates/`. For each literal, add `gql: crate::graphql::GqlRegistry::new(),` (in test helpers, `Default::default()` is fine). The boot site in `crates/bin/src/main.rs` is handled in Task 8 (build + assign there); for now add `gql: Default::default()` so it compiles.

- [ ] **Step 5: Build clean**

Run: `cargo build --workspace`
Expected: compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/graphql/mod.rs crates/http/src/state.rs crates/
git commit -m "feat(graphql): GqlRegistry schema cache + AppState.gql field"
```

---

## Task 7: axum handler + GraphiQL (`handler.rs`)

**Files:**
- Create: `crates/http/src/graphql/handler.rs`
- Modify: `crates/http/src/routes/mod.rs`

- [ ] **Step 1: Write the handler**

```rust
//! /api/graphql endpoint. POST executes; GET serves GraphiQL when docs_enabled.

use crate::state::AppState;
use async_graphql::http::GraphiQLSource;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use ferrum_core::Principal;

/// POST /api/graphql — execute a query/mutation. AppState + Principal are
/// injected into the request so resolvers can reach them via ctx.data.
pub async fn execute(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    req: GraphQLRequest,
) -> Response {
    let Some(schema) = state.gql.current().await else {
        return (StatusCode::SERVICE_UNAVAILABLE, "graphql schema not built").into_response();
    };
    let request = req.into_inner().data(state.clone()).data(principal);
    let resp: GraphQLResponse = schema.execute(request).await.into();
    resp.into_response()
}

/// GET /api/graphql — GraphiQL playground (mounted only when docs_enabled).
pub async fn playground() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/api/graphql").finish())
}
```

- [ ] **Step 2: Mount in the protected router**

In `crates/http/src/routes/mod.rs`, inside `build_router`, after the built-in `.merge(...)` chain and before the `extra` loop, add:

```rust
{
    use axum::routing::{get, post};
    let mut gql = Router::new().route("/api/graphql", post(crate::graphql::handler::execute));
    if state.config.docs_enabled {
        gql = gql.route("/api/graphql", get(crate::graphql::handler::playground).post(crate::graphql::handler::execute));
    }
    protected = protected.merge(gql);
}
```

> Note: axum panics on duplicate method+path. The conditional sets GET+POST when docs are on, POST only when off — assign the full route once based on the flag (rewrite as an `if/else` that builds the single `/api/graphql` route with the right method set, to avoid registering POST twice). Concretely:
>
> ```rust
> let route = if state.config.docs_enabled {
>     get(crate::graphql::handler::playground).post(crate::graphql::handler::execute)
> } else {
>     post(crate::graphql::handler::execute)
> };
> protected = protected.merge(Router::new().route("/api/graphql", route));
> ```

- [ ] **Step 3: Build**

Run: `cargo build --workspace`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/graphql/handler.rs crates/http/src/routes/mod.rs
git commit -m "feat(graphql): /api/graphql handler + GraphiQL playground"
```

---

## Task 8: Boot wiring + rebuild on content-type CRUD

**Files:**
- Modify: `crates/bin/src/main.rs`
- Modify: `crates/http/src/routes/schema.rs`

- [ ] **Step 1: Build the schema at boot**

In `crates/bin/src/main.rs`, after the schema registry is hydrated (`reload_from_db`) and before/after `AppState` is constructed: build the registry, then call `state.gql.rebuild(&state.schemas.registry().list().await).await`. If `AppState` is constructed once and used to build the router, do:

```rust
// after AppState `state` is constructed with gql: GqlRegistry::new()
let types = state.schemas.registry().list().await;
state.gql.rebuild(&types).await.expect("build initial GraphQL schema");
```

Place it before `build_router(state.clone(), ...)`.

- [ ] **Step 2: Rebuild on content-type create/patch/delete**

In `crates/http/src/routes/schema.rs`, in each of `create`, `patch_one`, `delete_one`, after the `SchemaService` mutation succeeds (the point where the registry is already updated), add:

```rust
let types = state.schemas.registry().list().await;
if let Err(e) = state.gql.rebuild(&types).await {
    tracing::error!(error = %e, "failed to rebuild GraphQL schema after content-type change");
}
```

Non-fatal: a rebuild failure logs but does not fail the content-type mutation (the registry is already consistent; the old GQL schema keeps serving until the next successful rebuild). Mirror this in `routes/single_type.rs` only if single types later join the GraphQL surface — not in v1.

- [ ] **Step 3: Build**

Run: `cargo build --workspace`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/bin/src/main.rs crates/http/src/routes/schema.rs
git commit -m "feat(graphql): build schema at boot, rebuild on content-type CRUD"
```

---

## Task 9: Integration tests

**Files:**
- Create: `crates/bin/tests/graphql.rs`

- [ ] **Step 1: Write the integration suite**

Model on `crates/bin/tests/integration_roles.rs` (TestApp, admin helper). Helper to POST GraphQL:

```rust
mod common;
use common::TestApp;
use serde_json::{json, Value};

async fn make_article(app: &TestApp) {
    let r = app.admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article",
            "display_name": "Article",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "views", "kind": "integer"}
            ]
        })).send().await.unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());
}

async fn gql(app: &TestApp, query: &str, variables: Value) -> Value {
    let r = app.admin(app.client.post(app.url("/api/graphql")))
        .json(&json!({ "query": query, "variables": variables }))
        .send().await.unwrap();
    assert_eq!(r.status(), 200, "{}", r.text().await.unwrap());
    r.json().await.unwrap()
}

#[tokio::test]
async fn create_query_update_delete_roundtrip() {
    let app = TestApp::spawn().await;
    make_article(&app).await;

    // create
    let created = gql(&app,
        "mutation($d: ArticleInput!){ createArticle(data:$d){ id title views } }",
        json!({ "d": { "title": "Hello", "views": 3 } })).await;
    assert!(created["errors"].is_null(), "{created}");
    let id = created["data"]["createArticle"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["createArticle"]["title"], "Hello");

    // get_one
    let got = gql(&app,
        "query($id: UUID!){ article(id:$id){ title views } }",
        json!({ "id": id })).await;
    assert_eq!(got["data"]["article"]["title"], "Hello");

    // list with sort + pagination meta
    let listed = gql(&app,
        "{ articles(page:1, pageSize:10, sort:\"title:asc\"){ data{ id title } meta{ total page pageSize } } }",
        json!({})).await;
    assert_eq!(listed["data"]["articles"]["meta"]["total"], 1);
    assert_eq!(listed["data"]["articles"]["data"][0]["title"], "Hello");

    // update
    let updated = gql(&app,
        "mutation($id: UUID!, $d: ArticleInput!){ updateArticle(id:$id, data:$d){ title views } }",
        json!({ "id": id, "d": { "title": "Hi2", "views": 9 } })).await;
    assert_eq!(updated["data"]["updateArticle"]["title"], "Hi2");

    // delete
    let deleted = gql(&app,
        "mutation($id: UUID!){ deleteArticle(id:$id) }",
        json!({ "id": id })).await;
    assert_eq!(deleted["data"]["deleteArticle"], true);

    // gone -> get returns null
    let gone = gql(&app, "query($id: UUID!){ article(id:$id){ title } }", json!({ "id": id })).await;
    assert!(gone["data"]["article"].is_null(), "{gone}");
}

#[tokio::test]
async fn not_found_has_error_code() {
    let app = TestApp::spawn().await;
    make_article(&app).await;
    let nil = "00000000-0000-0000-0000-000000000000";
    // get_one missing -> data null, no error (matches GraphQL nullable field)
    let got = gql(&app, "query($id: UUID!){ article(id:$id){ title } }", json!({ "id": nil })).await;
    assert!(got["data"]["article"].is_null());
    // update missing -> error with NOT_FOUND code
    let upd = gql(&app,
        "mutation($id: UUID!,$d: ArticleInput!){ updateArticle(id:$id,data:$d){ title } }",
        json!({ "id": nil, "d": { "title": "x" } })).await;
    assert_eq!(upd["errors"][0]["extensions"]["code"], "NOT_FOUND", "{upd}");
}

#[tokio::test]
async fn filter_narrows_results() {
    let app = TestApp::spawn().await;
    make_article(&app).await;
    for t in ["alpha", "beta", "alphabet"] {
        gql(&app, "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
            json!({ "d": { "title": t } })).await;
    }
    let r = gql(&app,
        "query($f: JSON){ articles(filters:$f){ meta{ total } data{ title } } }",
        json!({ "f": { "title": { "$containsi": "alpha" } } })).await;
    assert_eq!(r["data"]["articles"]["meta"]["total"], 2, "{r}");
}

#[tokio::test]
async fn schema_reflects_new_type_without_restart() {
    let app = TestApp::spawn().await;
    // before: no 'article' field -> introspection lacks it
    make_article(&app).await;
    // after creating the type, the query must resolve (would error "unknown field" otherwise)
    let listed = gql(&app, "{ articles(page:1,pageSize:5){ meta{ total } } }", json!({})).await;
    assert!(listed["errors"].is_null(), "schema rebuilt on create: {listed}");
    assert_eq!(listed["data"]["articles"]["meta"]["total"], 0);
}
```

- [ ] **Step 2: Run the suite**

Run: `cargo test -p ferrum-bin --test graphql -- --test-threads=1`
Expected: PASS (single-threaded to avoid the cold-parallel testcontainers flake noted in memory). If a field name or arg differs from the built schema, read the `errors` payload printed by the assert and reconcile with `build.rs`.

- [ ] **Step 3: Add an authz-denial test**

Append: create a custom role with no permission on `article`, create a user/token with that role (follow `integration_roles.rs` for the role + principal setup), POST a `createArticle` mutation as that principal, assert `errors[0].extensions.code == "FORBIDDEN"`. (Reuse the role-creation helpers from the roles suite; copy them locally — tests don't share helper modules beyond `common`.)

- [ ] **Step 4: Run the full suite + clippy + fmt**

Run:
```
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets
cargo fmt --all --check
cd ui && pnpm typecheck
```
Expected: all green. (UI typecheck is unaffected but the project workflow requires it before "done".)

- [ ] **Step 5: Commit**

```bash
git add crates/bin/tests/graphql.rs
git commit -m "test(graphql): integration suite — CRUD, filter, authz, live schema rebuild"
```

---

## Self-review notes (for the executor)

- **Spec coverage:** read scope (CRUD parity ✓ Tasks 5/9), engine (dynamic schema ✓ Task 4), sync (rebuild on reload ✓ Tasks 6/8), authz reuse (shared `can` via shared fns ✓ Task 2/5), gating (docs_enabled ✓ Task 7), error codes (✓ Task 5/9), tests (✓ Task 9). N+1 / subscriptions / single-types / field-authz are explicit non-goals.
- **API-shape risk:** async-graphql 7.x dynamic API method names (`ctx.args.try_get`, `.deserialize()`, `FieldValue::value`, `GqlValue::from_json`, `parent_value.as_value`) are the most likely to need small adjustment against the resolved crate version. Each task that uses them ends with a build step to surface mismatches early — fix against the compiler, don't guess.
- **populate via nested selection:** v1 passes `populate: None` to `list_entries`/`get_entry`, so nested relation/media fields resolve only if their values are already embedded in the row JSON. Full nested-selection-driven population (walking the GraphQL selection set to decide what to populate) is deferred — note it as a follow-up. If embedded relations return null in tests, that is expected for v1; tests above avoid asserting nested relation contents.
- **ListParams construction:** verify the real struct's fields/Default before relying on `..Default::default()`.
```
