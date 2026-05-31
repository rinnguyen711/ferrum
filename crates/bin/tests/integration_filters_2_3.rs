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
        json!({"title": "foo",     "views": 0,    "published": true,  "category": "tech"}),
        json!({"title": "barfoo",  "views": 5,    "published": false, "category": "tech"}),
        json!({"title": "foobar",  "views": 10,   "published": true,  "category": "design"}),
        json!({"title": "null-vw", "views": null, "published": true,  "category": "design"}),
        json!({"title": "xyz",     "views": 20,   "published": false, "category": null}),
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
async fn or_two_categories() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$or][0][category][$eq]=tech&filters[$or][1][category][$eq]=design",
    )
    .await;
    assert_eq!(body["meta"]["total"], 4);
}

#[tokio::test]
async fn or_mixing_ops() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$or][0][category][$eq]=tech&filters[$or][1][views][$gt]=15",
    )
    .await;
    // tech: foo, barfoo (2). views>15: xyz (1). Distinct rows = 3.
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn not_excludes_leaf_and_nulls() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    // NOT (views = 0) — by Postgres 3VL, NULL views also excluded.
    let body = list_body(&app, "filters[$not][views][$eq]=0").await;
    // Total rows = 5; views=0 = foo (1); views=NULL = null-vw (1).
    // NOT excludes both → 3 surviving rows.
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn not_of_or() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$not][$or][0][category][$eq]=tech&filters[$not][$or][1][category][$eq]=design",
    )
    .await;
    // Rows where category is neither tech nor design AND not null.
    // xyz has category=null → excluded by 3VL. So 0 rows match.
    assert_eq!(body["meta"]["total"], 0);
}

#[tokio::test]
async fn nested_or_inside_and() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$and][0][published][$eq]=true\
         &filters[$and][1][$or][0][category][$eq]=tech\
         &filters[$and][1][$or][1][category][$eq]=design",
    )
    .await;
    // published=true AND (tech OR design):
    //   foo (tech, pub=true), foobar (design, pub=true), null-vw (design, pub=true) → 3.
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn implicit_and_with_or() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[published][$eq]=true\
         &filters[$or][0][category][$eq]=tech\
         &filters[$or][1][category][$eq]=design",
    )
    .await;
    // Same as nested test above — should be 3.
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn or_with_pagination_and_sort() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$or][0][views][$gt]=5\
         &filters[$or][1][views][$null]=true\
         &sort=views:asc\
         &page=1&pageSize=2",
    )
    .await;
    assert_eq!(body["meta"]["total"], 3); // 10, 20, null
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn depth_cap_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let mut q = String::new();
    for _ in 0..9 {
        q.push_str("[$or][0]");
    }
    q.push_str("[title][$eq]=foo");
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post?filters{q}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn leaf_cap_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let mut parts = Vec::new();
    for i in 0..101 {
        parts.push(format!("filters[$or][{i}][title][$eq]=v{i}"));
    }
    let q = parts.join("&");
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post?{q}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn empty_or_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[$or]=foo")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn or_gap_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(
            app.url("/api/post?filters[$or][0][title][$eq]=a&filters[$or][2][title][$eq]=b"),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn not_with_index_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(
            app.url("/api/post?filters[$not][0][title][$eq]=a"),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}
