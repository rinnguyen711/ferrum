# OpenAPI Docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve a runtime-generated OpenAPI 3.1 spec at `GET /openapi.json` and a Swagger UI at `GET /docs`, reflecting the live content-type registry, public by default and disableable via config.

**Architecture:** A new `crates/http/src/openapi/` module assembles a `serde_json::Value` OpenAPI doc from `AppState`: a hand-written static block for `/admin/*`, `/auth/*`, `/healthz`, plus a dynamic block generated per `ContentType` from `state.schemas.registry().list()`. Two public (no-auth) axum routes serve the JSON and a CDN-loaded Swagger UI page. A `docs_enabled` config flag gates route registration.

**Tech Stack:** Rust, axum 0.7, serde_json, existing `ferrum_core` field model, testcontainers integration harness.

Spec: [docs/superpowers/specs/2026-06-04-openapi-docs-design.md](../specs/2026-06-04-openapi-docs-design.md)

---

## File Structure

- Create `crates/http/src/openapi/mod.rs` — handlers (`openapi_json`, `docs_ui`) + `router()` + the Swagger UI HTML.
- Create `crates/http/src/openapi/schema.rs` — `field_to_schema(&Field)`, `content_type_schemas(&ContentType)`, `content_type_paths(&ContentType)`.
- Create `crates/http/src/openapi/spec.rs` — `build(&AppState) -> Value` assembling info/servers/security/paths/components.
- Create `crates/http/src/openapi/static_paths.rs` — `static_paths() -> Value`, `static_components() -> Value`.
- Modify `crates/http/src/lib.rs` — declare `pub mod openapi;`.
- Modify `crates/http/src/state.rs:49-56` — add `docs_enabled`, `api_version`, `public_base_url` to `AppConfig`.
- Modify `crates/http/src/routes/mod.rs:14-31` — conditionally merge `openapi::router()` into the public router.
- Modify `crates/bin/src/config.rs` — read `FERRUM_DOCS_ENABLED`, `FERRUM_API_VERSION`, `FERRUM_PUBLIC_URL`.
- Modify `crates/bin/src/main.rs:46-50` — pass new config fields into `AppConfig`.
- Modify `crates/bin/tests/common/mod.rs:64-68` — add new fields to the test `AppConfig`.
- Create `crates/bin/tests/openapi.rs` — integration tests.

---

## Task 1: Extend AppConfig with docs fields

**Files:**
- Modify: `crates/http/src/state.rs:49-56`

- [ ] **Step 1: Add fields to `AppConfig`**

In `crates/http/src/state.rs`, replace the `AppConfig` struct (lines 49-56):

```rust
#[derive(Clone)]
pub struct AppConfig {
    /// HS256 signing secret for JWTs.
    pub jwt_secret: String,
    /// Access-token lifetime in seconds.
    pub jwt_ttl_secs: i64,
    pub page_size_max: u32,
    /// When false, `/openapi.json` and `/docs` are not mounted (prod opt-out).
    pub docs_enabled: bool,
    /// Reported as `info.version` in the OpenAPI doc.
    pub api_version: String,
    /// Reported as the single `servers[0].url` in the OpenAPI doc.
    pub public_base_url: String,
}
```

- [ ] **Step 2: Verify it compiles (expect errors at construction sites)**

Run: `cargo build -p ferrum-http`
Expected: FAIL — `crates/bin` construction sites missing fields are not built here, so `ferrum-http` alone should COMPILE. Confirm `ferrum-http` builds clean.

- [ ] **Step 3: Commit**

```bash
git add crates/http/src/state.rs
git commit -m "feat(openapi): add docs config fields to AppConfig"
```

---

## Task 2: field_to_schema mapping (TDD)

**Files:**
- Create: `crates/http/src/openapi/schema.rs`
- Modify: `crates/http/src/lib.rs`

- [ ] **Step 1: Declare the module**

In `crates/http/src/lib.rs`, add after the existing `pub mod media_embed;` line (keep alphabetical-ish grouping; placement is not load-bearing):

```rust
pub mod openapi;
```

- [ ] **Step 2: Create module dir with mod.rs stub so the crate compiles**

Create `crates/http/src/openapi/mod.rs`:

```rust
//! Runtime-generated OpenAPI 3.1 spec + Swagger UI.

pub mod schema;
```

- [ ] **Step 3: Write the failing test**

Create `crates/http/src/openapi/schema.rs`:

