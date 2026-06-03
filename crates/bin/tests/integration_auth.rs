mod common;
use common::{TestApp, TEST_EMAIL, TEST_PASSWORD};

#[tokio::test]
async fn setup_is_self_closing() {
    let app = TestApp::spawn().await;
    // spawn() already ran setup once; a second setup must 409.
    let resp = app
        .client
        .post(app.url("/auth/setup"))
        .json(&serde_json::json!({ "email": "second@example.test", "password": "another-pw-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "conflict");
}

#[tokio::test]
async fn setup_status_flips_after_setup() {
    // spawn() runs setup once, so by the time we get the app, setup is closed.
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .get(app.url("/auth/setup"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["setup_required"], false);
}

#[tokio::test]
async fn concurrent_setup_creates_one_admin() {
    let app = TestApp::spawn().await;
    // spawn() already created the first admin. Fire several concurrent setups
    // with distinct emails; all must 409 (the atomic insert lets none through).
    let mut handles = Vec::new();
    for i in 0..5 {
        let client = app.client.clone();
        let url = app.url("/auth/setup");
        handles.push(tokio::spawn(async move {
            client
                .post(url)
                .json(&serde_json::json!({
                    "email": format!("race{i}@example.test"),
                    "password": "race-password-123"
                }))
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        }));
    }
    for h in handles {
        assert_eq!(h.await.unwrap(), 409);
    }
    // Exactly one user total (the spawn() admin).
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM _users")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn login_good_credentials() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": TEST_EMAIL, "password": TEST_PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["token"].as_str().is_some());
    assert!(body["expires_at"].as_i64().is_some());
}

#[tokio::test]
async fn login_wrong_password_401() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": TEST_EMAIL, "password": "totally-wrong" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn login_unknown_email_401() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": "nobody@example.test", "password": "whatever-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn me_requires_token() {
    let app = TestApp::spawn().await;
    let resp = app.client.get(app.url("/auth/me")).send().await.unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn me_returns_principal_with_token() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.get(app.url("/auth/me")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["email"], TEST_EMAIL);
    assert_eq!(body["roles"][0], "admin");
}

#[tokio::test]
async fn protected_route_rejects_missing_token() {
    let app = TestApp::spawn().await;
    // /api/<type> is behind require_auth; no token → 401.
    let resp = app.client.get(app.url("/api/article")).send().await.unwrap();
    assert_eq!(resp.status(), 401);
}
