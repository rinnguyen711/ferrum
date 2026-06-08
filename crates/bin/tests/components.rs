mod common;

use common::TestApp;
use serde_json::json;

// ----- helpers -----

async fn make_hero_component(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/components")))
        .json(&json!({
            "uid": "shared.hero",
            "display_name": "Hero Block",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "subtitle", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn make_article_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article",
            "display_name": "Article",
            "fields": [
                {
                    "name": "hero",
                    "kind": "component",
                    "kind_meta": {"component": "shared.hero", "multiple": false}
                },
                {
                    "name": "sections",
                    "kind": "component",
                    "kind_meta": {"component": "shared.hero", "multiple": true}
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

// ----- tests -----

#[tokio::test]
async fn create_and_read_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;

    let resp = app
        .admin(app.client.get(app.url("/admin/components/shared.hero")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["uid"], "shared.hero");
    assert_eq!(body["display_name"], "Hero Block");
    assert_eq!(body["fields"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn list_components() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;

    let resp = app
        .admin(app.client.get(app.url("/admin/components")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
}

#[tokio::test]
async fn update_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;

    let resp = app
        .admin(app.client.put(app.url("/admin/components/shared.hero")))
        .json(&json!({
            "display_name": "Hero Block v2",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "subtitle", "kind": "string"},
                {"name": "cta", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["display_name"], "Hero Block v2");
    assert_eq!(body["fields"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn delete_unreferenced_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;

    let resp = app
        .admin(app.client.delete(app.url("/admin/components/shared.hero?confirm=true")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
}

#[tokio::test]
async fn delete_referenced_component_rejected() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.delete(app.url("/admin/components/shared.hero?confirm=true")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn get_content_type_injects_component_fields() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.get(app.url("/admin/content-types/article")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let hero_field = body["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "hero")
        .unwrap();
    let comp_fields = hero_field["kind_meta"]["_component_fields"].as_array().unwrap();
    assert_eq!(comp_fields.len(), 2);
    assert_eq!(comp_fields[0]["name"], "title");
}

#[tokio::test]
async fn write_entry_with_valid_single_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "hero": {"title": "Welcome", "subtitle": "Sub"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["hero"]["title"], "Welcome");
}

#[tokio::test]
async fn write_entry_with_valid_repeatable_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "sections": [
                {"title": "Intro"},
                {"title": "Features", "subtitle": "All features"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["sections"].as_array().unwrap().len(), 2);
    assert_eq!(body["sections"][0]["title"], "Intro");
}

#[tokio::test]
async fn write_entry_missing_required_inner_field_rejected() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "hero": {"subtitle": "no title here"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = resp.json().await.unwrap();
    let err_text = serde_json::to_string(&body).unwrap();
    assert!(err_text.contains("hero.title"), "expected hero.title in {err_text}");
}

#[tokio::test]
async fn write_entry_wrong_inner_field_type_rejected() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "hero": {"title": 42}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn write_entry_repeatable_wrong_type_rejected() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "sections": {"title": "not an array"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn uid_must_have_two_segments() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/components")))
        .json(&json!({
            "uid": "noperiod",
            "display_name": "Bad",
            "fields": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn component_cannot_have_relation_inner_field() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/components")))
        .json(&json!({
            "uid": "shared.bad",
            "display_name": "Bad",
            "fields": [
                {"name": "author", "kind": "relation", "kind_meta": {"target": "user", "cardinality": "many_to_one"}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn existing_entries_readable_after_component_update() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({"hero": {"title": "Old title"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let entry: serde_json::Value = resp.json().await.unwrap();
    let id = entry["id"].as_str().unwrap();

    app.admin(app.client.put(app.url("/admin/components/shared.hero")))
        .json(&json!({
            "display_name": "Hero Block",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "subtitle", "kind": "string"},
                {"name": "new_field", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();

    let resp = app
        .admin(app.client.get(app.url(&format!("/api/article/{id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["hero"]["title"], "Old title");
}
