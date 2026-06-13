mod common;
use common::{wait_for_audit, TestApp};
use serde_json::json;
use sqlx::Row;

#[tokio::test]
async fn login_success_is_audited() {
    let app = TestApp::spawn().await;
    let row = wait_for_audit(&app.pool, "auth.login").await;
    assert_eq!(row.get::<String, _>("category"), "auth");
    assert_eq!(row.get::<String, _>("status"), "success");
    assert_eq!(row.get::<String, _>("actor_type"), "user");
}

#[tokio::test]
async fn failed_login_is_audited() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .post(app.url("/auth/login"))
        .json(&json!({ "email": "nobody@example.test", "password": "wrong" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let row = wait_for_audit(&app.pool, "auth.login_failed").await;
    assert_eq!(row.get::<String, _>("status"), "failed");
    assert_eq!(row.get::<String, _>("actor_type"), "system");
}

#[tokio::test]
async fn content_create_and_update_audited_with_diff() {
    let app = TestApp::spawn().await;

    // 1) register a `post` content type with a plain string field.
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
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // 2) create an entry -> capture id.
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "Original"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    let id = body["id"].as_str().unwrap().to_string();

    // 3) update it, changing the scalar `title` field.
    let resp = app
        .admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({"title": "Updated"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let row = wait_for_audit(&app.pool, "entry.update").await;
    let changes: serde_json::Value = row.get("changes");
    assert!(
        changes.as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "update should record a non-empty diff, got: {changes:?}"
    );
}

#[tokio::test]
async fn prune_removes_old_rows() {
    let app = TestApp::spawn().await;
    sqlx::query(
        "INSERT INTO _audit_log (action, category, actor_type, actor_label, created_at)
         VALUES ('entry.create','content','system','seed', now() - INTERVAL '100 days')",
    )
    .execute(&app.pool)
    .await
    .unwrap();
    let removed = rustapi_sql::audit::prune_audit(&app.pool, 90).await.unwrap();
    assert!(removed >= 1);
}

#[tokio::test]
async fn list_endpoint_filters_by_category() {
    let app = TestApp::spawn().await;
    wait_for_audit(&app.pool, "auth.login").await;
    let resp = app
        .admin(app.client.get(app.url("/api/admin/audit?category=auth")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["total"].as_i64().unwrap() >= 1);
    for r in body["rows"].as_array().unwrap() {
        assert_eq!(r["category"], "auth");
    }
}

#[tokio::test]
async fn stats_endpoint_returns_counts() {
    let app = TestApp::spawn().await;
    wait_for_audit(&app.pool, "auth.login").await;
    let resp = app
        .admin(app.client.get(app.url("/api/admin/audit/stats")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["sign_ins"].as_i64().unwrap() >= 1);
}

#[tokio::test]
async fn audit_list_requires_admin() {
    let app = TestApp::spawn().await;
    let resp = app.client.get(app.url("/api/admin/audit")).send().await.unwrap();
    assert!(resp.status() == 401 || resp.status() == 403, "got {}", resp.status());
}

#[tokio::test]
async fn export_returns_csv() {
    let app = TestApp::spawn().await;
    wait_for_audit(&app.pool, "auth.login").await;
    let resp = app
        .admin(app.client.get(app.url("/api/admin/audit/export")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap().to_string();
    assert!(ct.contains("text/csv"), "content-type was {ct}");
    let text = resp.text().await.unwrap();
    assert!(text.starts_with("time,actor,action"));
}
