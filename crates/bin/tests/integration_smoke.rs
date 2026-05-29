mod common;
use common::TestApp;

#[tokio::test]
async fn healthz_ok() {
    let app = TestApp::spawn().await;
    let resp = app.client.get(app.url("/healthz")).send().await.unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn admin_requires_key() {
    let app = TestApp::spawn().await;
    let resp = app.client.get(app.url("/admin/content-types")).send().await.unwrap();
    assert_eq!(resp.status(), 401);
}
