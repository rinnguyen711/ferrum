//! Integration tests — first real exercise of the GraphQL resolvers.
//! All prior tasks were compile-checked + SDL-tested only, so these tests
//! catch resolver mis-wiring: list-array propagation, mutations, error-code
//! mapping, live schema rebuild on content-type CRUD, and authz denial.

mod common;
use common::TestApp;
use serde_json::{json, Value};

/// Create the `article` content type as admin. This POST triggers the
/// GraphQL schema rebuild (Task 8 hook), so `article`/`articles`/
/// `createArticle` exist afterwards.
async fn make_article(app: &TestApp) {
    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article", "display_name": "Article",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "views", "kind": "integer"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());
}

/// Create `n` articles titled "a{i}". Returns nothing; entries exist after.
async fn seed_articles(app: &TestApp, n: usize) {
    for i in 0..n {
        let body = gql(
            app,
            "mutation($t:String!){ createArticle(data:{title:$t}){ id } }",
            json!({ "t": format!("a{i:03}") }),
        )
        .await;
        assert!(
            body["data"]["createArticle"]["id"].is_string(),
            "seed failed: {body}"
        );
    }
}

/// POST a graphql op as the admin; assert HTTP 200; return parsed body.
async fn gql(app: &TestApp, query: &str, variables: Value) -> Value {
    let r = app
        .admin(app.client.post(app.url("/api/graphql")))
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "{}", r.text().await.unwrap());
    r.json().await.unwrap()
}

