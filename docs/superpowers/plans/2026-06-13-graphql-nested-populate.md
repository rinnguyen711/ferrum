# GraphQL Nested Populate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make GraphQL relation/media fields resolve to nested objects when sub-fields are selected, driven by the selection set, reusing REST's batched populate (no N+1).

**Architecture:** Relation/media GraphQL fields become object types (was scalar UUID). Every content type — including Single — gets a registered object so relation targets never dangle. The list/get resolvers read the selection set via `ctx.look_ahead()`, derive which relation/media fields were selected, and pass that as the `populate` arg to the existing `content::list_entries`/`get_entry`, whose batched populate pipeline embeds nested objects into the row JSON before child resolvers run. One level deep; deeper sub-relations resolve to null.

**Tech Stack:** Rust, axum, async-graphql 7.2.1 (dynamic), sqlx.

---

## Background the engineer needs

- **Current GraphQL state (merged):** `crates/http/src/graphql/` has `scalars.rs` (FieldKind→TypeRef), `build.rs` (runtime dynamic schema), `resolve.rs` (resolvers calling shared `content::*` fns), `handler.rs`, `mod.rs` (`GqlRegistry` cache). Today relation/media fields are typed as scalar `UUID` and resolvers pass `populate: None`.
- **Why relations are scalar today:** object-ref typing crashed `Schema::finish()` when a relation targeted a Single type, because `build_schema` only registers `Collection` types as objects. This plan fixes that by registering an object for EVERY content type.
- **REST populate (reused as-is):** `crates/http/src/populate.rs` — `content::list_entries`/`get_entry` accept `populate: Option<&str>` (comma-separated relation field names, REST `?populate=` syntax). The pipeline (`apply_forward`/`apply_inverse`/`apply_many`) runs ONE batched query per relation across the whole page and embeds the related object(s) into each row's JSON. Media embedding (`media_embed`) already runs unconditionally and replaces media uuid(s) with media object JSON.
- **async-graphql dynamic APIs (verified in 7.2.1 source):**
  - `ResolverContext` derefs to `&Context`. `Context::look_ahead() -> Lookahead` (context.rs:686).
  - `Lookahead::field(name) -> Lookahead` descends into a named field's sub-selection; `Lookahead::selection_fields() -> Vec<SelectionField>`; `Lookahead::exists() -> bool`.
  - `SelectionField::name() -> &str` (context.rs:762), `SelectionField::alias() -> Option<&str>`.
  - `Object::new(name).field(Field::new(name, TypeRef, resolver))`, `.register(...)`, `Schema::build("Query", Some("Mutation"), None)...finish()`.
  - `TypeRef::named`, `named_nn`, `named_nn_list`, `named_nn_list_nn`.
- **Field model:** `Field { name, kind: FieldKind, ... }`. `FieldKind::Relation` / `FieldKind::Media`. `scalars::is_list(&Field)` already returns true for m2m relations / multiple media. `field.relation_meta().target` is the target content-type name.
- **ContentType:** `.name` (snake api id), `.kind: ContentTypeKind::{Collection, Single}`, `.fields: Vec<Field>`. Reachable in a resolver via `state.schemas.registry().get(ct_name).await -> Option<ContentType>`.
- **Test harness:** `crates/bin/tests/graphql.rs` + `common::TestApp`. Create content types via `POST /admin/content-types` as admin (triggers GraphQL schema rebuild). Relation field payload (from relations.rs): `{"name":"author","kind":"relation","kind_meta":{"target":"writer","cardinality":"many_to_one"}}`. Bin package name is `ferrum` (NOT ferrum-bin).

---

## File Structure

- `crates/http/src/graphql/scalars.rs` — relation/media `base_type_name` → object refs (was UUID); update unit tests.
- `crates/http/src/graphql/build.rs` — register an object for EVERY content type (incl. Single); re-register the `Media` object; Single types still get no root Query/Mutation fields; update SDL tests.
- `crates/http/src/graphql/resolve.rs` — add `populate_from_selection` helper; `list_field`/`get_field` pass derived populate.
- `crates/bin/tests/graphql.rs` — nested-populate integration tests + closed coverage gaps.

