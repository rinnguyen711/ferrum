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
    // `banners` query) would never appear. With relation/media as scalar UUID
    // ids the schema always builds and the new type is selectable.
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
    // (now a UUID scalar) is selectable. Before the fix the rebuild errored and
    // froze the old schema → `banners` would be an unknown field.
    let body = gql(
        &app,
        "{ banners(page:1,pageSize:5){ meta{ total } data{ title page } } }",
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
