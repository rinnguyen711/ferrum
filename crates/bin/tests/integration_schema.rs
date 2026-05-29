mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn create_list_get_delete_content_type() {
    let app = TestApp::spawn().await;

    // Create
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true, "max_length": 64}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // List
    let resp = app
        .admin(app.client.get(app.url("/admin/content-types")))
        .send()
        .await
        .unwrap();
    let list: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "post");

    // Get one
    let resp = app
        .admin(app.client.get(app.url("/admin/content-types/post")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["display_name"], "Post");

    // Delete without confirm → 422
    let resp = app
        .admin(app.client.delete(app.url("/admin/content-types/post")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);

    // Delete with confirm
    let resp = app
        .admin(app.client.delete(app.url("/admin/content-types/post?confirm=true")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    // Gone
    let resp = app
        .admin(app.client.get(app.url("/admin/content-types/post")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn rejects_invalid_type_name() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "Bad Name",
            "display_name": "X",
            "fields": [{"name": "title", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn rejects_duplicate_create() {
    let app = TestApp::spawn().await;
    let payload = json!({
        "name": "post",
        "display_name": "Post",
        "fields": [{"name": "title", "kind": "string"}]
    });
    let resp = app.admin(app.client.post(app.url("/admin/content-types"))).json(&payload).send().await.unwrap();
    assert_eq!(resp.status(), 201);
    let resp = app.admin(app.client.post(app.url("/admin/content-types"))).json(&payload).send().await.unwrap();
    assert_eq!(resp.status(), 409);
}