```rust
//! Maps the content-type field model to OpenAPI/JSON Schema fragments.

use ferrum_core::field::{Field, FieldKind};
use serde_json::{json, Value};

/// Build a JSON Schema fragment for a single field's value type.
pub fn field_to_schema(field: &Field) -> Value {
    let mut schema = match field.kind {
        FieldKind::String => {
            json!({ "type": "string", "maxLength": field.effective_max_length() })
        }
        // Text is unbounded Postgres `text`; only surface a maxLength the user set.
        FieldKind::Text => match field.max_length {
            Some(n) => json!({ "type": "string", "maxLength": n }),
            None => json!({ "type": "string" }),
        },
        FieldKind::Integer => json!({ "type": "integer", "format": "int64" }),
        FieldKind::Float => json!({ "type": "number", "format": "double" }),
        FieldKind::Boolean => json!({ "type": "boolean" }),
        FieldKind::Datetime => json!({ "type": "string", "format": "date-time" }),
        FieldKind::Uuid => json!({ "type": "string", "format": "uuid" }),
        FieldKind::Email => json!({ "type": "string", "format": "email" }),
        FieldKind::Url => json!({ "type": "string", "format": "uri" }),
        FieldKind::Slug => {
            json!({ "type": "string", "pattern": "^[a-z0-9]+(?:-[a-z0-9]+)*$" })
        }
        FieldKind::Enum => {
            // Empty `enum` is invalid JSON Schema; fall back to a bare string
            // if enum_meta is missing/corrupt.
            let values = field.enum_meta().map(|m| m.values).unwrap_or_default();
            if values.is_empty() {
                json!({ "type": "string" })
            } else {
                json!({ "type": "string", "enum": values })
            }
        }
        FieldKind::Json => json!({}),
        FieldKind::Relation => {
            let many = field
                .relation_meta()
                .map(|m| {
                    matches!(m.cardinality, ferrum_core::field::Cardinality::ManyToMany)
                })
                .unwrap_or(false);
            if many {
                json!({ "type": "array", "items": { "type": "string", "format": "uuid" } })
            } else {
                json!({ "type": "string", "format": "uuid" })
            }
        }
        FieldKind::Media => {
            let multiple = field.media_meta().map(|m| m.multiple).unwrap_or(false);
            if multiple {
                json!({ "type": "array", "items": { "type": "string", "format": "uuid" } })
            } else {
                json!({ "type": "string", "format": "uuid" })
            }
        }
        // FieldKind is #[non_exhaustive]; stay permissive for future kinds.
        _ => json!({}),
    };
    if !field.default.is_null() {
        if let Value::Object(ref mut map) = schema {
            map.insert("default".into(), field.default.clone());
        }
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_core::field::Field;
    use serde_json::json;

    fn f(kind: FieldKind, kind_meta: Value) -> Field {
        Field {
            name: "x".into(),
            kind,
            required: false,
            unique: false,
            default: Value::Null,
            max_length: None,
            kind_meta,
        }
    }

    #[test]
    fn string_has_maxlength() {
        let s = field_to_schema(&f(FieldKind::String, json!({})));
        assert_eq!(s["type"], "string");
        assert_eq!(s["maxLength"], 255);
    }

    #[test]
    fn integer_float_bool() {
        assert_eq!(field_to_schema(&f(FieldKind::Integer, json!({})))["format"], "int64");
        assert_eq!(field_to_schema(&f(FieldKind::Float, json!({})))["format"], "double");
        assert_eq!(field_to_schema(&f(FieldKind::Boolean, json!({})))["type"], "boolean");
    }

    #[test]
    fn datetime_uuid_email_url() {
        assert_eq!(field_to_schema(&f(FieldKind::Datetime, json!({})))["format"], "date-time");
        assert_eq!(field_to_schema(&f(FieldKind::Uuid, json!({})))["format"], "uuid");
        assert_eq!(field_to_schema(&f(FieldKind::Email, json!({})))["format"], "email");
        assert_eq!(field_to_schema(&f(FieldKind::Url, json!({})))["format"], "uri");
    }

    #[test]
    fn slug_has_pattern() {
        let s = field_to_schema(&f(FieldKind::Slug, json!({})));
        assert!(s["pattern"].is_string());
    }

    #[test]
    fn enum_lists_values() {
        let s = field_to_schema(&f(FieldKind::Enum, json!({ "values": ["draft", "published"] })));
        assert_eq!(s["enum"], json!(["draft", "published"]));
    }

    #[test]
    fn json_is_any() {
        assert_eq!(field_to_schema(&f(FieldKind::Json, json!({}))), json!({}));
    }

    #[test]
    fn relation_single_vs_many() {
        let one = field_to_schema(&f(FieldKind::Relation, json!({ "target": "user", "cardinality": "many_to_one" })));
        assert_eq!(one["format"], "uuid");
        let many = field_to_schema(&f(FieldKind::Relation, json!({ "target": "tag", "cardinality": "many_to_many" })));
        assert_eq!(many["type"], "array");
    }

    #[test]
    fn media_single_vs_multiple() {
        let one = field_to_schema(&f(FieldKind::Media, json!({ "multiple": false })));
        assert_eq!(one["format"], "uuid");
        let many = field_to_schema(&f(FieldKind::Media, json!({ "multiple": true })));
        assert_eq!(many["type"], "array");
    }

    #[test]
    fn default_is_emitted() {
        let mut field = f(FieldKind::Integer, json!({}));
        field.default = json!(7);
        assert_eq!(field_to_schema(&field)["default"], json!(7));
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ferrum-http openapi::schema::tests`
Expected: PASS (all 9 tests). If `Cardinality` import path is wrong, fix to match `ferrum_core::field::Cardinality` (it is re-exported there per `crates/core/src/field.rs:488`).

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/lib.rs crates/http/src/openapi/mod.rs crates/http/src/openapi/schema.rs
git commit -m "feat(openapi): map field kinds to JSON Schema"
```

---

## Task 3: Per-content-type component schemas and paths (TDD)

**Files:**
- Modify: `crates/http/src/openapi/schema.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/http/src/openapi/schema.rs` (before the `#[cfg(test)] mod tests`, add the functions; add tests inside the existing test module):

