mod common;
use common::TestApp;
use serde_json::json;
use std::collections::HashSet;

/// Proves keyset pagination returns every row exactly once even when the sort
/// column has duplicate values across all 50 entries.  The id tiebreak must
/// prevent any row from appearing on two pages or being skipped at a page seam.
#[tokio::test]
async fn keyset_pages_all_rows_once_with_duplicate_sort_values() {
    let app = TestApp::spawn().await;

    // Create content type with an integer field.
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "views", "kind": "integer"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // Insert 50 entries ALL with the SAME views value to stress the tiebreak.
    let mut created: HashSet<String> = HashSet::new();
    for _ in 0..50 {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"views": 10}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
        let entry: serde_json::Value = resp.json().await.unwrap();
        let id = entry["id"]
            .as_str()
            .expect("id in created entry")
            .to_string();
        created.insert(id);
    }
    assert_eq!(
        created.len(),
        50,
        "all 50 inserts must produce distinct ids"
    );

    // Page through using keyset cursor, pageSize=10.
    let mut seen: HashSet<String> = HashSet::new();
    let mut cursor: Option<String> = None;
    let mut iterations = 0;

    loop {
        iterations += 1;
        assert!(
            iterations <= 20,
            "safety limit: still paginating after 20 iterations — possible infinite loop"
        );

        let url = match &cursor {
            Some(tok) => app.url(&format!(
                "/api/post?sort=views:desc&pageSize=10&cursor={}",
                tok
            )),
            None => app.url("/api/post?sort=views:desc&pageSize=10"),
        };

        let resp = app.admin(app.client.get(&url)).send().await.unwrap();
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

        let body: serde_json::Value = resp.json().await.unwrap();
        let data = body["data"].as_array().expect("data array in response");

        for item in data {
            let id = item["id"]
                .as_str()
                .expect("id field in list item")
                .to_string();
            assert!(
                seen.insert(id.clone()),
                "row {id} returned on two pages (keyset seam bug)"
            );
        }

        // Advance cursor or break on last page.
        let next = &body["meta"]["nextCursor"];
        if next.is_null() || next.as_str().is_none() {
            break;
        }
        cursor = Some(next.as_str().unwrap().to_string());
    }

    assert_eq!(
        seen.len(),
        50,
        "expected 50 unique rows across all pages, got {}",
        seen.len()
    );
    assert_eq!(
        seen, created,
        "rows seen via pagination must exactly match the rows that were inserted"
    );
}

/// Proves that ?withCount=false omits the `total` key from meta, and that the
/// default (no withCount param) includes it.
#[tokio::test]
async fn with_count_false_omits_total() {
    let app = TestApp::spawn().await;

    // Create content type.
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "views", "kind": "integer"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // Insert 5 entries.
    for i in 0..5 {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"views": i}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    }

    // withCount=false → total key must be absent from meta.
    let resp = app
        .admin(app.client.get(app.url("/api/post?withCount=false")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["meta"].get("total").is_none(),
        "expected `total` absent when withCount=false, got meta: {}",
        body["meta"]
    );

    // Default (no withCount) → total must be present and equal 5.
    let resp = app
        .admin(app.client.get(app.url("/api/post")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["meta"]["total"], 5,
        "expected total=5 in default offset mode, got meta: {}",
        body["meta"]
    );
}
