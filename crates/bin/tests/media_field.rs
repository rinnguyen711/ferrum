//! End-to-end integration tests for the `media` content-type field kind.
//! Covers the full round-trip: asset upload, content type creation with single
//! and multiple media fields, entry create/read, embedded metadata on GET, and
//! the SET NULL / cascade-delete behaviors when assets are deleted.

mod common;
use common::TestApp;
use serde_json::{json, Value};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The canonical 1x1 PNG used by the media tests.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
    0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
    0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
    0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Upload the tiny PNG and return the asset `id`.
async fn upload_asset(app: &TestApp, filename: &str) -> String {
    let part = reqwest::multipart::Part::bytes(TINY_PNG.to_vec())
        .file_name(filename.to_string())
        .mime_str("application/octet-stream")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let resp = app
        .admin(app.client.post(app.url("/admin/media/assets")))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "asset upload failed: {}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// Test 1: single + multiple media field round-trip with SET NULL / cascade delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_single_and_multi_round_trip() {
    let app = TestApp::spawn().await;

    // Step 1: Upload two assets; same bytes but distinct ids.
    let a1 = upload_asset(&app, "hero.png").await;
    let a2 = upload_asset(&app, "gallery-1.png").await;

    // Step 2: Create content type `post` with title + hero (single) + gallery (multiple).
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string"},
                {"name": "hero",    "kind": "media", "kind_meta": {"multiple": false}},
                {"name": "gallery", "kind": "media", "kind_meta": {"multiple": true}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "content-type create failed: {}", resp.text().await.unwrap());

    // Step 3: Create an entry pointing at both assets.
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({
            "title": "hi",
            "hero": a1,
            "gallery": [a2, a1]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "entry create failed: {}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let entry_id = created["id"].as_str().unwrap().to_string();

    // Step 4: GET the entry — media fields embed full asset metadata without ?populate.
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post/{entry_id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "entry GET failed: {}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // hero must be an object (embedded) with correct id and mime_type.
    assert!(
        body["hero"].is_object(),
        "expected hero to be an object, got: {}",
        body["hero"]
    );
    assert_eq!(body["hero"]["id"], a1, "hero id mismatch");
    assert_eq!(body["hero"]["mime_type"], "image/png", "hero mime_type mismatch");

    // gallery must be an array of length 2, preserving insertion order.
    let gallery = body["gallery"].as_array().expect("gallery should be an array");
    assert_eq!(gallery.len(), 2, "expected gallery length 2, got {}", gallery.len());
    assert_eq!(gallery[0]["id"], a2, "gallery[0] id mismatch");
    assert_eq!(gallery[1]["id"], a1, "gallery[1] id mismatch");

    // Step 5: Delete asset a1.
    let del = app
        .admin(app.client.delete(app.url(&format!("/admin/media/assets/{a1}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 204, "asset delete failed: {}", del.text().await.unwrap());

    // Step 6: GET entry again — SET NULL on hero, cascade-drop on gallery join row.
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post/{entry_id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "entry re-GET failed: {}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    assert!(
        body["hero"].is_null(),
        "expected hero to be null after asset delete (SET NULL), got: {}",
        body["hero"]
    );

    let gallery = body["gallery"].as_array().expect("gallery should still be an array");
    assert_eq!(
        gallery.len(),
        1,
        "expected gallery length 1 after a1 deleted (cascade), got: {gallery:?}"
    );
    assert_eq!(gallery[0]["id"], a2, "remaining gallery item should be a2");
}

// ---------------------------------------------------------------------------
// Test 2: validation rejects missing asset; explicit empty-array clears gallery
// ---------------------------------------------------------------------------

#[tokio::test]
async fn media_field_rejects_missing_asset_and_clears() {
    let app = TestApp::spawn().await;

    // Step 1: Create content type `doc` with hero (single) + shots (multiple).
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "doc",
            "display_name": "Doc",
            "fields": [
                {"name": "hero",  "kind": "media", "kind_meta": {"multiple": false}},
                {"name": "shots", "kind": "media", "kind_meta": {"multiple": true}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "content-type create failed: {}", resp.text().await.unwrap());

    // Step 2: POST with a non-existent asset id → 422.
    let ghost_id = Uuid::new_v4().to_string();
    let resp = app
        .admin(app.client.post(app.url("/api/doc")))
        .json(&json!({ "hero": ghost_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        422,
        "expected 422 for missing asset, got {}: {}",
        resp.status(),
        resp.text().await.unwrap()
    );

    // Step 3: Upload one real asset; create a doc pointing shots at it.
    let a1 = upload_asset(&app, "shot.png").await;

    let resp = app
        .admin(app.client.post(app.url("/api/doc")))
        .json(&json!({ "shots": [a1] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "doc create failed: {}", resp.text().await.unwrap());
    let doc_id = resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Verify shots length is 1.
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/doc/{doc_id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let shots = body["shots"].as_array().expect("shots should be an array");
    assert_eq!(shots.len(), 1, "expected 1 shot after create");

    // Step 4: PUT with shots: [] — full replace should clear the gallery.
    // `doc` has no required fields, so omitting hero is fine (PUT nulls it).
    let resp = app
        .admin(app.client.put(app.url(&format!("/api/doc/{doc_id}"))))
        .json(&json!({ "shots": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "PUT to clear shots failed: {}",
        resp.text().await.unwrap()
    );

    // Verify shots is now empty.
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/doc/{doc_id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let shots = body["shots"].as_array().expect("shots should still be an array after clear");
    assert!(shots.is_empty(), "expected shots to be empty after PUT with [], got: {shots:?}");
}