Functions to add at module level:

```rust
use ferrum_core::ContentType;

/// Returns (response_schema_name, request_schema_name) for a content type.
pub fn schema_names(ct_name: &str) -> (String, String) {
    let pascal = to_pascal(ct_name);
    (pascal.clone(), format!("{pascal}Input"))
}

fn to_pascal(name: &str) -> String {
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

/// Build response (`T`) and request (`TInput`) component schemas for a type.
/// Returns a JSON object suitable for splicing into `components.schemas`.
pub fn content_type_schemas(ct: &ContentType) -> Value {
    let (resp_name, req_name) = schema_names(&ct.name);

    let mut resp_props = serde_json::Map::new();
    resp_props.insert("id".into(), json!({ "type": "string", "format": "uuid" }));
    resp_props.insert("created_at".into(), json!({ "type": "string", "format": "date-time" }));
    resp_props.insert("updated_at".into(), json!({ "type": "string", "format": "date-time" }));

    let mut req_props = serde_json::Map::new();
    let mut req_required: Vec<String> = Vec::new();
    let mut resp_required: Vec<String> =
        vec!["id".into(), "created_at".into(), "updated_at".into()];

    for field in &ct.fields {
        let s = field_to_schema(field);
        resp_props.insert(field.name.clone(), s.clone());
        req_props.insert(field.name.clone(), s);
        if field.required {
            req_required.push(field.name.clone());
            resp_required.push(field.name.clone());
        }
    }

    json!({
        resp_name: {
            "type": "object",
            "properties": Value::Object(resp_props),
            "required": resp_required,
        },
        req_name: {
            "type": "object",
            "properties": Value::Object(req_props),
            "required": req_required,
        }
    })
}

/// Build the `/api/{name}` and `/api/{name}/{id}` path items for a type.
pub fn content_type_paths(ct: &ContentType) -> Value {
    let (resp_name, req_name) = schema_names(&ct.name);
    let resp_ref = format!("#/components/schemas/{resp_name}");
    let req_ref = format!("#/components/schemas/{req_name}");
    let tag = ct.display_name.clone();
    let secured = json!([{ "bearerAuth": [] }]);
    let errs = json!({
        "401": { "$ref": "#/components/responses/Unauthorized" },
        "403": { "$ref": "#/components/responses/Forbidden" },
        "404": { "$ref": "#/components/responses/NotFound" }
    });

    let list_get = json!({
        "tags": [tag],
        "summary": format!("List {} entries", ct.name),
        "security": secured,
        "parameters": [
            { "name": "page", "in": "query", "schema": { "type": "integer" } },
            { "name": "pageSize", "in": "query", "schema": { "type": "integer" } },
            { "name": "sort", "in": "query", "schema": { "type": "string" } },
            { "name": "populate", "in": "query", "schema": { "type": "string" } }
        ],
        "responses": merge_obj(json!({
            "200": {
                "description": "List of entries",
                "content": { "application/json": { "schema": {
                    "type": "object",
                    "required": ["data", "meta"],
                    "properties": {
                        "data": { "type": "array", "items": { "$ref": resp_ref } },
                        "meta": { "type": "object",
                            "required": ["page", "pageSize", "total"],
                            "properties": {
                                "page": { "type": "integer" },
                                "pageSize": { "type": "integer" },
                                "total": { "type": "integer" }
                            }}
                    }
                }}}
            }
        }), errs.clone())
    });

    let create_post = json!({
        "tags": [tag],
        "summary": format!("Create a {} entry", ct.name),
        "security": secured,
        "requestBody": { "required": true, "content": { "application/json": {
            "schema": { "$ref": req_ref }
        }}},
        "responses": merge_obj(json!({
            "201": { "description": "Created", "content": { "application/json": {
                "schema": { "$ref": resp_ref }
            }}}
        }), errs.clone())
    });

    let id_param = json!([{
        "name": "id", "in": "path", "required": true,
        "schema": { "type": "string", "format": "uuid" }
    }]);

    let get_one = json!({
        "tags": [tag], "summary": format!("Fetch one {} entry", ct.name),
        "security": secured, "parameters": id_param,
        "responses": merge_obj(json!({
            "200": { "description": "Entry", "content": { "application/json": {
                "schema": { "$ref": resp_ref }
            }}}
        }), errs.clone())
    });

    let put_one = json!({
        "tags": [tag], "summary": format!("Replace a {} entry", ct.name),
        "security": secured, "parameters": id_param,
        "requestBody": { "required": true, "content": { "application/json": {
            "schema": { "$ref": req_ref }
        }}},
        "responses": merge_obj(json!({
            "200": { "description": "Updated", "content": { "application/json": {
                "schema": { "$ref": resp_ref }
            }}}
        }), errs.clone())
    });

    let delete_one = json!({
        "tags": [tag], "summary": format!("Delete a {} entry", ct.name),
        "security": secured, "parameters": id_param,
        "responses": merge_obj(json!({ "204": { "description": "Deleted" } }), errs)
    });

    json!({
        format!("/api/{}", ct.name): {
            "get": list_get,
            "post": create_post
        },
        format!("/api/{}/{{id}}", ct.name): {
            "get": get_one,
            "put": put_one,
            "delete": delete_one
        }
    })
}

/// Shallow-merge two JSON objects (right wins on key conflict).
fn merge_obj(mut base: Value, extra: Value) -> Value {
    if let (Value::Object(ref mut b), Value::Object(e)) = (&mut base, extra) {
        for (k, v) in e {
            b.insert(k, v);
        }
    }
    base
}
```

