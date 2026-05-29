mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn add_then_drop_field() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [{"name": "title", "kind": "string", "required": true}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Add `views` field
    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({
            "add_fields": [{"name": "views", "kind": "integer"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Entry now accepts the new field
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "x", "views": 42}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Drop `views`
    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({
            "drop_fields": ["views"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Posting `views` is now unknown
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "y", "views": 9}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn cannot_drop_system_field() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [{"name": "title", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({"drop_fields": ["id"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn cannot_re_add_existing_field() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [{"name": "title", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({"add_fields": [{"name": "title", "kind": "text"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}
