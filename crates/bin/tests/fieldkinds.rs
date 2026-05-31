//! Phase 2.5 fieldkinds integration tests. Boots a real Postgres per test via
//! testcontainers and drives the axum router in-process. Covers write
//! validation, filter operator surfaces, and `extend_enum_values` evolution
//! for the new enum / json / email / url / slug field kinds.

mod common;
use common::TestApp;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// `post` with `title` (required string) and `status` (enum, optional,
/// values: draft/published).
async fn create_post_with_enum(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {
                    "name": "status",
                    "kind": "enum",
                    "kind_meta": {"values": ["draft", "published"]}
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

/// `post` whose `status` enum is `required`. Used to probe required-null path.
async fn create_post_with_required_enum(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {
                    "name": "status",
                    "kind": "enum",
                    "required": true,
                    "kind_meta": {"values": ["draft", "published"]}
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

/// `doc` with `title` (required) and `meta` (json, optional).
async fn create_doc_with_json(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "doc",
            "display_name": "Doc",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "meta", "kind": "json", "kind_meta": {}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

/// `account` with `name` (required) and `email` (email kind).
async fn create_user_with_email(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "account",
            "display_name": "Account",
            "fields": [
                {"name": "name", "kind": "string", "required": true},
                {"name": "email", "kind": "email", "kind_meta": {}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

/// `link` with `label` and `target` (url kind).
async fn create_link_with_url(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "link",
            "display_name": "Link",
            "fields": [
                {"name": "label", "kind": "string", "required": true},
                {"name": "target", "kind": "url", "kind_meta": {}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

/// `article` with `title` and `slug` (unique slug kind).
async fn create_article_with_slug(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article",
            "display_name": "Article",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "slug", "kind": "slug", "unique": true, "kind_meta": {}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

// ---------------------------------------------------------------------------
// Enum: write-path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_enum_valid_value_returns_201() {
    let app = TestApp::spawn().await;
    create_post_with_enum(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "hi", "status": "draft"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert!(body["id"].is_string(), "row created, id present: {body}");
    assert_eq!(body["status"], "draft");
}

#[tokio::test]
async fn post_enum_invalid_value_returns_422_with_allowed() {
    let app = TestApp::spawn().await;
    create_post_with_enum(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "hi", "status": "missing"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "enum_value_not_allowed");
    let allowed = body["error"]["details"]["allowed"]
        .as_array()
        .expect("allowed array");
    let allowed_strs: Vec<&str> = allowed.iter().filter_map(|v| v.as_str()).collect();
    assert!(allowed_strs.contains(&"draft"), "{body}");
    assert!(allowed_strs.contains(&"published"), "{body}");
}

// ---------------------------------------------------------------------------
// Enum: filters
// ---------------------------------------------------------------------------

#[tokio::test]
async fn filter_enum_eq_matches() {
    let app = TestApp::spawn().await;
    create_post_with_enum(&app).await;

    for (t, s) in [("a", "draft"), ("b", "published"), ("c", "draft")] {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"title": t, "status": s}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    }

    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[status][$eq]=draft")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    // Filter applies at SQL level on the enum column so `total` is correct,
    // even though `decode_field` returns null for the value in the row payload
    // (see CONCERN on `post_enum_valid_value_returns_201`).
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn filter_enum_in_returns_union() {
    let app = TestApp::spawn().await;
    create_post_with_enum(&app).await;

    for (t, s) in [("a", "draft"), ("b", "published"), ("c", "draft")] {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&json!({"title": t, "status": s}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }

    let resp = app
        .admin(app.client.get(app.url(
            "/api/post?filters[status][$in][0]=draft&filters[status][$in][1]=published",
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 3);
}

// ---------------------------------------------------------------------------
// Enum: extend_enum_values evolution
// ---------------------------------------------------------------------------

#[tokio::test]
async fn patch_extend_enum_values_adds_value_writable() {
    let app = TestApp::spawn().await;
    create_post_with_enum(&app).await;

    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({
            "extend_enum_values": [{"field": "status", "append": ["archived"]}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "stale", "status": "archived"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn patch_extend_enum_values_on_non_enum_returns_422() {
    let app = TestApp::spawn().await;
    create_post_with_enum(&app).await;

    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({
            "extend_enum_values": [{"field": "title", "append": ["x"]}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn patch_extend_enum_values_duplicate_returns_422() {
    let app = TestApp::spawn().await;
    create_post_with_enum(&app).await;

    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({
            "extend_enum_values": [{"field": "status", "append": ["draft"]}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn patch_extend_enum_values_empty_returns_422() {
    let app = TestApp::spawn().await;
    create_post_with_enum(&app).await;

    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({
            "extend_enum_values": [{"field": "status", "append": []}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

// ---------------------------------------------------------------------------
// Required enum / add-required
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_required_enum_null_returns_422() {
    let app = TestApp::spawn().await;
    create_post_with_required_enum(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "x", "status": Value::Null}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn patch_add_required_enum_field_returns_422() {
    // Same shape as `patch_add_required_no_default_on_populated_table_*` in
    // integration_patch.rs: the required-on-add guard fires at PG (23502) only
    // when the table has at least one row, so we seed before PATCHing.
    let app = TestApp::spawn().await;
    create_post_with_enum(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "seed", "status": "draft"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/post")))
        .json(&json!({
            "add_fields": [{
                "name": "kind",
                "kind": "enum",
                "required": true,
                "kind_meta": {"values": ["a"]}
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
    assert!(body["error"]["details"]["db"]["code"].is_string(),
        "expected details.db.code present, body={body}");
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_json_accepts_nested_object() {
    let app = TestApp::spawn().await;
    create_doc_with_json(&app).await;

    let payload = json!({"title": "x", "meta": {"a": [1, 2, {"b": true}]}});
    let resp = app
        .admin(app.client.post(app.url("/api/doc")))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let id = body["id"].as_str().unwrap().to_string();

    let resp = app
        .admin(app.client.get(app.url(&format!("/api/doc/{id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["meta"], json!({"a": [1, 2, {"b": true}]}));
}

#[tokio::test]
async fn filter_json_null_true_matches_null_rows() {
    let app = TestApp::spawn().await;
    create_doc_with_json(&app).await;

    for (t, has_meta) in [("a", true), ("b", true), ("c", false), ("d", false)] {
        let body = if has_meta {
            json!({"title": t, "meta": {"k": 1}})
        } else {
            json!({"title": t})
        };
        let resp = app
            .admin(app.client.post(app.url("/api/doc")))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    }

    let resp = app
        .admin(app.client.get(app.url("/api/doc?filters[meta][$null]=true")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn filter_json_null_false_matches_nonnull_rows() {
    let app = TestApp::spawn().await;
    create_doc_with_json(&app).await;

    for (t, has_meta) in [("a", true), ("b", true), ("c", false), ("d", false)] {
        let body = if has_meta {
            json!({"title": t, "meta": {"k": 1}})
        } else {
            json!({"title": t})
        };
        let resp = app
            .admin(app.client.post(app.url("/api/doc")))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }

    let resp = app
        .admin(app.client.get(app.url("/api/doc?filters[meta][$null]=false")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn filter_json_eq_returns_validation_error() {
    let app = TestApp::spawn().await;
    create_doc_with_json(&app).await;

    let resp = app
        .admin(app.client.get(app.url("/api/doc?filters[meta][$eq]=hi")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

// ---------------------------------------------------------------------------
// Email
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_email_valid_returns_201() {
    let app = TestApp::spawn().await;
    create_user_with_email(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/account")))
        .json(&json!({"name": "alice", "email": "a@b.co"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["email"], "a@b.co");
}

#[tokio::test]
async fn post_email_invalid_returns_422_bad_email() {
    let app = TestApp::spawn().await;
    create_user_with_email(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/account")))
        .json(&json!({"name": "alice", "email": "nope"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "bad_email");
}

#[tokio::test]
async fn filter_email_contains_raw_substring() {
    let app = TestApp::spawn().await;
    create_user_with_email(&app).await;

    for (n, e) in [("a", "alice@x.com"), ("b", "bob@y.com")] {
        let resp = app
            .admin(app.client.post(app.url("/api/account")))
            .json(&json!({"name": n, "email": e}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }

    let resp = app
        .admin(app.client.get(app.url("/api/account?filters[email][$contains]=ali")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["name"], "a");
    assert_eq!(body["data"][0]["email"], "alice@x.com");
}

// ---------------------------------------------------------------------------
// URL
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_url_valid_https_returns_201() {
    let app = TestApp::spawn().await;
    create_link_with_url(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/link")))
        .json(&json!({"label": "home", "target": "https://example.com/x"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["target"], "https://example.com/x");
}

#[tokio::test]
async fn post_url_ftp_returns_422() {
    let app = TestApp::spawn().await;
    create_link_with_url(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/link")))
        .json(&json!({"label": "home", "target": "ftp://example.com"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "bad_url");
}

// ---------------------------------------------------------------------------
// Slug
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_slug_valid_returns_201() {
    let app = TestApp::spawn().await;
    create_article_with_slug(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({"title": "hi", "slug": "hello-world"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["slug"], "hello-world");
}

#[tokio::test]
async fn post_slug_invalid_returns_422() {
    let app = TestApp::spawn().await;
    create_article_with_slug(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({"title": "hi", "slug": "Bad Slug!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "bad_slug");
}

#[tokio::test]
async fn slug_unique_violation_returns_409() {
    let app = TestApp::spawn().await;
    create_article_with_slug(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({"title": "one", "slug": "dup"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({"title": "two", "slug": "dup"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409, "{}", resp.text().await.unwrap());
}

// ---------------------------------------------------------------------------
// Mixed-kind round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mixed_entry_with_all_new_kinds_round_trips() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "entry",
            "display_name": "Entry",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {
                    "name": "status",
                    "kind": "enum",
                    "kind_meta": {"values": ["draft", "published"]}
                },
                {"name": "meta", "kind": "json", "kind_meta": {}},
                {"name": "owner_email", "kind": "email", "kind_meta": {}},
                {"name": "homepage", "kind": "url", "kind_meta": {}},
                {"name": "slug", "kind": "slug", "kind_meta": {}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let payload = json!({
        "title": "hi",
        "status": "published",
        "meta": {"k": [1, 2, 3], "nested": {"flag": true}},
        "owner_email": "owner@example.com",
        "homepage": "https://example.com/home",
        "slug": "mixed-entry"
    });
    let resp = app
        .admin(app.client.post(app.url("/api/entry")))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let id = body["id"].as_str().unwrap().to_string();

    let resp = app
        .admin(app.client.get(app.url(&format!("/api/entry/{id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["title"], "hi");
    assert_eq!(body["status"], "published");
    assert_eq!(body["meta"], json!({"k": [1, 2, 3], "nested": {"flag": true}}));
    assert_eq!(body["owner_email"], "owner@example.com");
    assert_eq!(body["homepage"], "https://example.com/home");
    assert_eq!(body["slug"], "mixed-entry");
}