Tests to add inside the existing `mod tests`:

```rust
    use ferrum_core::ContentType;
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_ct() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "article".into(),
            display_name: "Article".into(),
            fields: vec![
                Field { name: "title".into(), kind: FieldKind::String, required: true, unique: false, default: Value::Null, max_length: None, kind_meta: json!({}) },
                Field { name: "views".into(), kind: FieldKind::Integer, required: false, unique: false, default: Value::Null, max_length: None, kind_meta: json!({}) },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn schema_names_pascalcase() {
        assert_eq!(schema_names("blog_post"), ("BlogPost".into(), "BlogPostInput".into()));
    }

    #[test]
    fn response_schema_has_system_fields_request_does_not() {
        let s = content_type_schemas(&sample_ct());
        let resp = &s["Article"]["properties"];
        let req = &s["ArticleInput"]["properties"];
        assert!(resp["id"].is_object());
        assert!(resp["created_at"].is_object());
        assert!(resp["title"].is_object());
        assert!(req["title"].is_object());
        assert!(req["id"].is_null(), "request schema must omit id");
        assert!(req["created_at"].is_null(), "request schema must omit timestamps");
    }

    #[test]
    fn required_field_listed_in_both() {
        let s = content_type_schemas(&sample_ct());
        assert!(s["Article"]["required"].as_array().unwrap().iter().any(|v| v == "title"));
        assert!(s["ArticleInput"]["required"].as_array().unwrap().iter().any(|v| v == "title"));
    }

    #[test]
    fn paths_cover_list_and_item() {
        let p = content_type_paths(&sample_ct());
        assert!(p["/api/article"]["get"].is_object());
        assert!(p["/api/article"]["post"].is_object());
        assert!(p["/api/article/{id}"]["get"].is_object());
        assert!(p["/api/article/{id}"]["put"].is_object());
        assert!(p["/api/article/{id}"]["delete"].is_object());
    }
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p ferrum-http openapi::schema::tests`
Expected: PASS (all tests incl. the 4 new ones). Add `chrono` / `uuid` to `crates/http/Cargo.toml` `[dev-dependencies]` only if not already present (they are workspace deps used elsewhere in this crate — check `crates/http/Cargo.toml` first; reuse `*.workspace = true`).

- [ ] **Step 3: Commit**

```bash
git add crates/http/src/openapi/schema.rs crates/http/Cargo.toml
git commit -m "feat(openapi): generate component schemas and paths per content type"
```

---

## Task 4: Static paths and components block

**Files:**
- Create: `crates/http/src/openapi/static_paths.rs`
- Modify: `crates/http/src/openapi/mod.rs`

- [ ] **Step 1: Add module declaration**

In `crates/http/src/openapi/mod.rs`, add under the existing `pub mod schema;`:

```rust
pub mod static_paths;
```

- [ ] **Step 2: Write the static block with a smoke test**

Create `crates/http/src/openapi/static_paths.rs`:

