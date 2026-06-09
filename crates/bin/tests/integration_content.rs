mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn single_type_rejected_on_collection_routes() {
    let app = TestApp::spawn().await;

    // Create a single-kind content type via HTTP.
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "homepage",
            "display_name": "Homepage",
            "kind": "single",
            "fields": [
                {"name": "title", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // GET /api/<name> on a single type must return 422.
    let resp = app
        .admin(app.client.get(app.url("/api/homepage")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());

    // POST /api/<name> on a single type must return 422.
    let resp = app
        .admin(app.client.post(app.url("/api/homepage")))
        .json(&json!({"title": "Hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

async fn make_post_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true, "max_length": 64},
                {"name": "views", "kind": "integer"},
                {"name": "published", "kind": "boolean", "default": false}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
}

#[tokio::test]
async fn full_entry_lifecycle() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;

    // Create entry
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "Hello", "views": 3}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let entry: serde_json::Value = resp.json().await.unwrap();
    let id = entry["id"].as_str().unwrap().to_string();
    assert_eq!(entry["title"], "Hello");
    assert_eq!(entry["views"], 3);
    assert_eq!(entry["published"], false);

    // List
    let resp = app.admin(app.client.get(app.url("/api/post"))).send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["id"], id);

    // Get one
    let resp = app.admin(app.client.get(app.url(&format!("/api/post/{id}")))).send().await.unwrap();
    assert_eq!(resp.status(), 200);

    // Update
    let resp = app
        .admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({"title": "Hello v2", "views": 10, "published": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["title"], "Hello v2");
    assert_eq!(updated["published"], true);

    // Delete
    let resp = app.admin(app.client.delete(app.url(&format!("/api/post/{id}")))).send().await.unwrap();
    assert_eq!(resp.status(), 204);

    let resp = app.admin(app.client.get(app.url(&format!("/api/post/{id}")))).send().await.unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn required_field_missing_rejected() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"views": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn unknown_field_rejected() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "x", "ghost": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn put_full_replace_nulls_absent_optional_fields() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;

    // Create with all optional fields populated
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "Hi", "views": 99, "published": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let id = resp.json::<serde_json::Value>().await.unwrap()["id"].as_str().unwrap().to_string();

    // PUT without `views` — full replace should null it
    let resp = app
        .admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({"title": "Hi v2", "published": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["title"], "Hi v2");
    assert!(updated["views"].is_null(), "views should be nulled by PUT, got {:?}", updated["views"]);
    assert_eq!(updated["published"], false);
}

#[tokio::test]
async fn post_explicit_null_for_optional_int() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;

    // Explicit JSON null for optional integer field — exercises typed-null bind.
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "x", "views": null}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["views"].is_null());
}

#[tokio::test]
async fn pagination_and_sort() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;
    for i in 0..5 {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"title": format!("t{i}"), "views": i}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }
    let resp = app
        .admin(app.client.get(app.url("/api/post?page=1&pageSize=2&sort=views:desc")))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 5);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"][0]["views"], 4);
    assert_eq!(body["data"][1]["views"], 3);
}

#[tokio::test]
async fn unknown_sort_field_rejected() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?sort=ghost:asc")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}
