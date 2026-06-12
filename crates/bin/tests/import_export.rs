mod common;
use common::TestApp;
use serde_json::json;

/// Create a `post` content type with a single string field `title`.
async fn setup_post_type(app: &TestApp) {
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
}

/// Create one entry and return its id as a String.
async fn create_entry(app: &TestApp, title: &str) -> String {
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": title}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn export_selected_ids_returns_csv() {
    let app = TestApp::spawn().await;
    setup_post_type(&app).await;
    let id1 = create_entry(&app, "First").await;
    let id2 = create_entry(&app, "Second").await;
    let _id3 = create_entry(&app, "Third").await; // not exported

    let url = app.url(&format!(
        "/admin/content-types/post/entries/export?ids={id1},{id2}"
    ));
    let resp = app.admin(app.client.get(&url)).send().await.unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/csv"));

    let body = resp.text().await.unwrap();
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(body.as_bytes());
    let hdrs = rdr.headers().unwrap().clone();
    let title_idx = hdrs.iter().position(|h| h == "title").unwrap();
    let id_idx = hdrs.iter().position(|h| h == "id").unwrap();
    let records: Vec<csv::StringRecord> = rdr.records().collect::<Result<_, _>>().unwrap();
    assert_eq!(records.len(), 2);
    let exported_ids: Vec<&str> = records.iter().map(|r| r.get(id_idx).unwrap()).collect();
    assert!(exported_ids.contains(&id1.as_str()));
    assert!(exported_ids.contains(&id2.as_str()));
    let titles: Vec<&str> = records.iter().map(|r| r.get(title_idx).unwrap()).collect();
    assert!(titles.contains(&"First"));
    assert!(titles.contains(&"Second"));
}

#[tokio::test]
async fn export_no_ids_returns_422() {
    let app = TestApp::spawn().await;
    setup_post_type(&app).await;
    let resp = app
        .admin(
            app.client
                .get(app.url("/admin/content-types/post/entries/export")),
        )
        .send()
        .await
        .unwrap();
    // Server returns 422 for validation errors (ids required)
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn export_unknown_type_returns_404() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.get(
            app.url("/admin/content-types/nonexistent/entries/export?ids=00000000-0000-0000-0000-000000000000"),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn import_new_rows_inserts() {
    let app = TestApp::spawn().await;
    setup_post_type(&app).await;

    let csv = "id,title\n,Hello Import\n,Second Import\n";
    let part = reqwest::multipart::Part::bytes(csv.as_bytes().to_vec())
        .file_name("data.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let resp = app
        .admin(
            app.client
                .post(app.url("/admin/content-types/post/entries/import"))
                .multipart(form),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["inserted"], 2);
    assert_eq!(body["updated"], 0);
    assert_eq!(body["errors"].as_array().unwrap().len(), 0);

    // Verify entries exist in DB via list endpoint
    let list_resp = app
        .admin(app.client.get(app.url("/api/post")))
        .send()
        .await
        .unwrap();
    let list: serde_json::Value = list_resp.json().await.unwrap();
    let entries = list["data"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn import_existing_id_upserts() {
    let app = TestApp::spawn().await;
    setup_post_type(&app).await;
    let id = create_entry(&app, "Original").await;

    let csv = format!("id,title\n{id},Updated Title\n");
    let part = reqwest::multipart::Part::bytes(csv.as_bytes().to_vec())
        .file_name("data.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let resp = app
        .admin(
            app.client
                .post(app.url("/admin/content-types/post/entries/import"))
                .multipart(form),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["inserted"], 0);
    assert_eq!(body["updated"], 1);

    let get_resp = app
        .admin(app.client.get(app.url(&format!("/api/post/{id}"))))
        .send()
        .await
        .unwrap();
    let entry: serde_json::Value = get_resp.json().await.unwrap();
    assert_eq!(entry["title"], "Updated Title");
}

#[tokio::test]
async fn import_partial_errors_continue() {
    let app = TestApp::spawn().await;
    setup_post_type(&app).await;

    // CSV has an unknown field — body_to_binds rejects all rows with unknown fields.
    // Tests that errors are collected per-row (not a fatal 500).
    let csv = "id,title,bogus\n,Row1,x\n,Row2,y\n";
    let part = reqwest::multipart::Part::bytes(csv.as_bytes().to_vec())
        .file_name("data.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);
    let resp = app
        .admin(
            app.client
                .post(app.url("/admin/content-types/post/entries/import"))
                .multipart(form),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    // All rows fail but response is 200 with error list (not a 500)
    assert_eq!(body["inserted"], 0);
    assert_eq!(body["errors"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn import_empty_file_returns_422() {
    let app = TestApp::spawn().await;
    setup_post_type(&app).await;

    let part = reqwest::multipart::Part::bytes(vec![])
        .file_name("data.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);
    let resp = app
        .admin(
            app.client
                .post(app.url("/admin/content-types/post/entries/import"))
                .multipart(form),
        )
        .send()
        .await
        .unwrap();
    // Server returns 422 for validation errors (empty file)
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn import_too_many_rows_returns_422() {
    let app = TestApp::spawn().await;
    setup_post_type(&app).await;

    let mut csv_data = "id,title\n".to_string();
    for i in 0..1001 {
        csv_data.push_str(&format!(",Row {i}\n"));
    }
    let part = reqwest::multipart::Part::bytes(csv_data.into_bytes())
        .file_name("data.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);
    let resp = app
        .admin(
            app.client
                .post(app.url("/admin/content-types/post/entries/import"))
                .multipart(form),
        )
        .send()
        .await
        .unwrap();
    // Server returns 422 for validation errors (too many rows)
    assert_eq!(resp.status(), 422);
}