```rust
//! Hand-written OpenAPI paths/components for the fixed (non-dynamic) routes:
//! /healthz, /auth/*, /admin/content-types*, /admin/users*, /admin/media/*.
//! These handlers are stable-shaped; update this literal if one changes.

use serde_json::{json, Value};

pub fn static_paths() -> Value {
    let secured = json!([{ "bearerAuth": [] }]);
    json!({
        "/healthz": {
            "get": {
                "tags": ["system"],
                "summary": "Liveness probe",
                "responses": { "200": { "description": "OK" } }
            }
        },
        "/auth/setup": {
            "get": {
                "tags": ["auth"],
                "summary": "First-run setup status",
                "responses": { "200": { "description": "Setup status" } }
            },
            "post": {
                "tags": ["auth"],
                "summary": "Create the first admin user",
                "requestBody": { "required": true, "content": { "application/json": {
                    "schema": { "type": "object",
                        "properties": {
                            "email": { "type": "string", "format": "email" },
                            "password": { "type": "string" }
                        },
                        "required": ["email", "password"] }
                }}},
                "responses": { "201": { "description": "Admin created" } }
            }
        },
        "/auth/login": {
            "post": {
                "tags": ["auth"],
                "summary": "Exchange credentials for a bearer token",
                "requestBody": { "required": true, "content": { "application/json": {
                    "schema": { "type": "object",
                        "properties": {
                            "email": { "type": "string", "format": "email" },
                            "password": { "type": "string" }
                        },
                        "required": ["email", "password"] }
                }}},
                "responses": {
                    "200": { "description": "Token issued", "content": { "application/json": {
                        "schema": { "type": "object", "properties": { "token": { "type": "string" } } }
                    }}},
                    "401": { "$ref": "#/components/responses/Unauthorized" }
                }
            }
        },
        "/auth/me": {
            "get": {
                "tags": ["auth"], "summary": "Current principal",
                "security": secured,
                "responses": { "200": { "description": "Principal" },
                    "401": { "$ref": "#/components/responses/Unauthorized" } }
            }
        },
        "/admin/content-types": {
            "get": { "tags": ["schema"], "summary": "List content types",
                "security": secured, "responses": { "200": { "description": "Content types" } } },
            "post": { "tags": ["schema"], "summary": "Create a content type",
                "security": secured, "responses": { "201": { "description": "Created" } } }
        },
        "/admin/content-types/{name}": {
            "get": { "tags": ["schema"], "summary": "Fetch one content type",
                "security": secured,
                "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
                "responses": { "200": { "description": "Content type" } } },
            "patch": { "tags": ["schema"], "summary": "Patch a content type",
                "security": secured,
                "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
                "responses": { "200": { "description": "Updated" } } },
            "delete": { "tags": ["schema"], "summary": "Delete a content type",
                "security": secured,
                "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
                "responses": { "204": { "description": "Deleted" } } }
        },
        "/admin/users": {
            "get": { "tags": ["users"], "summary": "List users",
                "security": secured, "responses": { "200": { "description": "Users" } } },
            "post": { "tags": ["users"], "summary": "Create a user",
                "security": secured, "responses": { "201": { "description": "Created" } } }
        },
        "/admin/users/{id}": {
            "patch": { "tags": ["users"], "summary": "Update a user",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Updated" } } },
            "delete": { "tags": ["users"], "summary": "Delete a user",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "204": { "description": "Deleted" } } }
        },
        "/admin/media/providers": {
            "get": { "tags": ["media"], "summary": "List storage providers",
                "security": secured, "responses": { "200": { "description": "Providers" } } }
        },
        "/admin/media/settings": {
            "get": { "tags": ["media"], "summary": "Get media settings",
                "security": secured, "responses": { "200": { "description": "Settings" } } },
            "put": { "tags": ["media"], "summary": "Update media settings",
                "security": secured, "responses": { "204": { "description": "Updated" } } }
        },
        "/admin/media/settings/test": {
            "post": { "tags": ["media"], "summary": "Test provider settings",
                "security": secured, "responses": { "200": { "description": "Result" } } }
        },
        "/admin/media/folders": {
            "get": { "tags": ["media"], "summary": "List folders",
                "security": secured, "responses": { "200": { "description": "Folders" } } },
            "post": { "tags": ["media"], "summary": "Create a folder",
                "security": secured, "responses": { "201": { "description": "Created" } } }
        },
        "/admin/media/folders/{id}": {
            "patch": { "tags": ["media"], "summary": "Update a folder",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Updated" } } },
            "delete": { "tags": ["media"], "summary": "Delete a folder",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "204": { "description": "Deleted" } } }
        },
        "/admin/media/assets": {
            "get": { "tags": ["media"], "summary": "List assets",
                "security": secured, "responses": { "200": { "description": "Assets" } } },
            "post": { "tags": ["media"], "summary": "Upload an asset",
                "security": secured, "responses": { "201": { "description": "Uploaded" } } }
        },
        "/admin/media/assets/{id}": {
            "get": { "tags": ["media"], "summary": "Fetch asset metadata",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Asset" } } },
            "patch": { "tags": ["media"], "summary": "Update asset metadata",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Updated" } } },
            "delete": { "tags": ["media"], "summary": "Delete an asset",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "204": { "description": "Deleted" } } }
        },
        "/admin/media/assets/{id}/raw": {
            "get": { "tags": ["media"], "summary": "Download raw asset bytes",
                "security": secured,
                "parameters": [{ "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } }],
                "responses": { "200": { "description": "Raw bytes" } } }
        }
    })
}

/// Reusable `components.responses` referenced by both static and dynamic paths,
/// plus a shared `Error` schema.
pub fn static_components() -> Value {
    let err_content = json!({ "application/json": {
        "schema": { "$ref": "#/components/schemas/Error" }
    }});
    json!({
        "schemas": {
            "Error": {
                "type": "object",
                "properties": {
                    "error": { "type": "string" },
                    "message": { "type": "string" }
                }
            }
        },
        "responses": {
            "Unauthorized": { "description": "Missing or invalid token", "content": err_content },
            "Forbidden": { "description": "Not permitted", "content": err_content },
            "NotFound": { "description": "Resource not found", "content": err_content }
        },
        "securitySchemes": {
            "bearerAuth": { "type": "http", "scheme": "bearer", "bearerFormat": "JWT" }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_paths_include_auth_and_admin() {
        let p = static_paths();
        assert!(p["/auth/login"]["post"].is_object());
        assert!(p["/admin/content-types"]["get"].is_object());
        assert!(p["/admin/media/assets/{id}/raw"]["get"].is_object());
    }

    #[test]
    fn components_define_error_and_security() {
        let c = static_components();
        assert!(c["schemas"]["Error"].is_object());
        assert_eq!(c["securitySchemes"]["bearerAuth"]["scheme"], "bearer");
        assert!(c["responses"]["Unauthorized"].is_object());
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p ferrum-http openapi::static_paths::tests`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/openapi/mod.rs crates/http/src/openapi/static_paths.rs
git commit -m "feat(openapi): hand-written static paths and shared components"
```

---

## Task 5: Assemble the full spec from AppState (TDD)

**Files:**
- Create: `crates/http/src/openapi/spec.rs`
- Modify: `crates/http/src/openapi/mod.rs`

- [ ] **Step 1: Add module declaration**

In `crates/http/src/openapi/mod.rs`, add under the existing module declarations:

```rust
pub mod spec;
```

- [ ] **Step 2: Write the builder + failing test**

Create `crates/http/src/openapi/spec.rs`:

```rust
//! Assembles the full OpenAPI 3.1 document from live AppState.