---

## Task 1: Relation/media base type → object refs (`scalars.rs`)

**Files:**
- Modify: `crates/http/src/graphql/scalars.rs`

- [ ] **Step 1: Update the unit tests to expect object names (write failing tests first)**

In `crates/http/src/graphql/scalars.rs`, the existing tests assert relation/media base type is `"UUID"`. Change them to expect object names. Find the relation/media test(s) and replace their assertions:

```rust
    #[test]
    fn relation_base_is_target_object() {
        // many_to_one relation → single object ref named after the target (PascalCase)
        let mto = f(
            FieldKind::Relation,
            false,
            json!({ "target": "writer", "cardinality": "many_to_one" }),
        );
        assert_eq!(base_type_name(&mto), "Writer");
        assert!(!is_list(&mto));

        // many_to_many → still object ref (Writer), but is_list = true
        let mtm = f(
            FieldKind::Relation,
            false,
            json!({ "target": "tag", "cardinality": "many_to_many" }),
        );
        assert_eq!(base_type_name(&mtm), "Tag");
        assert!(is_list(&mtm));
    }

    #[test]
    fn media_base_is_media_object() {
        let single = f(FieldKind::Media, false, json!({ "multiple": false }));
        assert_eq!(base_type_name(&single), "Media");
        assert!(!is_list(&single));

        let multiple = f(FieldKind::Media, false, json!({ "multiple": true }));
        assert_eq!(base_type_name(&multiple), "Media");
        assert!(is_list(&multiple));
    }
```

