mod common;
use common::TestApp;

#[tokio::test]
async fn folder_crud_and_nonempty_delete() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "images" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let folder: serde_json::Value = resp.json().await.unwrap();
    let fid = folder["id"].as_str().unwrap().to_string();

    let dup = app
        .admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "images" }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup.status(), 409);

    let child = app
        .admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "2026", "parent_id": fid }))
        .send()
        .await
        .unwrap();
    assert_eq!(child.status(), 201);

    let del = app
        .admin(
            app.client
                .delete(app.url(&format!("/admin/media/folders/{fid}"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 409);

    let list = app
        .admin(app.client.get(app.url("/admin/media/folders")))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let arr: serde_json::Value = list.json().await.unwrap();
    assert!(arr
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "images"));
}

#[tokio::test]
async fn settings_masks_secrets_and_lists_providers() {
    let app = TestApp::spawn().await;

    let provs: serde_json::Value = app
        .admin(app.client.get(app.url("/admin/media/providers")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ids: Vec<&str> = provs
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"local") && ids.contains(&"s3"));

    let put = app
        .admin(app.client.put(app.url("/admin/media/settings")))
        .json(&serde_json::json!({ "provider": "local", "config": { "base_dir": "./media-data" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 204);

    let got: serde_json::Value = app
        .admin(app.client.get(app.url("/admin/media/settings")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(got["provider"], "local");
    assert_eq!(got["config"]["base_dir"], "./media-data");
}

#[tokio::test]
async fn asset_upload_and_raw_round_trip() {
    let app = TestApp::spawn().await;

    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let part = reqwest::multipart::Part::bytes(png.to_vec())
        .file_name("pixel.png")
        .mime_str("application/octet-stream")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let resp = app
        .admin(app.client.post(app.url("/admin/media/assets")))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let asset: serde_json::Value = resp.json().await.unwrap();
    let aid = asset["id"].as_str().unwrap().to_string();
    assert_eq!(asset["mime_type"], "image/png");
    assert_eq!(asset["width"], 1);
    assert_eq!(asset["height"], 1);

    let patch = app
        .admin(
            app.client
                .patch(app.url(&format!("/admin/media/assets/{aid}"))),
        )
        .json(&serde_json::json!({ "alt_text": "a pixel", "caption": "tiny" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch.status(), 200);
    let patched: serde_json::Value = patch.json().await.unwrap();
    assert_eq!(patched["alt_text"], "a pixel");

    let raw = app
        .admin(
            app.client
                .get(app.url(&format!("/admin/media/assets/{aid}/raw"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(raw.status(), 200);
    assert_eq!(raw.headers().get("content-type").unwrap(), "image/png");
    let body = raw.bytes().await.unwrap();
    assert_eq!(&body[..], png);

    let del = app
        .admin(
            app.client
                .delete(app.url(&format!("/admin/media/assets/{aid}"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 204);

    let gone = app
        .admin(
            app.client
                .get(app.url(&format!("/admin/media/assets/{aid}"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404);
}

#[tokio::test]
async fn folders_scope_all_returns_full_tree() {
    let app = TestApp::spawn().await;

    let root: serde_json::Value = app
        .admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "covers" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let rid = root["id"].as_str().unwrap().to_string();

    app.admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "2026", "parent_id": rid }))
        .send()
        .await
        .unwrap();

    let level: serde_json::Value = app
        .admin(app.client.get(app.url("/admin/media/folders")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        level.as_array().unwrap().len(),
        1,
        "root level shows one folder"
    );

    let all: serde_json::Value = app
        .admin(app.client.get(app.url("/admin/media/folders?scope=all")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        all.as_array().unwrap().len(),
        2,
        "scope=all shows every folder"
    );
}
