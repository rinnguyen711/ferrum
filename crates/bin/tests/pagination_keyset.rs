mod common;
use common::TestApp;
use serde_json::json;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Shared seed helper: create the `post` type with an integer `views` field,
// insert `n` entries all with the given `views` value, return their ids.
// ---------------------------------------------------------------------------
async fn seed(app: &TestApp, n: usize, views: i64) -> HashSet<String> {
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

    let mut ids: HashSet<String> = HashSet::new();
    for _ in 0..n {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"views": views}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
        let entry: serde_json::Value = resp.json().await.unwrap();
        let id = entry["id"]
            .as_str()
            .expect("id in created entry")
            .to_string();
        ids.insert(id);
    }
    assert_eq!(ids.len(), n, "all {n} inserts must produce distinct ids");
    ids
}

/// Proves keyset pagination returns every row exactly once even when the sort
/// column has duplicate values across all 50 entries.  The id tiebreak must
/// prevent any row from appearing on two pages or being skipped at a page seam.
/// Uses `cursor=first` to start keyset paging per the new contract.
#[tokio::test]
async fn keyset_pages_all_rows_once_with_duplicate_sort_values() {
    let app = TestApp::spawn().await;

    let created = seed(&app, 50, 10).await;

    // Page through using keyset cursor, pageSize=10.
    // Start with `cursor=first` (new sentinel — start keyset mode from beginning).
    let mut seen: HashSet<String> = HashSet::new();
    let mut cursor: Option<String> = Some("first".to_string());

    // pages needed = 50/10 = 5; headroom for off-by-one
    let max_iters = (50 / 10) * 2 + 2;
    let mut iterations = 0;

    loop {
        iterations += 1;
        assert!(
            iterations <= max_iters,
            "safety limit: still paginating after {max_iters} iterations — possible infinite loop"
        );

        let tok = cursor.as_deref().unwrap();
        let url = app.url(&format!(
            "/api/post?sort=views:desc&pageSize=10&cursor={}",
            tok
        ));

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

    seed(&app, 5, 0).await;

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

/// Locks the contract: plain offset requests (no cursor param) must NEVER emit
/// `nextCursor`, even when the page is full. Offset mode returns `total` instead.
#[tokio::test]
async fn offset_mode_omits_next_cursor() {
    let app = TestApp::spawn().await;

    seed(&app, 5, 42).await;

    let body: serde_json::Value = app
        .admin(app.client.get(app.url("/api/post?pageSize=10")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(
        body["meta"].get("nextCursor").is_none(),
        "offset mode must NOT emit nextCursor; got {:?}",
        body["meta"]
    );
    assert_eq!(
        body["meta"]["total"],
        serde_json::json!(5),
        "offset mode must emit total; got meta: {}",
        body["meta"]
    );
}
