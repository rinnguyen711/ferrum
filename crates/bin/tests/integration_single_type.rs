mod common;
use common::TestApp;
use serde_json::json;

async fn make_homepage_type(app: &TestApp) {
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
}

#[tokio::test]
async fn get_single_type_no_entry_returns_null() {
    let app = TestApp::spawn().await;
    make_homepage_type(&app).await;

    let resp = app
        .admin(app.client.get(app.url("/api/single-types/homepage")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.is_null(), "expected null body, got: {body}");
}

#[tokio::test]
async fn put_single_type_creates_then_updates() {
    let app = TestApp::spawn().await;
    make_homepage_type(&app).await;

    // First PUT — creates the entry
    let resp = app
        .admin(app.client.put(app.url("/api/single-types/homepage")))
        .json(&json!({"title": "Welcome"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(created["title"], "Welcome");
    let id = created["id"].as_str().unwrap().to_string();

    // Second PUT — updates the existing entry
    let resp = app
        .admin(app.client.put(app.url("/api/single-types/homepage")))
        .json(&json!({"title": "Updated"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let updated: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(updated["title"], "Updated");
    // Same record — id must not change
    assert_eq!(updated["id"].as_str().unwrap(), id);

    // GET should now return the updated entry
    let resp = app
        .admin(app.client.get(app.url("/api/single-types/homepage")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let got: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(got["title"], "Updated");
}

#[tokio::test]
async fn get_single_type_unknown_returns_404() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.get(app.url("/api/single-types/nope")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn get_single_type_rejects_collection_kind() {
    let app = TestApp::spawn().await;

    // Create a collection-kind content type
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article",
            "display_name": "Article",
            "fields": [
                {"name": "title", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // GET /api/single-types/<collection-name> must return 422
    let resp = app
        .admin(app.client.get(app.url("/api/single-types/article")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}