use crate::openapi::{schema, static_paths};
use crate::state::AppState;
use serde_json::{json, Map, Value};

/// Build the OpenAPI document. Pure assembly over already-loaded registry
/// data plus config; performs the async registry read but no other I/O.
pub async fn build(state: &AppState) -> Value {
    let cfg = &state.config;

    // paths = static block + per-content-type dynamic block.
    let mut paths = static_paths::static_paths();
    let paths_map = paths.as_object_mut().expect("static_paths is an object");

    // components = static components; schemas get extended per content type.
    let mut components = static_paths::static_components();
    let schemas_map = components["schemas"]
        .as_object_mut()
        .expect("components.schemas is an object")
        .clone();
    let mut schemas_map: Map<String, Value> = schemas_map;

    for ct in state.schemas.registry().list().await {
        if let Value::Object(ct_paths) = schema::content_type_paths(&ct) {
            for (k, v) in ct_paths {
                paths_map.insert(k, v);
            }
        }
        if let Value::Object(ct_schemas) = schema::content_type_schemas(&ct) {
            for (k, v) in ct_schemas {
                schemas_map.insert(k, v);
            }
        }
    }
    components["schemas"] = Value::Object(schemas_map);

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "ferrum",
            "version": cfg.api_version,
            "description": "Auto-generated API reference. Dynamic /api/{type} endpoints reflect the live content-type registry."
        },
        "servers": [{ "url": cfg.public_base_url }],
        "paths": paths,
        "components": components
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AlwaysAllow, AppConfig, NoopSink};
    use ferrum_core::{ContentType, Field};
    use ferrum_core::field::FieldKind;
    use ferrum_schema::{SchemaRegistry, SchemaService};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // Builds AppState backed by an in-memory registry only. The DB pool and
    // storage provider are never touched by `build`, so we use lazy/never
    // connections. If `PgPool` cannot be constructed without a live DB in this
    // crate's test context, move this assertion to the integration test in
    // Task 7 and keep only the pure schema/static tests as units.
    #[tokio::test]
    async fn build_includes_dynamic_paths_and_omits_input_system_fields() {
        let registry = SchemaRegistry::new();
        registry
            .insert(ContentType {
                id: uuid::Uuid::nil(),
                name: "article".into(),
                display_name: "Article".into(),
                fields: vec![Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: true,
                    unique: false,
                    default: serde_json::Value::Null,
                    max_length: None,
                    kind_meta: json!({}),
                }],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .await;

        // Construct AppState with a lazily-connected pool (no I/O until used).
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid/never-used")
            .expect("lazy pool");
        let schemas = SchemaService::new(pool.clone(), registry);
        let storage: Arc<RwLock<Arc<dyn ferrum_media::StorageProvider>>> =
            Arc::new(RwLock::new(Arc::new(ferrum_media::LocalProvider::new(
                std::env::temp_dir(),
            ))));
        let state = AppState {
            pool,
            schemas,
            authz: Arc::new(AlwaysAllow),
            events: Arc::new(NoopSink),
            config: AppConfig {
                jwt_secret: "x".repeat(32),
                jwt_ttl_secs: 3600,
                page_size_max: 100,
                docs_enabled: true,
                api_version: "9.9.9".into(),
                public_base_url: "/".into(),
            },
            storage,
            secret_key: None,
        };

        let doc = build(&state).await;
        assert_eq!(doc["openapi"], "3.1.0");
        assert_eq!(doc["info"]["version"], "9.9.9");
        assert!(doc["paths"]["/api/article"]["get"].is_object());
        assert!(doc["paths"]["/auth/login"]["post"].is_object());
        assert!(doc["components"]["schemas"]["Article"].is_object());
        assert!(doc["components"]["schemas"]["ArticleInput"]["properties"]["id"].is_null());
    }
}
```

- [ ] **Step 3: Run the test**

Run: `cargo test -p ferrum-http openapi::spec::tests`
Expected: PASS. If `LocalProvider::new` signature differs, check `crates/media/src/local.rs` and adjust the constructor call. If constructing `AppState` in a unit test proves impractical (e.g. provider constructor needs more), delete this `#[tokio::test]` and rely on the Task 7 integration test (note in the test module why). The `build` function itself stays unchanged.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/openapi/mod.rs crates/http/src/openapi/spec.rs
git commit -m "feat(openapi): assemble full spec from live AppState"
```

---

## Task 6: HTTP handlers, Swagger UI, and router

**Files:**
- Modify: `crates/http/src/openapi/mod.rs`

- [ ] **Step 1: Implement handlers + router**

Replace the contents of `crates/http/src/openapi/mod.rs` with:

```rust
//! Runtime-generated OpenAPI 3.1 spec + Swagger UI.

