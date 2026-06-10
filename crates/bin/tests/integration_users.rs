mod common;
use common::{TestApp, TEST_EMAIL};

/// Create a second user via the admin API and return its id.
async fn create_user(app: &TestApp, email: &str, password: &str, roles: &[&str]) -> String {
    let resp = app
        .admin(app.client.post(app.url("/admin/users")))
        .json(&serde_json::json!({ "email": email, "password": password, "roles": roles }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "create_user should 201");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

/// Log in and return a bearer token.
async fn token_for(app: &TestApp, email: &str, password: &str) -> String {
    let body: serde_json::Value = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn list_includes_seeded_admin() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.get(app.url("/admin/users")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|u| u["email"] == TEST_EMAIL));
}

#[tokio::test]
async fn create_then_list_and_dup_conflict() {
    let app = TestApp::spawn().await;
    create_user(&app, "ed@example.test", "editor-pw-123", &["editor"]).await;

    let dup = app
        .admin(app.client.post(app.url("/admin/users")))
        .json(&serde_json::json!({ "email": "ed@example.test", "password": "another-123", "roles": ["editor"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup.status(), 409);
}

#[tokio::test]
async fn create_short_password_422() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/users")))
        .json(&serde_json::json!({ "email": "x@example.test", "password": "short", "roles": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn update_password_then_login_works() {
    let app = TestApp::spawn().await;
    let id = create_user(&app, "rot@example.test", "first-pw-123", &["viewer"]).await;

    let patch = app
        .admin(app.client.patch(app.url(&format!("/admin/users/{id}"))))
        .json(&serde_json::json!({ "password": "second-pw-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), 200);

    let old = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": "rot@example.test", "password": "first-pw-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(old.status(), 401);

    let new = app
        .client
        .post(app.url("/auth/login"))
        .json(&serde_json::json!({ "email": "rot@example.test", "password": "second-pw-123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(new.status(), 200);
}

#[tokio::test]
async fn update_roles_reflected() {
    let app = TestApp::spawn().await;
    let id = create_user(&app, "promote@example.test", "promote-123", &["viewer"]).await;
    let resp = app
        .admin(app.client.patch(app.url(&format!("/admin/users/{id}"))))
        .json(&serde_json::json!({ "roles": ["editor"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["roles"][0], "editor");
}

#[tokio::test]
async fn delete_user_then_404() {
    let app = TestApp::spawn().await;
    let id = create_user(&app, "gone@example.test", "gone-pw-123", &[]).await;
    let del = app
        .admin(app.client.delete(app.url(&format!("/admin/users/{id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 204);
    let again = app
        .admin(app.client.delete(app.url(&format!("/admin/users/{id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 404);
}

#[tokio::test]
async fn self_delete_blocked_409() {
    let app = TestApp::spawn().await;
    let list: serde_json::Value = app
        .admin(app.client.get(app.url("/admin/users")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let me = list
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["email"] == TEST_EMAIL)
        .unwrap();
    let my_id = me["id"].as_str().unwrap();
    let resp = app
        .admin(app.client.delete(app.url(&format!("/admin/users/{my_id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn self_demote_blocked_409() {
    let app = TestApp::spawn().await;
    let list: serde_json::Value = app
        .admin(app.client.get(app.url("/admin/users")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let me = list
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["email"] == TEST_EMAIL)
        .unwrap();
    let my_id = me["id"].as_str().unwrap();
    let resp = app
        .admin(app.client.patch(app.url(&format!("/admin/users/{my_id}"))))
        .json(&serde_json::json!({ "roles": ["viewer"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn non_admin_forbidden() {
    let app = TestApp::spawn().await;
    create_user(&app, "editor2@example.test", "editor2-pw-123", &["editor"]).await;
    let token = token_for(&app, "editor2@example.test", "editor2-pw-123").await;

    let resp = app
        .client
        .get(app.url("/admin/users"))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}