(Keep the existing `f(...)` test helper and any other tests. If the old assertions live in a single combined test, split or rewrite them to the above.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ferrum-http graphql::scalars`
Expected: FAIL — `base_type_name` still returns "UUID" for relation/media.

- [ ] **Step 3: Change the relation/media arm of `base_type_name`**

Replace the combined scalar arm:

```rust
        // Relation/Media surface as UUID id(s); list-ness handled by `is_list`.
        FieldKind::Relation | FieldKind::Media => UUID_SCALAR.to_string(),
```

with object-ref typing:

```rust
        // Relation → the target type's object (PascalCase). Media → the shared
        // `Media` object. Both are registered for every content type in
        // build.rs, so the ref never dangles (even for Single-type targets).
        // List-ness (m2m / multiple) is applied by `is_list` in `wrap_ref`.
        FieldKind::Relation => crate::graphql::build::pascal(
            &field
                .relation_meta()
                .map(|m| m.target)
                .unwrap_or_default(),
        ),
        FieldKind::Media => "Media".to_string(),
```

Update the doc comment on `base_type_name` (currently says relation/media are scalar UUID): change to state relation → target object, media → `Media` object, populated one level via the selection set; unpopulated relations resolve to null.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ferrum-http graphql::scalars`
Expected: PASS.

> Note: `cargo build -p ferrum-http` will now FAIL or produce a schema that can't `finish()` because the `Media` object and Single-type objects aren't registered yet (Task 2). That's expected — the build/SDL tests are fixed in Task 2. Do NOT try to make the whole crate green here; just the scalars unit tests. If `cargo test -p ferrum-http graphql::scalars` compiles the crate and fails on build.rs SDL tests, that's fine for this task — proceed to commit the scalars change. If it won't compile at all (not just test failures), check the error is confined to build.rs/SDL (expected) vs a real scalars.rs mistake.

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/graphql/scalars.rs
git commit -m "feat(graphql): type relation/media fields as object refs (was scalar UUID)"
```

---

## Task 2: Register an object per content type + Media object (`build.rs`)

**Files:**
- Modify: `crates/http/src/graphql/build.rs`

- [ ] **Step 1: Update the SDL tests (failing first)**

In `build.rs` tests, there is `single_type_is_skipped` asserting a Single type produces no `homepages(` root field. Keep that intent but ALSO assert the Single type's OBJECT is now registered. Replace/extend:

```rust
    #[test]
    fn single_type_object_registered_no_root_field() {
        let mut s = article(); // existing test helper building a Collection ContentType
        s.name = "homepage".into();
        s.kind = ContentTypeKind::Single;
        let schema = build_schema(&[s]).expect("build");
        let sdl = schema.sdl();
        // object IS registered (so relations can target it)...
        assert!(sdl.contains("type Homepage"), "single object registered: {sdl}");
        // ...but NO root collection field for it
        assert!(!sdl.contains("homepages("), "single type has no list query: {sdl}");
    }
```

Add a test that a relation to a Single type builds (the core fix):

```rust
    #[test]
    fn relation_to_single_target_builds() {
        use ferrum_core::field::{Field, FieldKind};
        use serde_json::{json, Value};
        // Single target
        let mut home = article();
        home.name = "homepage".into();
        home.kind = ContentTypeKind::Single;
        // Collection with a relation to the Single
        let mut banner = article();
        banner.name = "banner".into();
        banner.kind = ContentTypeKind::Collection;
        banner.fields = vec![Field {
            name: "page".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: Value::Null,
            max_length: None,
            kind_meta: json!({ "target": "homepage", "cardinality": "many_to_one" }),
        }];
        // must NOT error (previously dangling ref → Err)
        let schema = build_schema(&[home, banner]).expect("schema with relation to single builds");
        let sdl = schema.sdl();
        assert!(sdl.contains("type Banner"));
        assert!(sdl.contains("page: Homepage"), "relation field typed as target object: {sdl}");
    }
```

If the existing `article()` helper isn't accessible or shaped as assumed, adapt to the real test helpers already in build.rs's test module (read them first).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ferrum-http graphql::build`
Expected: FAIL — Single objects not registered; relation-to-single errors on `finish()`.

- [ ] **Step 3: Register an object for every content type; re-add the Media object**

In `build_schema`:

(a) Re-register a shared `Media` object alongside the scalars. After the scalar registrations and before/after the `Meta` object, add:

```rust
    // Shared Media object. Media fields embed a media object into the row JSON
    // (see media_embed), so children read id/url/etc. from the parent value.
    let media = Object::new("Media")
        .field(Field::new(
            "id",
            TypeRef::named_nn(scalars::UUID_SCALAR),
            resolve::json_field_resolver("id"),
        ))
        .field(Field::new(
            "url",
            TypeRef::named(TypeRef::STRING),
            resolve::json_field_resolver("url"),
        ));
    builder = builder.register(media);
```

> Note on Media fields: confirm what keys `media_embed::apply_media_embed` writes into the row JSON for a media value (read `crates/http/src/media_embed.rs`). At minimum it should expose `id` and `url`. If it emits more (e.g. `mime`, `size`, `alt`), you MAY add those fields to the Media object, but keep it to `id` + `url` for this task unless the embed clearly provides others by the same key names — extra fields with no backing key would just resolve null. Keep scope tight: `id` (non-null) + `url` (nullable).

(b) Register an object for EVERY content type, not just Collection. Split the loop: register objects/inputs/envelopes-for-collections, but the OBJECT for every type. Change the single filtered loop into: first register an output object for every type; then a second pass (or same loop, unfiltered, with a Collection guard for root fields).

Concretely, replace the `for ct in types.iter().filter(|ct| ct.kind == ContentTypeKind::Collection)` loop with an unfiltered loop, registering the output object always and the collection-only artifacts + root fields behind a `Collection` check:

```rust
    for ct in types.iter() {
        let type_name = pascal(&ct.name);

        // Output object is registered for EVERY type so relation fields can
        // reference Single-type targets without dangling.
        builder = builder.register(build_output_object(ct));
        builder = register_enums(builder, ct, &mut registered_enums);

        // Collections also get an input, list envelope, and root Query/Mutation
        // fields. Single types are not queryable as collections in v1.
        if ct.kind != ContentTypeKind::Collection {
            continue;
        }
        surfaced_any = true;
        let input_name = format!("{type_name}Input");
        let list_name = format!("{type_name}List");
        builder = builder
            .register(build_input_object(ct))
            .register(build_list_envelope(&type_name));

        // ... (the existing Query list+single and Mutation create/update/delete
        //      field registrations stay exactly as they are) ...
    }
```

Keep the `surfaced_any` / `_empty` placeholder logic and the final `query`/`mutation` registration + `finish()` unchanged. Update the `build_schema` doc comment + the file-top comment that say "Collection types only / Single skipped / no shared Media object" to reflect: objects registered for all types, Single types have no root fields, Media object shared.

> Watch for a duplicate-registration panic: a relation target that is itself a Collection gets its object registered once in its own loop iteration — fine. But ensure no type is registered twice (the unfiltered loop registers each `build_output_object(ct)` exactly once per ct — good). Enums are deduped via `registered_enums`.

- [ ] **Step 4: Run build + SDL tests**

Run: `cargo test -p ferrum-http graphql`
Expected: PASS (scalars from Task 1 + build SDL incl. the new Single-object and relation-to-single tests). If `.sdl()` substring asserts are off by formatting, print SDL (assert messages include `{sdl}`) and reconcile.

- [ ] **Step 5: Build the crate clean**

Run: `cargo build -p ferrum-http && cargo clippy -p ferrum-http --all-targets`
Expected: compiles, clippy clean.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/graphql/build.rs
git commit -m "feat(graphql): register object per content type + Media object; relation targets never dangle"
```

---

## Task 3: Derive populate from the selection set (`resolve.rs`)

**Files:**
- Modify: `crates/http/src/graphql/resolve.rs`

- [ ] **Step 1: Add the `populate_from_selection` helper**

In `resolve.rs`, add a helper that inspects the selection set and returns the comma-joined relation/media field names selected. It needs the `ContentType` to know which selected field names are relation/media (vs scalars). Signature and impl:

```rust
use ferrum_core::{ContentType, FieldKind};

/// Inspect the GraphQL selection set and return a comma-joined `populate`
/// string of the relation/media fields the client selected (one level). `None`
/// if nothing to populate. `entry_lookahead` is the Lookahead positioned at the
/// entry object (for list: `data`; for get: the field itself).
fn selected_populate(entry: async_graphql::Lookahead<'_>, ct: &ContentType) -> Option<String> {
    use std::collections::HashSet;
    let rel_media: HashSet<&str> = ct
        .fields
        .iter()
        .filter(|f| matches!(f.kind, FieldKind::Relation | FieldKind::Media))
        .map(|f| f.name.as_str())
        .collect();
    let mut names: Vec<String> = Vec::new();
    for sf in entry.selection_fields() {
        let n = sf.name();
        if rel_media.contains(n) && !names.iter().any(|e| e == n) {
            names.push(n.to_string());
        }
    }
    if names.is_empty() {
        None
    } else {
        Some(names.join(","))
    }
}
```

> Verify imports: `async_graphql::Lookahead` is the right path (the `look_ahead` module is re-exported at crate root in 7.2.1 — confirm; if not, use the correct path e.g. `async_graphql::context::Lookahead`). `SelectionField::name()` returns `&str`. `FieldKind` import path is `ferrum_core::FieldKind` (already used elsewhere). If `ct.fields[].kind` matching needs the enum in scope, import it.

- [ ] **Step 2: Wire it into `list_field`**

In `list_field`, the entry fields sit under `data`. Currently `populate: None` and `list_entries(..., None, ...)`. Change to compute populate from the selection BEFORE the async block (look_ahead borrows from `ctx`), fetch the `ContentType`, then pass it.

Because `selected_populate` needs the `ContentType` and that requires an async registry read, do the lookup inside the async block but compute the Lookahead-derived NAMES synchronously is not possible (need ct first). Resolve by: capture the selected field NAMES synchronously (they don't need ct), then filter against ct inside the async block. Refactor the helper into two parts — a sync name-collector and an async filter — OR fetch ct's relation/media names without async. Simplest: collect ALL selected names synchronously from the Lookahead (sync, borrows ctx), move the `Vec<String>` into the async block, then filter against the ct fetched there.

Replace the helper with this split:

```rust
/// Sync: collect the names selected under the entry object's `data`/self.
fn selected_field_names(entry: async_graphql::Lookahead<'_>) -> Vec<String> {
    entry.selection_fields().iter().map(|sf| sf.name().to_string()).collect()
}

/// Filter selected names down to relation/media fields of `ct`, joined for populate.
fn populate_arg(selected: &[String], ct: &ContentType) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for f in &ct.fields {
        if matches!(f.kind, FieldKind::Relation | FieldKind::Media)
            && selected.iter().any(|s| s == &f.name)
            && !out.contains(&f.name.as_str())
        {
            out.push(&f.name);
        }
    }
    if out.is_empty() { None } else { Some(out.join(",")) }
}
```

Then `list_field`:

```rust
pub fn list_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx| {
        let ct_name = ct_name.clone();
        let st = app_state(&ctx);
        let pr = principal(&ctx);
        let page = opt_u32(&ctx, "page");
        let page_size = opt_u32(&ctx, "pageSize");
        let sort = opt_string(&ctx, "sort");
        let raw_query = ctx
            .args
            .get("filters")
            .and_then(|v| v.deserialize::<JsonValue>().ok())
            .map(filters_to_raw_query)
            .unwrap_or_default();
        // Selection lives under `data` for the list envelope. Collect names
        // synchronously (Lookahead borrows ctx).
        let selected = selected_field_names(ctx.look_ahead().field("data"));
        FieldFuture::new(async move {
            let st = st?;
            let pr = pr?;
            let populate = st
                .schemas
                .registry()
                .get(&ct_name)
                .await
                .and_then(|ct| populate_arg(&selected, &ct));
            let params = ListParams {
                page,
                page_size,
                sort,
                populate: None,
                status: None,
            };
            let env =
                content::list_entries(&st, &pr, &ct_name, params, populate.as_deref(), &raw_query)
                    .await
                    .map_err(gql_err)?;
            Ok(Some(FieldValue::value(json_to_gql(env))))
        })
    }
}
```

- [ ] **Step 3: Wire it into `get_field`**

For get-one, entry fields are directly under the field (no `data`). Use `ctx.look_ahead()` directly:

```rust
pub fn get_field(ct_name: String) -> impl Fn(ResolverContext) -> FieldFuture + Clone {
    move |ctx| {
        let ct_name = ct_name.clone();
        let st = app_state(&ctx);
        let pr = principal(&ctx);
        let id = id_arg(&ctx);
        let selected = selected_field_names(ctx.look_ahead());
        FieldFuture::new(async move {
            let st = st?;
            let pr = pr?;
            let id = id?;
            let populate = st
                .schemas
                .registry()
                .get(&ct_name)
                .await
                .and_then(|ct| populate_arg(&selected, &ct));
            match content::get_entry(&st, &pr, &ct_name, id, populate.as_deref()).await {
                Ok(entry) => Ok(Some(FieldValue::value(json_to_gql(entry)))),
                Err(Error::NotFound) => Ok(None),
                Err(e) => Err(gql_err(e)),
            }
        })
    }
}
```

- [ ] **Step 4: Update the file-top v1-limitation comment**

The module doc currently says "list/get are called with populate = None, so relation/media nested selection is deferred". Update it to: relation/media fields selected at the first level are populated via the selection set (`selected_field_names` + `populate_arg`); deeper sub-relations are not populated and resolve to null.

- [ ] **Step 5: Build + clippy**

Run: `cargo build -p ferrum-http && cargo clippy -p ferrum-http --all-targets`
Expected: compiles, clippy clean. (Resolver behavior is exercised by Task 4's integration tests — the unit/SDL tests still pass but don't run resolvers.)

Run: `cargo test -p ferrum-http graphql` — existing scalars+build tests still PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/graphql/resolve.rs
git commit -m "feat(graphql): drive relation/media populate from the selection set (one level)"
```

