mod common;
use common::TestApp;
use serde_json::{json, Value};

async fn make_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true, "max_length": 64},
                {"name": "views", "kind": "integer"},
                {"name": "published", "kind": "boolean", "default": false},
                {"name": "category", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
}

async fn seed(app: &TestApp) {
    let rows = vec![
        json!({"title": "a", "views": 0,    "published": true,  "category": "x"}),
        json!({"title": "b", "views": 5,    "published": false, "category": "x"}),
        json!({"title": "c", "views": 10,   "published": true,  "category": "y"}),
        json!({"title": "d", "views": null, "published": true,  "category": "y"}),
        json!({"title": "e", "views": 20,   "published": false, "category": null}),
    ];
    for row in rows {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&row)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    }
}

async fn list_body(app: &TestApp, query: &str) -> Value {
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post?{query}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    resp.json().await.unwrap()
}

#[tokio::test]
async fn eq_string_filter() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$eq]=c").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "c");
}

#[tokio::test]
async fn ne_integer_filter() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$ne]=0").await;
    // SQL `<>` excludes NULL — d (views=null) is NOT returned. a (views=0) is filtered.
    // Remaining: b (5), c (10), e (20).
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn null_true_returns_nulls() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$null]=true").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "d");
}

#[tokio::test]
async fn null_false_returns_non_nulls() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$null]=false").await;
    assert_eq!(body["meta"]["total"], 4);
}

#[tokio::test]
async fn implicit_and_combines() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[category][$eq]=x&filters[published][$eq]=true").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "a");
}

#[tokio::test]
async fn count_reflects_filter() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[published][$eq]=true").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn pagination_and_filter_compose() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[published][$eq]=true&page=1&pageSize=2&sort=views:asc",
    )
    .await;
    assert_eq!(body["meta"]["total"], 3);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    // published=true rows by views asc: a(0), c(10), d(null).
    // Postgres default is NULLS LAST for ASC, so a then c.
    assert_eq!(body["data"][0]["title"], "a");
    assert_eq!(body["data"][1]["title"], "c");
}

#[tokio::test]
async fn eq_null_rewrites_to_is_null() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$eq]=null").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "d");
}

#[tokio::test]
async fn unknown_field_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[ghost][$eq]=1")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn unknown_op_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[title][$bogus]=1")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn malformed_int_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[views][$eq]=abc")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn duplicate_col_op_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[views][$eq]=1&filters[views][$eq]=2")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}
