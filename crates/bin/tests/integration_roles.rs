mod common;
use common::TestApp;
use serde_json::{json, Value};

/// Create a collection content type named `post` so role permissions can
/// reference a known type. The test harness runs migrations only (no seed),
/// so no content types exist by default.
async fn make_post_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_author(app: &TestApp, permissions: Value) -> reqwest::Response {
    app.admin(app.client.post(app.url("/admin/roles")))
        .json(&json!({
            "key": "author",
            "name": "Author",
            "permissions": permissions,
        }))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn create_and_get_role() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;

    let resp = create_author(&app, json!([{"content_type": "post", "action": "find"}])).await;
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.get(app.url("/admin/roles/author")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let perms = body["permissions"].as_array().unwrap();
    assert_eq!(perms.len(), 1);
    assert_eq!(perms[0]["content_type"], "post");
    assert_eq!(perms[0]["action"], "find");

    let resp = app
        .admin(app.client.get(app.url("/admin/roles")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list: Value = resp.json().await.unwrap();
    let author = list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "author")
        .expect("author in list");
    assert_eq!(author["permission_count"], 1);
}

#[tokio::test]
async fn update_role_replaces_permissions() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;

    let resp = create_author(&app, json!([{"content_type": "post", "action": "find"}])).await;
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.put(app.url("/admin/roles/author")))
        .json(&json!({
            "name": "Author",
            "permissions": [
                {"content_type": "post", "action": "find"},
                {"content_type": "post", "action": "create"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.get(app.url("/admin/roles/author")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["permissions"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn duplicate_key_rejected() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;

    let resp = create_author(&app, json!([{"content_type": "post", "action": "find"}])).await;
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = create_author(&app, json!([])).await;
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn unknown_content_type_rejected() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.post(app.url("/admin/roles")))
        .json(&json!({
            "key": "x",
            "name": "X",
            "permissions": [{"content_type": "ghost-type-nope", "action": "find"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn unknown_verb_rejected() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/admin/roles")))
        .json(&json!({
            "key": "y",
            "name": "Y",
            "permissions": [{"content_type": "post", "action": "frobnicate"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn bad_key_rejected() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.post(app.url("/admin/roles")))
        .json(&json!({
            "key": "Not Valid!",
            "name": "Z",
            "permissions": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn system_role_cannot_be_updated() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.put(app.url("/admin/roles/editor")))
        .json(&json!({
            "name": "Editor",
            "permissions": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn system_role_cannot_be_deleted() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.delete(app.url("/admin/roles/editor")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn delete_custom_role() {
    let app = TestApp::spawn().await;
    make_post_type(&app).await;

    let resp = create_author(&app, json!([{"content_type": "post", "action": "find"}])).await;
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.delete(app.url("/admin/roles/author")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.get(app.url("/admin/roles/author")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