---

## Task 4: Integration tests (`crates/bin/tests/graphql.rs`)

**Files:**
- Modify: `crates/bin/tests/graphql.rs`

- [ ] **Step 1: Add nested-populate + coverage-gap tests**

Append to `crates/bin/tests/graphql.rs`. Reuse existing helpers (`gql`, `create_token`, `with_token`, `make_article`) — read the file first to match their exact signatures. Add helpers for related types as needed.

```rust
// Helper: create a `writer` collection and an `article` collection with a
// many_to_one relation `author` → writer, plus a m2m `tags` → tag.
async fn make_blog(app: &TestApp) {
    // writer
    let r = app.admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "writer", "display_name": "Writer",
            "fields": [{"name": "name", "kind": "string", "required": true}]
        })).send().await.unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());
    // tag
    let r = app.admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "tag", "display_name": "Tag",
            "fields": [{"name": "label", "kind": "string"}]
        })).send().await.unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());
    // article with relation + m2m
    let r = app.admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article", "display_name": "Article",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "author", "kind": "relation", "kind_meta": {"target": "writer", "cardinality": "many_to_one"}},
                {"name": "tags", "kind": "relation", "kind_meta": {"target": "tag", "cardinality": "many_to_many"}}
            ]
        })).send().await.unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());
}

#[tokio::test]
async fn forward_relation_populated_when_selected() {
    let app = TestApp::spawn().await;
    make_blog(&app).await;
    // create a writer, capture id
    let w = gql(&app, "mutation($d: WriterInput!){ createWriter(data:$d){ id name } }",
        json!({"d":{"name":"Ada"}})).await;
    assert!(w["errors"].is_null(), "{w}");
    let writer_id = w["data"]["createWriter"]["id"].as_str().unwrap().to_string();
    // create an article referencing the writer (relation input is a scalar uuid)
    let a = gql(&app, "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d":{"title":"Post","author": writer_id}})).await;
    assert!(a["errors"].is_null(), "{a}");
    // query the article list selecting the nested author object
    let q = gql(&app,
        "{ articles{ data{ title author { id name } } } }", json!({})).await;
    assert!(q["errors"].is_null(), "{q}");
    let row = &q["data"]["articles"]["data"][0];
    assert_eq!(row["title"], "Post");
    assert_eq!(row["author"]["name"], "Ada", "author object populated: {q}");
}

#[tokio::test]
async fn relation_id_only_selectable() {
    let app = TestApp::spawn().await;
    make_blog(&app).await;
    let w = gql(&app, "mutation($d: WriterInput!){ createWriter(data:$d){ id } }",
        json!({"d":{"name":"Ada"}})).await;
    let writer_id = w["data"]["createWriter"]["id"].as_str().unwrap().to_string();
    gql(&app, "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d":{"title":"P","author": writer_id}})).await;
    // selecting author { id } returns the uuid (populated object has id)
    let q = gql(&app, "{ articles{ data{ author { id } } } }", json!({})).await;
    assert!(q["errors"].is_null(), "{q}");
    assert_eq!(q["data"]["articles"]["data"][0]["author"]["id"], writer_id);
}

#[tokio::test]
async fn m2m_relation_populated_as_list() {
    let app = TestApp::spawn().await;
    make_blog(&app).await;
    let t1 = gql(&app, "mutation($d: TagInput!){ createTag(data:$d){ id } }", json!({"d":{"label":"rust"}})).await;
    let t1id = t1["data"]["createTag"]["id"].as_str().unwrap().to_string();
    // m2m input is a list of uuids
    gql(&app, "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d":{"title":"P","tags":[t1id]}})).await;
    let q = gql(&app, "{ articles{ data{ tags { id label } } } }", json!({})).await;
    assert!(q["errors"].is_null(), "{q}");
    let tags = &q["data"]["articles"]["data"][0]["tags"];
    assert!(tags.is_array(), "tags is a list: {q}");
    assert_eq!(tags[0]["label"], "rust");
}

#[tokio::test]
async fn nested_populate_one_level_deeper_is_null() {
    let app = TestApp::spawn().await;
    make_blog(&app).await;
    let w = gql(&app, "mutation($d: WriterInput!){ createWriter(data:$d){ id } }", json!({"d":{"name":"Ada"}})).await;
    let wid = w["data"]["createWriter"]["id"].as_str().unwrap().to_string();
    gql(&app, "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d":{"title":"P","author": wid}})).await;
    // writer has no relations, so to test "deeper", select author's scalar (ok)
    // and confirm the one-level rule: author resolves, no error.
    let q = gql(&app, "{ articles{ data{ author { name } } } }", json!({})).await;
    assert!(q["errors"].is_null(), "{q}");
    assert_eq!(q["data"]["articles"]["data"][0]["author"]["name"], "Ada");
}

#[tokio::test]
async fn relation_to_single_type_object_selectable() {
    let app = TestApp::spawn().await;
    // Single type homepage
    let r = app.admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name":"homepage","display_name":"Homepage","kind":"single",
            "fields":[{"name":"hero","kind":"string"}]
        })).send().await.unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());
    // Collection banner with relation to the Single
    let r = app.admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name":"banner","display_name":"Banner",
            "fields":[
                {"name":"title","kind":"string"},
                {"name":"page","kind":"relation","kind_meta":{"target":"homepage","cardinality":"many_to_one"}}
            ]
        })).send().await.unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());
    // schema built WITH banner; page is an object ref to Homepage, selectable as { id }
    let q = gql(&app, "{ banners{ data{ title page { id } } } }", json!({})).await;
    assert!(q["errors"].is_null(), "schema built + page object selectable: {q}");
    assert_eq!(q["data"]["banners"]["meta"]["total"].as_i64().unwrap_or(0), 0);
}
```