async fn create_token(app: &TestApp, scopes: &[&str]) -> String {
    let resp = app
        .admin(app.client.post(app.url("/api/admin/tokens")))
        .json(&json!({ "name": "t", "scopes": scopes }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let v: Value = resp.json().await.unwrap();
    v["token"].as_str().unwrap().to_string()
}

fn with_token(builder: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    builder.header("authorization", format!("Bearer {token}"))
}

#[tokio::test]
async fn create_query_update_delete_roundtrip() {
    let app = TestApp::spawn().await;
    make_article(&app).await;

    // create
    let body = gql(
        &app,
        "mutation($d: ArticleInput!){ createArticle(data:$d){ id title views } }",
        json!({"d": {"title": "Hello", "views": 3}}),
    )
    .await;
    assert!(body["errors"].is_null(), "{body}");
    let id = body["data"]["createArticle"]["id"]
        .as_str()
        .expect("createArticle.id")
        .to_string();
    assert_eq!(body["data"]["createArticle"]["title"], "Hello", "{body}");

    // get_one
    let body = gql(
        &app,
        "query($id: UUID!){ article(id:$id){ title views } }",
        json!({"id": id}),
    )
    .await;
    assert_eq!(body["data"]["article"]["title"], "Hello", "{body}");
    assert_eq!(body["data"]["article"]["views"], 3, "{body}");

    // list — CRITICAL list-propagation check. If the data array doesn't
    // resolve element fields, data[0].title will be null/missing.
    let body = gql(
        &app,
        "{ articles(page:1,pageSize:10,sort:\"title:asc\"){ data{ id title } meta{ total page pageSize } } }",
        json!({}),
    )
    .await;
    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(body["data"]["articles"]["meta"]["total"], 1, "{body}");
    assert_eq!(
        body["data"]["articles"]["data"][0]["title"], "Hello",
        "list-array propagation failed: {body}"
    );

    // update
    let body = gql(
        &app,
        "mutation($id: UUID!, $d: ArticleInput!){ updateArticle(id:$id,data:$d){ title views } }",
        json!({"id": id, "d": {"title": "Hi2", "views": 9}}),
    )
    .await;
    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(body["data"]["updateArticle"]["title"], "Hi2", "{body}");

    // delete
    let body = gql(
        &app,
        "mutation($id: UUID!){ deleteArticle(id:$id) }",
        json!({"id": id}),
    )
    .await;
    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(body["data"]["deleteArticle"], true, "{body}");

    // gone
    let body = gql(
        &app,
        "query($id: UUID!){ article(id:$id){ title } }",
        json!({"id": id}),
    )
    .await;
    assert!(body["data"]["article"].is_null(), "{body}");
}

#[tokio::test]
async fn update_missing_returns_not_found_code() {
    let app = TestApp::spawn().await;
    make_article(&app).await;
    let nil = "00000000-0000-0000-0000-000000000000";

    // get missing -> null, no error
    let body = gql(
        &app,
        "query($id: UUID!){ article(id:$id){ title } }",
        json!({"id": nil}),
    )
    .await;
    assert!(body["data"]["article"].is_null(), "{body}");
    assert!(body["errors"].is_null(), "{body}");

    // update missing -> NOT_FOUND
    let body = gql(
        &app,
        "mutation($id: UUID!){ updateArticle(id:$id,data:{title:\"x\"}){ title } }",
        json!({"id": nil}),
    )
    .await;
    assert_eq!(
        body["errors"][0]["extensions"]["code"], "NOT_FOUND",
        "{body}"
    );
}

#[tokio::test]
async fn filter_narrows_results() {
    let app = TestApp::spawn().await;
    make_article(&app).await;

    for title in ["alpha", "beta", "alphabet"] {
        let body = gql(
            &app,
            "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
            json!({"d": {"title": title}}),
        )
        .await;
        assert!(body["errors"].is_null(), "{body}");
    }

    let body = gql(
        &app,
        "query($f: JSON){ articles(filters:$f){ meta{ total } data{ title } } }",
        json!({"f": {"title": {"$containsi": "alpha"}}}),
    )
    .await;
    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(body["data"]["articles"]["meta"]["total"], 2, "{body}");
}

#[tokio::test]
async fn schema_reflects_new_type_without_restart() {
    let app = TestApp::spawn().await;
    // Before make_article the schema has no `articles`. Creating the type at
    // runtime rebuilds the schema (Task 8 hook) — no restart.
    make_article(&app).await;

    let body = gql(
        &app,
        "{ articles(page:1,pageSize:5){ meta{ total } } }",
        json!({}),
    )
    .await;
    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(body["data"]["articles"]["meta"]["total"], 0, "{body}");
}

#[tokio::test]
async fn relation_to_single_type_does_not_break_schema() {
    // Regression: a relation whose target is a Single type used to type the
    // field as an object ref to a type the schema never registers (Singles are
    // excluded from v1), producing a dangling type ref → Schema::finish() Err.
    // That froze the GraphQL schema on rebuild, so the `banner` type (and its
    // `banners` query) would never appear. The fix registers an object for
    // EVERY content type (incl. Single), so the relation field types as a valid
    // object ref, the schema always builds, and the new type is selectable.
    let app = TestApp::spawn().await;

    // Single content type — a valid relation target whose REST validation only
    // checks existence, not kind.
    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "homepage",
            "display_name": "Homepage",
            "kind": "single",
            "fields": [{"name": "hero", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    // Collection with a many_to_one relation pointing at the Single type. This
    // create triggers the GraphQL rebuild — the trigger for the bug.
    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "banner",
            "display_name": "Banner",
            "fields": [
                {"name": "title", "kind": "string"},
                {
                    "name": "page",
                    "kind": "relation",
                    "kind_meta": {"target": "homepage", "cardinality": "many_to_one"}
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    // Proof: the schema rebuilt WITH `banner`, and the relation field `page`
    // (now an object ref to the Single-type `Homepage`) is selectable. Before
    // the fix the rebuild errored and froze the old schema → `banners` would be
    // an unknown field.
    let body = gql(
        &app,
        "{ banners(page:1,pageSize:5){ meta{ total } data{ title page { id } } } }",
        json!({}),
    )
    .await;
    assert!(
        body["errors"].is_null(),
        "schema froze / page not selectable: {body}"
    );
    assert_eq!(body["data"]["banners"]["meta"]["total"], 0, "{body}");
    assert!(body["data"]["banners"]["data"].is_array(), "{body}");
}

#[tokio::test]
async fn mutation_denied_for_read_only_token() {
    let app = TestApp::spawn().await;
    make_article(&app).await;
    let token = create_token(&app, &["content:read"]).await;

    // createArticle with a read-only token -> GraphQL FORBIDDEN error.
    // HTTP status is still 200 (errors carried in body), so do it inline.
    let r = with_token(app.client.post(app.url("/api/graphql")), &token)
        .json(&json!({
            "query": "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
            "variables": {"d": {"title": "x"}}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "{}", r.text().await.unwrap());
    let body: Value = r.json().await.unwrap();
    assert_eq!(
        body["errors"][0]["extensions"]["code"], "FORBIDDEN",
        "{body}"
    );

    // sanity: a content:read token CAN query
    let r = with_token(app.client.post(app.url("/api/graphql")), &token)
        .json(&json!({
            "query": "{ articles(page:1,pageSize:5){ meta{ total } } }",
            "variables": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "{}", r.text().await.unwrap());
    let body: Value = r.json().await.unwrap();
    assert!(body["errors"].is_null(), "{body}");
}

// ---------------------------------------------------------------------------
// Nested relation/media populate (Task 4) — FIRST real exercise of selection-
// set-driven populate. These prove relation fields resolve to nested objects
// when sub-fields are selected (was scalar UUID before this plan).
// ---------------------------------------------------------------------------

/// Create a `writer` collection, a `tag` collection, and an `article`
/// collection with a many_to_one relation `author` → writer plus a
/// many_to_many `tags` → tag. The final POST rebuilds the GraphQL schema.
async fn make_blog(app: &TestApp) {
    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "writer", "display_name": "Writer",
            "fields": [{"name": "name", "kind": "string", "required": true}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "tag", "display_name": "Tag",
            "fields": [{"name": "label", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article", "display_name": "Article",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "author", "kind": "relation", "kind_meta": {"target": "writer", "cardinality": "many_to_one"}},
                {"name": "tags", "kind": "relation", "kind_meta": {"target": "tag", "cardinality": "many_to_many"}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());
}

/// THE proof: a forward (m2o) relation resolves to a nested object whose
/// scalar (`author.name`) is populated when selected under `data`.
#[tokio::test]
async fn forward_relation_populated_when_selected() {
    let app = TestApp::spawn().await;
    make_blog(&app).await;

    let w = gql(
        &app,
        "mutation($d: WriterInput!){ createWriter(data:$d){ id name } }",
        json!({"d": {"name": "Ada"}}),
    )
    .await;
    assert!(w["errors"].is_null(), "{w}");
    let writer_id = w["data"]["createWriter"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let a = gql(
        &app,
        "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d": {"title": "Post", "author": writer_id}}),
    )
    .await;
    assert!(a["errors"].is_null(), "{a}");

    let q = gql(
        &app,
        "{ articles{ data{ title author { id name } } } }",
        json!({}),
    )
    .await;
    assert!(q["errors"].is_null(), "{q}");
    let row = &q["data"]["articles"]["data"][0];
    assert_eq!(row["title"], "Post", "{q}");
    assert_eq!(row["author"]["name"], "Ada", "author object populated: {q}");
}

/// Selecting only `author { id }` resolves to the target writer's uuid.
#[tokio::test]
async fn relation_id_only_selectable() {
    let app = TestApp::spawn().await;
    make_blog(&app).await;

    let w = gql(
        &app,
        "mutation($d: WriterInput!){ createWriter(data:$d){ id } }",
        json!({"d": {"name": "Ada"}}),
    )
    .await;
    let writer_id = w["data"]["createWriter"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    gql(
        &app,
        "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d": {"title": "P", "author": writer_id.clone()}}),
    )
    .await;

    let q = gql(&app, "{ articles{ data{ author { id } } } }", json!({})).await;
    assert!(q["errors"].is_null(), "{q}");
    assert_eq!(
        q["data"]["articles"]["data"][0]["author"]["id"], writer_id,
        "{q}"
    );
}

/// A many_to_many relation populates to a list of nested objects.
#[tokio::test]
async fn m2m_relation_populated_as_list() {
    let app = TestApp::spawn().await;
    make_blog(&app).await;

    let t1 = gql(
        &app,
        "mutation($d: TagInput!){ createTag(data:$d){ id } }",
        json!({"d": {"label": "rust"}}),
    )
    .await;
    assert!(t1["errors"].is_null(), "{t1}");
    let t1id = t1["data"]["createTag"]["id"].as_str().unwrap().to_string();

    let a = gql(
        &app,
        "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d": {"title": "P", "tags": [t1id]}}),
    )
    .await;
    assert!(a["errors"].is_null(), "{a}");

    let q = gql(&app, "{ articles{ data{ tags { id label } } } }", json!({})).await;
    assert!(q["errors"].is_null(), "{q}");
    let tags = &q["data"]["articles"]["data"][0]["tags"];
    assert!(tags.is_array(), "tags is a list: {q}");
    assert_eq!(tags[0]["label"], "rust", "{q}");
}

/// One-level populate: the selected relation resolves with no error. (The
/// target `writer` has no relations of its own, so this honestly only proves
/// depth-1 resolves — not a true depth-2-null assertion.)
#[tokio::test]
async fn relation_resolves_one_level() {
    let app = TestApp::spawn().await;
    make_blog(&app).await;

    let w = gql(
        &app,
        "mutation($d: WriterInput!){ createWriter(data:$d){ id } }",
        json!({"d": {"name": "Ada"}}),
    )
    .await;
    let wid = w["data"]["createWriter"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    gql(
        &app,
        "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d": {"title": "P", "author": wid}}),
    )
    .await;

    let q = gql(&app, "{ articles{ data{ author { name } } } }", json!({})).await;
    assert!(q["errors"].is_null(), "{q}");
    assert_eq!(
        q["data"]["articles"]["data"][0]["author"]["name"], "Ada",
        "{q}"
    );
}

/// A relation whose target is a Single type builds AND the relation field is
/// object-selectable (`page { id }`). No banners created → total 0.
#[tokio::test]
async fn relation_to_single_type_object_selectable() {
    let app = TestApp::spawn().await;

    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "homepage", "display_name": "Homepage", "kind": "single",
            "fields": [{"name": "hero", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "banner", "display_name": "Banner",
            "fields": [
                {"name": "title", "kind": "string"},
                {"name": "page", "kind": "relation", "kind_meta": {"target": "homepage", "cardinality": "many_to_one"}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    let q = gql(
        &app,
        "{ banners{ data{ title page { id } } meta{ total } } }",
        json!({}),
    )
    .await;
    assert!(
        q["errors"].is_null(),
        "schema built + page object selectable: {q}"
    );
    assert_eq!(q["data"]["banners"]["meta"]["total"], 0, "{q}");
}

/// The canonical 1x1 PNG used by the media tests.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// Upload the tiny PNG via the admin multipart endpoint and return the asset id.
async fn upload_asset(app: &TestApp, filename: &str) -> String {
    let part = reqwest::multipart::Part::bytes(TINY_PNG.to_vec())
        .file_name(filename.to_string())
        .mime_str("application/octet-stream")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let resp = app
        .admin(app.client.post(app.url("/admin/media/assets")))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

/// A single media field populates to a nested `Media` object whose scalars
/// (`id`/`file_name`/`mime_type`) resolve when selected under `data`. The
/// media input is a scalar uuid, exactly like a forward relation.
#[tokio::test]
async fn media_field_populated() {
    let app = TestApp::spawn().await;

    // Content type `doc` with a single media field `cover`. This POST rebuilds
    // the GraphQL schema, so `docs` / `DocInput` exist afterwards.
    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "doc", "display_name": "Doc",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "cover", "kind": "media", "kind_meta": {"multiple": false}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    // Upload an asset and capture its id.
    let asset_id = upload_asset(&app, "cover.png").await;

    // Create a doc whose `cover` references the asset (scalar uuid input).
    let c = gql(
        &app,
        "mutation($d: DocInput!){ createDoc(data:$d){ id } }",
        json!({"d": {"title": "P", "cover": asset_id.clone()}}),
    )
    .await;
    assert!(c["errors"].is_null(), "{c}");

    // Query selecting the media object's fields — proves the media field
    // populates to a Media object, not a scalar uuid.
    let q = gql(
        &app,
        "{ docs{ data{ title cover { id file_name mime_type } } } }",
        json!({}),
    )
    .await;
    assert!(q["errors"].is_null(), "{q}");
    let row = &q["data"]["docs"]["data"][0];
    assert_eq!(row["title"], "P", "{q}");
    assert_eq!(row["cover"]["id"], asset_id, "media object populated: {q}");
    assert_eq!(row["cover"]["file_name"], "cover.png", "{q}");
    assert_eq!(row["cover"]["mime_type"], "image/png", "{q}");
}

// ---------------------------------------------------------------------------
// Coverage-gap tests: field-kind round-trips + an extra error-code mapping.
// ---------------------------------------------------------------------------

/// An enum field round-trips through create + query. The GraphQL output type
/// is a registered `<Field>Enum`; selecting it returns the enum member.
#[tokio::test]
async fn enum_field_round_trips() {
    let app = TestApp::spawn().await;
    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post", "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "status", "kind": "enum", "kind_meta": {"values": ["draft", "published"]}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    let c = gql(
        &app,
        "mutation($d: PostInput!){ createPost(data:$d){ id status } }",
        json!({"d": {"title": "x", "status": "published"}}),
    )
    .await;
    assert!(c["errors"].is_null(), "{c}");
    assert_eq!(c["data"]["createPost"]["status"], "published", "{c}");

    let q = gql(&app, "{ posts{ data{ title status } } }", json!({})).await;
    assert!(q["errors"].is_null(), "{q}");
    assert_eq!(q["data"]["posts"]["data"][0]["status"], "published", "{q}");
}

/// A json field round-trips a nested structure through create + query.
#[tokio::test]
async fn json_field_round_trips() {
    let app = TestApp::spawn().await;
    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "doc", "display_name": "Doc",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "meta", "kind": "json", "kind_meta": {}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    let payload = json!({"a": [1, 2, {"b": true}], "nested": {"flag": false}});
    let c = gql(
        &app,
        "mutation($d: DocInput!){ createDoc(data:$d){ id meta } }",
        json!({"d": {"title": "x", "meta": payload.clone()}}),
    )
    .await;
    assert!(c["errors"].is_null(), "{c}");
    assert_eq!(c["data"]["createDoc"]["meta"], payload, "{c}");

    let q = gql(&app, "{ docs{ data{ meta } } }", json!({})).await;
    assert!(q["errors"].is_null(), "{q}");
    assert_eq!(q["data"]["docs"]["data"][0]["meta"], payload, "{q}");
}

/// The DateTime + UUID custom scalars round-trip via the always-present system
/// fields `id` (UUID) and `created_at` (DateTime).
#[tokio::test]
async fn datetime_uuid_scalars_round_trip() {
    let app = TestApp::spawn().await;
    make_article(&app).await;

    let c = gql(
        &app,
        "mutation($d: ArticleInput!){ createArticle(data:$d){ id created_at } }",
        json!({"d": {"title": "x"}}),
    )
    .await;
    assert!(c["errors"].is_null(), "{c}");
    let id = c["data"]["createArticle"]["id"].as_str().unwrap();
    // UUID scalar serializes to a uuid-shaped string.
    assert!(
        uuid::Uuid::parse_str(id).is_ok(),
        "id is a UUID scalar: {c}"
    );
    // DateTime scalar serializes to a non-empty rfc3339 string.
    let created = c["data"]["createArticle"]["created_at"].as_str().unwrap();
    assert!(
        chrono::DateTime::parse_from_rfc3339(created).is_ok(),
        "created_at is a DateTime scalar: {c}"
    );
}

/// Unique-constraint violation maps to the `CONFLICT` error code (beyond the
/// existing NOT_FOUND / FORBIDDEN coverage).
#[tokio::test]
async fn unique_violation_maps_to_conflict_code() {
    let app = TestApp::spawn().await;
    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article", "display_name": "Article",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "slug", "kind": "slug", "unique": true, "kind_meta": {}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    let a = gql(
        &app,
        "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d": {"title": "one", "slug": "dup"}}),
    )
    .await;
    assert!(a["errors"].is_null(), "{a}");

    let b = gql(
        &app,
        "mutation($d: ArticleInput!){ createArticle(data:$d){ id } }",
        json!({"d": {"title": "two", "slug": "dup"}}),
    )
    .await;
    assert_eq!(b["errors"][0]["extensions"]["code"], "CONFLICT", "{b}");
}

// ---------------------------------------------------------------------------
// Cursor pagination (Tasks 1-2) — first exercise of cursor:"first" + nextCursor.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn graphql_cursor_first_returns_page_and_next_cursor() {
    let app = TestApp::spawn().await;
    make_article(&app).await;
    seed_articles(&app, 5).await;

    let body = gql(
        &app,
        "query{ articles(cursor:\"first\", pageSize:2, sort:\"title:asc\") \
            { data{ title } meta{ nextCursor } } }",
        json!({}),
    )
    .await;

    let data = &body["data"]["articles"]["data"];
    assert_eq!(data.as_array().unwrap().len(), 2, "{body}");
    assert!(
        body["data"]["articles"]["meta"]["nextCursor"].is_string(),
        "full page should carry a nextCursor token: {body}"
    );
}

#[tokio::test]
async fn graphql_cursor_paginates_to_end_without_overlap() {
    let app = TestApp::spawn().await;
    make_article(&app).await;
    seed_articles(&app, 5).await;

    // Page 1
    let p1 = gql(
        &app,
        "query{ articles(cursor:\"first\", pageSize:2, sort:\"title:asc\") \
            { data{ title } meta{ nextCursor } } }",
        json!({}),
    )
    .await;
    let tok1 = p1["data"]["articles"]["meta"]["nextCursor"]
        .as_str()
        .unwrap()
        .to_string();
    let t1: Vec<String> = p1["data"]["articles"]["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap().to_string())
        .collect();

    // Page 2 — pass the token back via a variable (avoids quoting issues)
    let p2 = gql(
        &app,
        "query($c:String){ articles(cursor:$c, pageSize:2, sort:\"title:asc\") \
            { data{ title } meta{ nextCursor } } }",
        json!({ "c": tok1 }),
    )
    .await;
    let t2: Vec<String> = p2["data"]["articles"]["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap().to_string())
        .collect();

    // No overlap between page 1 and page 2.
    for title in &t1 {
        assert!(!t2.contains(title), "page2 overlaps page1 on {title}");
    }

    // Page 3 — final, 1 row of 5, short page → nextCursor null.
    let tok2 = p2["data"]["articles"]["meta"]["nextCursor"]
        .as_str()
        .unwrap()
        .to_string();
    let p3 = gql(
        &app,
        "query($c:String){ articles(cursor:$c, pageSize:2, sort:\"title:asc\") \
            { data{ title } meta{ nextCursor } } }",
        json!({ "c": tok2 }),
    )
    .await;
    assert_eq!(p3["data"]["articles"]["data"].as_array().unwrap().len(), 1, "{p3}");
    assert!(
        p3["data"]["articles"]["meta"]["nextCursor"].is_null(),
        "last/short page should have null nextCursor: {p3}"
    );
}

#[tokio::test]
async fn graphql_cursor_with_non_scalar_sort_is_bad_user_input() {
    let app = TestApp::spawn().await;
    // Article variant with a json field (non-scalar, not keyset-sortable).
    let r = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article", "display_name": "Article",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "blob", "kind": "json"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", r.text().await.unwrap());

    let body = gql(
        &app,
        "query{ articles(cursor:\"first\", sort:\"blob:asc\") \
            { data{ title } } }",
        json!({}),
    )
    .await;

    assert!(
        body["data"]["articles"].is_null(),
        "errored field should null its data: {body}"
    );
    assert_eq!(
        body["errors"][0]["extensions"]["code"], "BAD_USER_INPUT",
        "{body}"
    );
}

#[tokio::test]
async fn graphql_offset_paging_still_works() {
    let app = TestApp::spawn().await;
    make_article(&app).await;
    seed_articles(&app, 3).await;

    let body = gql(
        &app,
        "query{ articles(page:1, pageSize:2, sort:\"title:asc\") \
            { data{ title } meta{ page total nextCursor } } }",
        json!({}),
    )
    .await;

    assert_eq!(body["data"]["articles"]["data"].as_array().unwrap().len(), 2, "{body}");
    assert_eq!(body["data"]["articles"]["meta"]["page"], 1, "{body}");
    assert_eq!(body["data"]["articles"]["meta"]["total"], 3, "{body}");
    // offset mode → no cursor token
    assert!(
        body["data"]["articles"]["meta"]["nextCursor"].is_null(),
        "offset mode should not emit nextCursor: {body}"
    );
}
