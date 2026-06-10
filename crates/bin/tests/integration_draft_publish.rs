mod common;
use common::TestApp;
use serde_json::{json, Value};

async fn make_dp_type(app: &TestApp, name: &str, dp: bool) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": name,
            "display_name": name,
            "options": { "draft_publish": dp },
            "fields": [
                {"name": "title", "kind": "string", "required": true, "max_length": 64}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_entry(app: &TestApp, ty: &str, body: Value) -> Value {
    let resp = app
        .admin(app.client.post(app.url(&format!("/api/{ty}"))))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    resp.json().await.unwrap()
}

async fn list(app: &TestApp, path: &str) -> Value {
    let resp = app
        .admin(app.client.get(app.url(path)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    resp.json().await.unwrap()
}

#[tokio::test]
async fn publish_flow_and_status_filter() {
    let app = TestApp::spawn().await;
    make_dp_type(&app, "note", true).await;

    let created = create_entry(&app, "note", json!({ "title": "hello" })).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert!(
        created["published_at"].is_null(),
        "new entry should be a draft"
    );

    // default list = published only → excludes the draft
    let listed = list(&app, "/api/note").await;
    assert_eq!(listed["meta"]["total"], 0);

    // status=draft includes it
    let drafts = list(&app, "/api/note?status=draft").await;
    assert_eq!(drafts["meta"]["total"], 1);

    // status=all includes it
    let all = list(&app, "/api/note?status=all").await;
    assert_eq!(all["meta"]["total"], 1);

    // publish
    let resp = app
        .admin(app.client.post(app.url(&format!("/api/note/{id}/publish"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let pubd: Value = resp.json().await.unwrap();
    assert!(!pubd["published_at"].is_null());

    // now default list includes it
    let listed2 = list(&app, "/api/note").await;
    assert_eq!(listed2["meta"]["total"], 1);

    // unpublish → back to draft
    let resp = app
        .admin(
            app.client
                .post(app.url(&format!("/api/note/{id}/unpublish"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let unpubd: Value = resp.json().await.unwrap();
    assert!(unpubd["published_at"].is_null());

    // published_at in a PUT body is ignored, not an error
    let resp = app
        .admin(app.client.put(app.url(&format!("/api/note/{id}"))))
        .json(&json!({ "title": "hi", "published_at": "2026-01-01T00:00:00Z" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let updated: Value = resp.json().await.unwrap();
    assert!(
        updated["published_at"].is_null(),
        "PUT must not set published_at"
    );
}

#[tokio::test]
async fn publish_rejected_for_non_draft_publish_type() {
    let app = TestApp::spawn().await;
    make_dp_type(&app, "plain", false).await;
    let created = create_entry(&app, "plain", json!({ "title": "x" })).await;
    let id = created["id"].as_str().unwrap();
    let resp = app
        .admin(
            app.client
                .post(app.url(&format!("/api/plain/{id}/publish"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn non_dp_type_ignores_status_and_has_no_published_at() {
    let app = TestApp::spawn().await;
    make_dp_type(&app, "plain2", false).await;
    create_entry(&app, "plain2", json!({ "title": "a" })).await;
    create_entry(&app, "plain2", json!({ "title": "b" })).await;
    // default list returns all rows (status ignored)
    let listed = list(&app, "/api/plain2").await;
    assert_eq!(listed["meta"]["total"], 2);
    // ?status=published also returns all (ignored for non-DP)
    let pub_listed = list(&app, "/api/plain2?status=published").await;
    assert_eq!(pub_listed["meta"]["total"], 2);
    // entries have no published_at key
    assert!(listed["data"][0].get("published_at").is_none());
}

#[tokio::test]
async fn enable_dp_on_existing_type_via_patch() {
    let app = TestApp::spawn().await;
    make_dp_type(&app, "art", false).await;
    create_entry(&app, "art", json!({ "title": "old" })).await;
    // enable D&P via PATCH
    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/art")))
        .json(&json!({ "options": { "draft_publish": true } }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    // existing row is now a draft (published_at null) → excluded from default list
    let listed = list(&app, "/api/art").await;
    assert_eq!(listed["meta"]["total"], 0);
    let drafts = list(&app, "/api/art?status=draft").await;
    assert_eq!(drafts["meta"]["total"], 1);
    // disabling is rejected
    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/art")))
        .json(&json!({ "options": { "draft_publish": false } }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}
