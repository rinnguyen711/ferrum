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

/// CRITICAL regression: keyset paging on the DEFAULT sort (created_at, timestamptz)
/// must return HTTP 200, not 422 "operator does not exist: timestamptz < text".
/// The bug was that read_sort_value returned BoundValue::Str for Datetime columns,
/// then json_to_bound also decoded them as Str — the sqlx binder sent OID 25 (text)
/// instead of OID 1184 (timestamptz), causing Postgres to reject the comparison.
#[tokio::test]
async fn keyset_datetime_default_sort_pages_without_db_error() {
    let app = TestApp::spawn().await;

    // Create a `post` type with a `title` string field.
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // Insert 25 rows.
    let mut all_ids: HashSet<String> = HashSet::new();
    for i in 0..25 {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"title": format!("post-{i}")}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
        let entry: serde_json::Value = resp.json().await.unwrap();
        let id = entry["id"].as_str().expect("id").to_string();
        all_ids.insert(id);
    }
    assert_eq!(all_ids.len(), 25);

    // Page with DEFAULT sort (created_at:desc) via cursor=first, pageSize=10.
    let mut seen: HashSet<String> = HashSet::new();
    let mut cursor: Option<String> = Some("first".to_string());
    let max_iters = (25 / 10) * 2 + 4;
    let mut iterations = 0;

    loop {
        iterations += 1;
        assert!(
            iterations <= max_iters,
            "safety limit: still paginating after {max_iters} iterations"
        );

        let tok = cursor.as_deref().unwrap();
        // No explicit sort → defaults to created_at:desc (timestamptz column).
        let url = app.url(&format!("/api/post?cursor={tok}&pageSize=10"));
        let resp = app.admin(app.client.get(&url)).send().await.unwrap();
        assert_eq!(
            resp.status(),
            200,
            "keyset on created_at must not return 422 (was OID mismatch bug): {}",
            resp.text().await.unwrap()
        );

        let body: serde_json::Value = resp.json().await.unwrap();
        let data = body["data"].as_array().expect("data array");
        for item in data {
            let id = item["id"].as_str().expect("id").to_string();
            assert!(seen.insert(id.clone()), "duplicate id {id}");
        }

        let next = &body["meta"]["nextCursor"];
        if next.is_null() || next.as_str().is_none() {
            break;
        }
        cursor = Some(next.as_str().unwrap().to_string());
    }

    assert_eq!(seen.len(), 25, "expected 25 rows, got {}", seen.len());
    assert_eq!(seen, all_ids, "all inserted ids must appear exactly once");
}

/// Keyset ascending sort on an integer column pages all rows in order.
/// Exercises the `>` comparison path (asc) vs the `<` (desc) default.
#[tokio::test]
async fn keyset_ascending_sort_pages_all_rows() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "rank", "kind": "integer"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // Insert 30 rows with DISTINCT increasing rank values.
    let mut all_ids: HashSet<String> = HashSet::new();
    for i in 0..30_i64 {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"rank": i}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
        let entry: serde_json::Value = resp.json().await.unwrap();
        let id = entry["id"].as_str().expect("id").to_string();
        all_ids.insert(id);
    }
    assert_eq!(all_ids.len(), 30);

    let mut seen: HashSet<String> = HashSet::new();
    let mut cursor: Option<String> = Some("first".to_string());
    let max_iters = (30 / 10) * 2 + 4;
    let mut iterations = 0;

    loop {
        iterations += 1;
        assert!(
            iterations <= max_iters,
            "safety limit: still paginating after {max_iters} iterations"
        );

        let tok = cursor.as_deref().unwrap();
        let url = app.url(&format!("/api/post?sort=rank:asc&pageSize=10&cursor={tok}"));
        let resp = app.admin(app.client.get(&url)).send().await.unwrap();
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

        let body: serde_json::Value = resp.json().await.unwrap();
        let data = body["data"].as_array().expect("data array");
        for item in data {
            let id = item["id"].as_str().expect("id").to_string();
            assert!(seen.insert(id.clone()), "duplicate id {id}");
        }

        let next = &body["meta"]["nextCursor"];
        if next.is_null() || next.as_str().is_none() {
            break;
        }
        cursor = Some(next.as_str().unwrap().to_string());
    }

    assert_eq!(seen.len(), 30, "expected 30 rows, got {}", seen.len());
    assert_eq!(seen, all_ids, "all inserted ids must appear exactly once");
}

/// A non-scalar sort column (json kind) in keyset mode must return 422, not 200 or 500.
/// `json` kind is stored in a jsonb column, which IS a stored column (passes is_sortable),
/// so it reaches the keyset guard introduced by Fix 2 and must be rejected there with 422.
#[tokio::test]
async fn keyset_non_scalar_sort_rejected() {
    let app = TestApp::spawn().await;

    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "body", "kind": "json"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // Insert a couple rows.
    for _ in 0..2 {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"body": {"foo": "bar"}}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    }

    // Request keyset with a json-kind sort column — must be 422.
    let resp = app
        .admin(
            app.client
                .get(app.url("/api/post?cursor=first&sort=body:asc&pageSize=10")),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        422,
        "non-scalar sort in keyset mode must be rejected with 422, got: {}",
        resp.text().await.unwrap()
    );
}

/// A cursor token obtained with one sort direction must be rejected (422) when
/// replayed with a different sort direction — ensures sort-mismatch guard works
/// end-to-end through the HTTP layer.
#[tokio::test]
async fn keyset_sort_mismatch_cursor_rejected() {
    let app = TestApp::spawn().await;

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

    // Insert 15 rows.
    for i in 0..15_i64 {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"views": i}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    }

    // Obtain a real nextCursor with sort=views:desc.
    let body: serde_json::Value = app
        .admin(
            app.client
                .get(app.url("/api/post?sort=views:desc&pageSize=10&cursor=first")),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = body["meta"]["nextCursor"]
        .as_str()
        .expect("nextCursor must be present for a full page of 10 from 15 rows");

    // Replay the desc token with asc sort — must be rejected as sort-mismatch (422).
    let url = format!("/api/post?sort=views:asc&pageSize=10&cursor={token}");
    let resp = app
        .admin(app.client.get(app.url(&url)))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        422,
        "sort-direction mismatch must be rejected with 422, got: {}",
        resp.text().await.unwrap()
    );
}