pub mod schema;
pub mod spec;
pub mod static_paths;

use crate::state::AppState;
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;

async fn openapi_json(State(state): State<AppState>) -> Json<Value> {
    Json(spec::build(&state).await)
}

async fn docs_ui() -> Html<&'static str> {
    Html(SWAGGER_UI_HTML)
}

/// Public (no-auth) routes for the spec JSON and the Swagger UI page.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/openapi.json", get(openapi_json))
        .route("/docs", get(docs_ui))
}

// Note: r##"..."## delimiter — the HTML contains `"#swagger-ui"` which would
// prematurely close a plain r#"..."# raw string.
const SWAGGER_UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>ferrum — API docs</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js" crossorigin></script>
  <script>
    window.onload = () => {
      window.ui = SwaggerUIBundle({
        url: "/openapi.json",
        dom_id: "#swagger-ui",
      });
    };
  </script>
</body>
</html>"##;
```

- [ ] **Step 2: Verify the crate compiles**

Run: `cargo build -p ferrum-http`
Expected: PASS (clean build of `ferrum-http`).

- [ ] **Step 3: Commit**

```bash
git add crates/http/src/openapi/mod.rs
git commit -m "feat(openapi): serve /openapi.json and Swagger UI at /docs"
```

---

## Task 7: Wire into router + config + construction sites

**Files:**
- Modify: `crates/http/src/routes/mod.rs:14-31`
- Modify: `crates/bin/src/config.rs`
- Modify: `crates/bin/src/main.rs:46-50`
- Modify: `crates/bin/tests/common/mod.rs:64-68`

- [ ] **Step 1: Conditionally mount docs routes**

In `crates/http/src/routes/mod.rs`, replace the body of `build_router` (lines 14-31). Add `use crate::openapi;` to the imports at the top, then:

```rust
pub fn build_router(state: AppState) -> Router {
    let mut public = Router::new()
        .route("/healthz", get(health::healthz))
        .merge(auth::public_router());

    if state.config.docs_enabled {
        public = public.merge(openapi::router());
    }

    let protected = Router::new()
        .merge(schema::router())
        .merge(content::router())
        .merge(users::router())
        .merge(media::router())
        .merge(auth::protected_router())
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    public.merge(protected).with_state(state)
}
```

- [ ] **Step 2: Read new env vars in config**

In `crates/bin/src/config.rs`, add three fields to the `Config` struct (after `seed`):

```rust
    /// When false, /openapi.json and /docs are not mounted.
    pub docs_enabled: bool,
    /// Reported as OpenAPI info.version.
    pub api_version: String,
    /// Reported as OpenAPI servers[0].url.
    pub public_base_url: String,
```

And in `from_env`, before the final `Ok(Self { ... })`, add:

```rust
        let docs_enabled = std::env::var("FERRUM_DOCS_ENABLED")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| !matches!(s.as_str(), "0" | "false" | "no"))
            .unwrap_or(true);
        let api_version =
            std::env::var("FERRUM_API_VERSION").unwrap_or_else(|_| "0.1.0".into());
        let public_base_url =
            std::env::var("FERRUM_PUBLIC_URL").unwrap_or_else(|_| "/".into());
```

Then add `docs_enabled, api_version, public_base_url,` to the `Ok(Self { ... })` initializer.

- [ ] **Step 3: Pass fields through in main.rs**

In `crates/bin/src/main.rs`, replace the `AppConfig { ... }` block (lines 46-50):

```rust
        config: AppConfig {
            jwt_secret: cfg.jwt_secret.clone(),
            jwt_ttl_secs: cfg.jwt_ttl_secs,
            page_size_max: cfg.page_size_max,
            docs_enabled: cfg.docs_enabled,
            api_version: cfg.api_version.clone(),
            public_base_url: cfg.public_base_url.clone(),
        },
```

- [ ] **Step 4: Fix the test harness AppConfig**

In `crates/bin/tests/common/mod.rs`, replace the `config: AppConfig { ... }` block (lines 64-68):

```rust
            config: AppConfig {
                jwt_secret: JWT_SECRET.into(),
                jwt_ttl_secs: 3600,
                page_size_max: 100,
                docs_enabled: true,
                api_version: "test".into(),
                public_base_url: "/".into(),
            },
```

- [ ] **Step 5: Build the whole workspace**

Run: `cargo build`
Expected: PASS. Fixes any remaining `AppConfig` construction errors.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/routes/mod.rs crates/bin/src/config.rs crates/bin/src/main.rs crates/bin/tests/common/mod.rs
git commit -m "feat(openapi): wire docs routes, config flags, construction sites"
```

---

## Task 8: Integration tests (runtime freshness + toggle)

**Files:**
- Create: `crates/bin/tests/openapi.rs`

- [ ] **Step 1: Write the integration tests**

Create `crates/bin/tests/openapi.rs`:

