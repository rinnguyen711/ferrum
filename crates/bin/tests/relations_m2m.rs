//! Phase 2 relations close-out: one_to_one + many_to_many integration tests.
//! Boots a real Postgres per test via testcontainers and drives the axum
//! router in-process.

mod common;
use common::TestApp;
use serde_json::{json, Value};
use uuid::Uuid;

/// post.tags many_to_many → tag, inverse "posts".
async fn setup_post_tags(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "tag",
            "display_name": "Tag",
            "fields": [{"name": "label", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string"},
                {"name": "tags", "kind": "relation",
                 "kind_meta": {"target": "tag", "cardinality": "many_to_many", "inverse": "posts"}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_tag(app: &TestApp, label: &str) -> Uuid {
    let resp = app
        .admin(app.client.post(app.url("/api/tag")))
        .json(&json!({"label": label}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn m2m_create_and_populate_forward() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let t1 = create_tag(&app, "rust").await;
    let t2 = create_tag(&app, "web").await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "hello", "tags": [t1, t2]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let post: Value = resp.json().await.unwrap();
    let post_id = post["id"].as_str().unwrap();

    // Unpopulated GET omits tags.
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post/{post_id}"))))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert!(
        body.get("tags").is_none(),
        "tags should be omitted when not populated: {body}"
    );

    // Populated GET returns the tag objects.
    let resp = app
        .admin(
            app.client
                .get(app.url(&format!("/api/post/{post_id}?populate=tags"))),
        )
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let tags = body["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    let labels: Vec<&str> = tags.iter().map(|t| t["label"].as_str().unwrap()).collect();
    assert!(labels.contains(&"rust") && labels.contains(&"web"));
}

#[tokio::test]
async fn m2m_inverse_populate() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let t1 = create_tag(&app, "rust").await;
    for title in ["a", "b"] {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"title": title, "tags": [t1]}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }
    let resp = app
        .admin(
            app.client
                .get(app.url(&format!("/api/tag/{t1}?populate=posts"))),
        )
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let posts = body["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 2);
}

#[tokio::test]
async fn m2m_patch_replace_set() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let t1 = create_tag(&app, "rust").await;
    let t2 = create_tag(&app, "web").await;
    let t3 = create_tag(&app, "db").await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "p", "tags": [t1, t2]}))
        .send()
        .await
        .unwrap();
    let post: Value = resp.json().await.unwrap();
    let id = post["id"].as_str().unwrap().to_string();

    // Replace {t1,t2} with {t2,t3}.
    let resp = app
        .admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({"title": "p", "tags": [t2, t3]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(
            app.client
                .get(app.url(&format!("/api/post/{id}?populate=tags"))),
        )
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let labels: Vec<&str> = body["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels.len(), 2);
    assert!(labels.contains(&"web") && labels.contains(&"db") && !labels.contains(&"rust"));

    // Clear with [].
    let resp = app
        .admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({"title": "p", "tags": []}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let resp = app
        .admin(
            app.client
                .get(app.url(&format!("/api/post/{id}?populate=tags"))),
        )
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert!(body["tags"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn m2m_bad_target_id_rejected() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let ghost = Uuid::new_v4();
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "x", "tags": [ghost]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn m2m_links_cascade_on_target_delete() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let t1 = create_tag(&app, "rust").await;
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "p", "tags": [t1]}))
        .send()
        .await
        .unwrap();
    let id = resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .admin(app.client.delete(app.url(&format!("/api/tag/{t1}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(
            app.client
                .get(app.url(&format!("/api/post/{id}?populate=tags"))),
        )
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert!(body["tags"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn one_to_one_unique_and_inverse() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({"name":"profile","display_name":"Profile","fields":[{"name":"bio","kind":"string"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({"name":"user","display_name":"User","fields":[
            {"name":"name","kind":"string"},
            // "user" is reserved — use "owner" as the inverse name.
            {"name":"profile","kind":"relation","kind_meta":{"target":"profile","cardinality":"one_to_one","inverse":"owner"}}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.post(app.url("/api/profile")))
        .json(&json!({"bio": "hi"}))
        .send()
        .await
        .unwrap();
    let prof_id = resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .admin(app.client.post(app.url("/api/user")))
        .json(&json!({"name": "a", "profile": prof_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.post(app.url("/api/user")))
        .json(&json!({"name": "b", "profile": prof_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        409,
        "second user reusing profile must conflict: {}",
        resp.text().await.unwrap()
    );

    let resp = app
        .admin(
            app.client
                .get(app.url(&format!("/api/profile/{prof_id}?populate=owner"))),
        )
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["owner"].is_object(),
        "1:1 inverse should be a single object: {body}"
    );
    assert_eq!(body["owner"]["name"].as_str().unwrap(), "a");
}

#[tokio::test]
async fn m2m_join_table_dropped_with_type() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let resp = app
        .admin(
            app.client
                .delete(app.url("/admin/content-types/post?confirm=true")),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string"},
                {"name": "tags", "kind": "relation", "kind_meta": {"target": "tag", "cardinality": "many_to_many"}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        201,
        "recreate after drop must not collide: {}",
        resp.text().await.unwrap()
    );
}
