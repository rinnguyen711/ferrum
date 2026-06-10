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
async fn gt_excludes_nulls() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$gt]=5").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn gte_inclusive() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$gte]=5").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn lt_basic() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$lt]=10").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn lte_inclusive() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$lte]=10").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn gt_on_string_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[title][$gt]=hi")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn in_two_categories() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[category][$in][0]=tech&filters[category][$in][1]=design",
    )
    .await;
    assert_eq!(body["meta"]["total"], 4);
}

#[tokio::test]
async fn nin_excludes() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$nin][0]=0&filters[views][$nin][1]=20").await;
    // PG `NOT IN` excludes NULLs.
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn in_single_value_behaves_like_eq() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[category][$in][0]=tech").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn in_missing_index_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[views][$in]=1")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn in_duplicate_index_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(
            app.client
                .get(app.url("/api/post?filters[views][$in][0]=1&filters[views][$in][0]=2")),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn in_over_cap_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let mut q = String::new();
    for i in 0..=100 {
        if !q.is_empty() {
            q.push('&');
        }
        q.push_str(&format!("filters[views][$in][{i}]={i}"));
    }
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post?{q}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn contains_basic() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$contains]=foo").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn containsi_case_insensitive() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$containsi]=FOO").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn starts_with_basic() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$startsWith]=foo").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn ends_with_basic() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$endsWith]=foo").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn contains_literal_percent() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "50% off", "category": "deal"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    // `%25` URL-decodes to `%`; server escapes to `\%`; wraps to `%50\%%`.
    let body = list_body(&app, "filters[title][$contains]=50%25").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "50% off");
}

#[tokio::test]
async fn contains_on_integer_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(
            app.client
                .get(app.url("/api/post?filters[views][$contains]=5")),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn compose_multiple_groups() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[title][$contains]=foo&filters[views][$gt]=0&filters[category][$in][0]=tech&filters[category][$in][1]=design",
    )
    .await;
    // barfoo (5, tech) and foobar (10, design) survive all three.
    assert_eq!(body["meta"]["total"], 2);
}
