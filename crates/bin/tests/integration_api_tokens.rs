mod common;
use common::TestApp;
use serde_json::{json, Value};

async fn make_article_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article",
            "display_name": "Article",
            "fields": [{"name": "title", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_token(app: &TestApp, scopes: &[&str], expires_at: Option<&str>) -> String {
    let mut body = json!({ "name": "test", "scopes": scopes });
    if let Some(exp) = expires_at {
        body["expires_at"] = json!(exp);
    }
    let resp = app
        .admin(app.client.post(app.url("/api/admin/tokens")))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let v: Value = resp.json().await.unwrap();
    v["token"].as_str().unwrap().to_string()
}

fn with_token(
    _app: &TestApp,
    builder: reqwest::RequestBuilder,
    token: &str,
) -> reqwest::RequestBuilder {
    builder.header("authorization", format!("Bearer {token}"))
}

#[tokio::test]
async fn content_read_token_can_list() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read"], None).await;

    let resp = with_token(&app, app.client.get(app.url("/api/article")), &token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn content_read_token_cannot_create() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read"], None).await;

    let resp = with_token(&app, app.client.post(app.url("/api/article")), &token)
        .json(&json!({"title": "hi"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn content_readwrite_token_can_create() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read", "content:write"], None).await;

    let resp = with_token(&app, app.client.post(app.url("/api/article")), &token)
        .json(&json!({"title": "hi"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn expired_token_returns_401() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    // expires_at in the past
    let token = create_token(&app, &["content:read"], Some("2000-01-01T00:00:00Z")).await;

    let resp = with_token(&app, app.client.get(app.url("/api/article")), &token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn revoked_token_returns_401() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read"], None).await;

    // Get the token id from list
    let list_resp = app
        .admin(app.client.get(app.url("/api/admin/tokens")))
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let list: Vec<Value> = list_resp.json().await.unwrap();
    let id = list[0]["id"].as_str().unwrap();

    // Revoke
    let del = app
        .admin(
            app.client
                .delete(app.url(&format!("/api/admin/tokens/{id}"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 204);

    // Now the token should be rejected
    let resp = with_token(&app, app.client.get(app.url("/api/article")), &token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn unknown_token_returns_401() {
    let app = TestApp::spawn().await;
    let resp = with_token(
        &app,
        app.client.get(app.url("/api/article")),
        "rat_notarealtoken000000000000000000000000000000000000000000000000",
    )
    .send()
    .await
    .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn jwt_auth_still_works() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/article")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn create_token_no_scopes_returns_422() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/api/admin/tokens")))
        .json(&json!({"name": "bad", "scopes": []}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn last_used_at_updated_on_auth() {
    let app = TestApp::spawn().await;
    make_article_type(&app).await;
    let token = create_token(&app, &["content:read"], None).await;

    // Before use — last_used_at should be null
    let list: Vec<Value> = app
        .admin(app.client.get(app.url("/api/admin/tokens")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list[0]["last_used_at"].is_null());

    // Use the token
    with_token(&app, app.client.get(app.url("/api/article")), &token)
        .send()
        .await
        .unwrap();

    // After use — last_used_at should be set
    let list2: Vec<Value> = app
        .admin(app.client.get(app.url("/api/admin/tokens")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!list2[0]["last_used_at"].is_null());
}