```rust
//! End-to-end tests for the runtime-generated OpenAPI docs.

mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn openapi_reflects_created_content_type() {
    let app = TestApp::spawn().await;

    // Create a content type via the admin API.
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "widget",
            "display_name": "Widget",
            "fields": [
                { "name": "label", "kind": "string", "required": true }
            ]
        }))
        .send()
        .await
        .expect("create content type");
    assert_eq!(resp.status(), 201, "content type should be created");

    // Fetch the spec (public, no auth).
    let doc: serde_json::Value = app
        .client
        .get(app.url("/openapi.json"))
        .send()
        .await
        .expect("openapi request")
        .json()
        .await
        .expect("openapi json");

    assert_eq!(doc["openapi"], "3.1.0");
    assert!(doc["paths"]["/api/widget"]["get"].is_object(), "dynamic path present");
    assert!(doc["components"]["schemas"]["Widget"].is_object(), "response schema present");
    assert!(
        doc["components"]["schemas"]["WidgetInput"]["properties"]["id"].is_null(),
        "request schema omits system id"
    );
    assert!(doc["paths"]["/auth/login"]["post"].is_object(), "static path present");
}

#[tokio::test]
async fn docs_ui_served_as_html() {
    let app = TestApp::spawn().await;
    let resp = app.client.get(app.url("/docs")).send().await.expect("docs request");
    assert_eq!(resp.status(), 200);
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap().to_string();
    assert!(ct.starts_with("text/html"), "got content-type {ct}");
    let body = resp.text().await.expect("body");
    assert!(body.contains("swagger-ui"), "page loads Swagger UI");
}
```

- [ ] **Step 2: Run the integration tests**

Run: `cargo test -p ferrum-bin --test openapi`
Expected: PASS (2 tests). Requires Docker for testcontainers (same as other integration tests).

- [ ] **Step 3: Commit**

```bash
git add crates/bin/tests/openapi.rs
git commit -m "test(openapi): integration coverage for runtime spec + UI"
```

---

## Task 9: Docs-disabled toggle test

**Files:**
- Modify: `crates/bin/tests/common/mod.rs`
- Modify: `crates/bin/tests/openapi.rs`

- [ ] **Step 1: Add a spawn variant with docs disabled**

In `crates/bin/tests/common/mod.rs`, refactor `spawn` to delegate to a new
`spawn_with_docs(docs_enabled: bool)`. Add this method to the `impl TestApp`
block, and change `spawn` to call it:

```rust
    pub async fn spawn() -> Self {
        Self::spawn_with_docs(true).await
    }

    pub async fn spawn_with_docs(docs_enabled: bool) -> Self {
```

Then in the body, change the `config: AppConfig { ... }` line `docs_enabled: true,`
to `docs_enabled,` (use the parameter). Everything else in the body is unchanged.

- [ ] **Step 2: Add the toggle test**

Append to `crates/bin/tests/openapi.rs`:

```rust
#[tokio::test]
async fn docs_disabled_returns_404() {
    let app = TestApp::spawn_with_docs(false).await;

    let spec = app.client.get(app.url("/openapi.json")).send().await.expect("spec request");
    assert_eq!(spec.status(), 404, "/openapi.json must 404 when docs disabled");

    let ui = app.client.get(app.url("/docs")).send().await.expect("docs request");
    assert_eq!(ui.status(), 404, "/docs must 404 when docs disabled");
}
```

- [ ] **Step 3: Run all openapi integration tests**

Run: `cargo test -p ferrum-bin --test openapi`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/bin/tests/common/mod.rs crates/bin/tests/openapi.rs
git commit -m "test(openapi): assert routes 404 when docs disabled"
```

---

## Task 10: Document the env vars

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add an env-var section**

In `README.md`, in the configuration/environment-variables section (search for
an existing `FERRUM_` entry such as `FERRUM_PAGE_SIZE_MAX`; place these next
to it, matching the surrounding format). Add:

```markdown
| `FERRUM_DOCS_ENABLED` | `true` | Serve `/openapi.json` and `/docs` (Swagger UI). Set to `false` in production to hide the API schema. |
| `FERRUM_API_VERSION` | `0.1.0` | Reported as `info.version` in the OpenAPI document. |
| `FERRUM_PUBLIC_URL` | `/` | Reported as the OpenAPI `servers[0].url`. |
```

If the README has no env-var table, add a short `## API Documentation` section
describing `GET /docs`, `GET /openapi.json`, and the three env vars in prose.

- [ ] **Step 2: Verify the full test suite still passes**

Run: `cargo test`
Expected: PASS (all crates). 

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs(openapi): document docs env vars and endpoints"
```

---

## Self-Review Notes

- **Spec coverage:** runtime generation (Task 5), `/openapi.json`+`/docs` (Task 6), field mapping table (Task 2), T/TInput split (Task 3), static block (Task 4), public+`docs_enabled` 404 toggle (Tasks 7,9), config fields incl. `api_version`/`public_base_url` (Tasks 1,7), security note via README disable guidance (Task 10), tests incl. runtime-freshness + validity (Tasks 8,9). All spec sections mapped.
- **Type consistency:** `field_to_schema`, `schema_names`, `content_type_schemas`, `content_type_paths`, `static_paths`, `static_components`, `spec::build`, `openapi::router`, `AppConfig.{docs_enabled,api_version,public_base_url}` used identically across tasks.
- **Known verification points flagged inline:** `Cardinality` import path (Task 2), `LocalProvider::new` signature and unit-test feasibility (Task 5) — fall back to integration coverage if `AppState` is impractical to build in a unit test.
```

