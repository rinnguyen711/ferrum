mod common;
use common::TestApp;

#[tokio::test]
async fn healthz_ok() {
    let app = TestApp::spawn().await;
    let resp = app.client.get(app.url("/healthz")).send().await.unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["version"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(body["db_ms"].is_number());
}

#[tokio::test]
async fn admin_requires_auth() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .get(app.url("/admin/content-types"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}