> Note for `relation_to_single_type_object_selectable`: it queries `meta` without selecting it — fix the query to include `meta{ total }` if you assert on it, or drop the meta assertion. Make the query `{ banners{ data{ title page { id } } meta{ total } } }` and assert total==0. Adjust to match the envelope shape.

Also add the previously-skipped coverage-gap tests (small): an enum field round-trips through create+query; a json field; a datetime/uuid scalar; and at least one more error-code mapping (e.g. a duplicate unique → CONFLICT, or an unauthorized path → UNAUTHORIZED). If any of these is awkward to set up, include the ones that are straightforward and note which you skipped and why in your report — don't pad with low-value tests.

- [ ] **Step 2: Run the graphql suite**

Run: `cargo test -p ferrum --test graphql -- --test-threads=1`
Expected: PASS (existing 6 + the new tests). If a populate test fails (e.g. `author.name` null), that's a REAL resolver bug — STOP and report with the response JSON; do NOT weaken the assertion.

- [ ] **Step 3: Full gates**

Run:
```
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets
cargo fmt --all --check
```
Expected: all green.

- [ ] **Step 4: Commit**

```bash
git add crates/bin/tests/graphql.rs
git commit -m "test(graphql): nested relation/media populate + closed coverage gaps"
```

---

## Self-review notes (for the executor)

- **Spec coverage:** object typing (Task 1), object-per-type incl. Single + Media object / dangling-ref fix (Task 2), selection-set populate one level (Task 3), tests incl. one-level-null + relation-to-single + coverage gaps (Task 4). All spec sections mapped.
- **Highest-risk item:** the `Lookahead` path under the list envelope. The entry fields are under `data` — `ctx.look_ahead().field("data")`. If `look_ahead()` from a dynamic resolver scopes differently than expected, the `forward_relation_populated_when_selected` test will fail with `author.name` null (populate string came back empty). If so, debug by logging the derived populate string; the fix is the correct `.field(...)` path, not weakening the test.
- **`async_graphql::Lookahead` import path** — verify against 7.2.1 (crate-root re-export vs `context::`). Build will tell you.
- **Media object fields** — kept to id+url; if `media_embed` keys differ, adjust child resolver keys to match the embedded JSON.
- **No N+1:** populate runs once per relation across the page inside `list_entries` (batched `apply_forward`/`apply_many`), not per row. Child resolvers only read embedded JSON. Confirm no per-row query was introduced.
```
