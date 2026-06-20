//! End-to-end integration suite for localization (Task 12).
//!
//! Exercises the real HTTP content API, GraphQL, and /admin/locales against a
//! real Postgres. NOTE on the harness: `TestApp` builds the router with a fresh
//! empty `LocaleRegistry` (it does NOT hydrate from `_locales` at boot). Every
//! `POST /admin/locales` calls the route's `reload()`, which re-loads ALL rows
//! from `_locales` into the registry — including the migration-seeded `en`.
//! So a single locale upsert is enough to populate the registry with `en` + the
//! new code, which every localized resolve step below depends on.

mod common;
use common::TestApp;
use serde_json::{json, Value};

/// Register a locale via the admin route. Returns the parsed response body
/// (the upserted locale object). Asserts 200.
async fn add_locale(app: &TestApp, code: &str, name: &str) -> Value {
    let resp = app
        .admin(app.client.post(app.url("/admin/locales")))
        .json(&json!({ "code": code, "name": name }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    resp.json().await.unwrap()
}

/// Create a localized `post` collection type: a `title` string + a unique
/// `slug`. `options.localized = true` makes the table carry document_id/locale
/// and a scoped UNIQUE(document_id, locale, slug).
async fn make_localized_post(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "options": { "localized": true },
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

/// POST /api/post?locale=<locale> with the given body. Returns (status, body).
async fn post_post(app: &TestApp, locale: &str, body: Value) -> (reqwest::StatusCode, Value) {
    let resp = app
        .admin(
            app.client
                .post(app.url(&format!("/api/post?locale={locale}"))),
        )
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.json().await.unwrap();
    (status, body)
}

/// GET /api/post/<document_id>?locale=<locale>. Returns (status, body).
async fn get_post(app: &TestApp, document_id: &str, locale: &str) -> (reqwest::StatusCode, Value) {
    let resp = app
        .admin(
            app.client
                .get(app.url(&format!("/api/post/{document_id}?locale={locale}"))),
        )
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.json().await.unwrap();
    (status, body)
}

// ---------------------------------------------------------------------------
// 1. Two-locale round trip.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn two_locales_round_trip() {
    let app = TestApp::spawn().await;
    add_locale(&app, "fr", "French").await; // also hydrates `en` into registry
    make_localized_post(&app).await;

    // en row → captures document_id.
    let (status, en) = post_post(&app, "en", json!({"title": "Hello", "slug": "hello"})).await;
    assert_eq!(status, 201, "{en}");
    assert_eq!(en["locale"], "en", "{en}");
    let document_id = en["document_id"].as_str().expect("document_id").to_string();

    // fr row for the SAME document.
    let (status, fr) = post_post(
        &app,
        "fr",
        json!({"title": "Bonjour", "slug": "bonjour", "document_id": document_id}),
    )
    .await;
    assert_eq!(status, 201, "{fr}");
    assert_eq!(fr["document_id"], document_id, "{fr}");
    assert_eq!(fr["locale"], "fr", "{fr}");

    // GET en.
    let (status, got) = get_post(&app, &document_id, "en").await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(got["title"], "Hello", "{got}");
    assert_eq!(got["locale"], "en", "{got}");

    // GET fr.
    let (status, got) = get_post(&app, &document_id, "fr").await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(got["title"], "Bonjour", "{got}");
    assert_eq!(got["locale"], "fr", "{got}");
}

// ---------------------------------------------------------------------------
// 2. Fallback to default when the requested locale has no row.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn fallback_to_default_on_missing() {
    let app = TestApp::spawn().await;
    // Register `de` (and via reload, `en`). Only an en row will exist.
    add_locale(&app, "de", "German").await;
    make_localized_post(&app).await;

    let (status, en) = post_post(&app, "en", json!({"title": "Hello", "slug": "hello"})).await;
    assert_eq!(status, 201, "{en}");
    let document_id = en["document_id"].as_str().unwrap().to_string();

    // de is registered but the document has no de row → fall back to en row.
    let (status, got) = get_post(&app, &document_id, "de").await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(got["title"], "Hello", "{got}");
    // Fallback row's own locale is en (the default), not the requested de.
    assert_eq!(
        got["locale"], "en",
        "fallback should surface the en row: {got}"
    );
}

// ---------------------------------------------------------------------------
// 3. Unknown (unregistered) locale → 422.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn unknown_locale_422() {
    let app = TestApp::spawn().await;
    add_locale(&app, "fr", "French").await;
    make_localized_post(&app).await;

    let (status, en) = post_post(&app, "en", json!({"title": "Hello", "slug": "hello"})).await;
    assert_eq!(status, 201, "{en}");
    let document_id = en["document_id"].as_str().unwrap().to_string();

    // zz is not a registered locale → resolve fails → 422.
    let (status, body) = get_post(&app, &document_id, "zz").await;
    assert_eq!(status, 422, "{body}");
}

// ---------------------------------------------------------------------------
// 4. Slug uniqueness is scoped per (document, locale).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn slug_unique_per_locale() {
    let app = TestApp::spawn().await;
    add_locale(&app, "fr", "French").await;
    make_localized_post(&app).await;

    // Document A: en row with slug "hello".
    let (status, a_en) = post_post(&app, "en", json!({"title": "Hello", "slug": "hello"})).await;
    assert_eq!(status, 201, "{a_en}");
    let doc_a = a_en["document_id"].as_str().unwrap().to_string();

    // Same document A, fr locale, SAME slug "hello" → allowed (different locale).
    let (status, a_fr) = post_post(
        &app,
        "fr",
        json!({"title": "Bonjour", "slug": "hello", "document_id": doc_a}),
    )
    .await;
    assert_eq!(
        status, 201,
        "scoped unique allows same slug across locales: {a_fr}"
    );

    // A SECOND, different document, en locale, SAME slug "hello" → allowed
    // (scope is document_id+locale+slug, so a different document_id is distinct).
    let (status, b_en) = post_post(&app, "en", json!({"title": "Hello 2", "slug": "hello"})).await;
    assert_eq!(
        status, 201,
        "different document may reuse the same locale+slug: {b_en}"
    );
    assert_ne!(b_en["document_id"], a_en["document_id"], "{b_en}");
}

// ---------------------------------------------------------------------------
// 5. Duplicate (document_id, locale) → 409 conflict.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn duplicate_document_locale_409() {
    let app = TestApp::spawn().await;
    add_locale(&app, "fr", "French").await;
    make_localized_post(&app).await;

    let (status, en) = post_post(&app, "en", json!({"title": "Hello", "slug": "hello"})).await;
    assert_eq!(status, 201, "{en}");
    let document_id = en["document_id"].as_str().unwrap().to_string();

    // Same document_id + same locale (en) → UNIQUE(document_id, locale) violation.
    let (status, dup) = post_post(
        &app,
        "en",
        json!({"title": "Dup", "slug": "dup", "document_id": document_id}),
    )
    .await;
    assert_eq!(
        status, 409,
        "duplicate (document_id, locale) must conflict: {dup}"
    );
}

// ---------------------------------------------------------------------------
// 6. List returns one row per document (requested locale, else default).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn list_returns_one_row_per_document() {
    let app = TestApp::spawn().await;
    add_locale(&app, "fr", "French").await;
    make_localized_post(&app).await;

    // Document A: en + fr.
    let (_, a_en) = post_post(&app, "en", json!({"title": "A-en", "slug": "a-en"})).await;
    let doc_a = a_en["document_id"].as_str().unwrap().to_string();
    post_post(
        &app,
        "fr",
        json!({"title": "A-fr", "slug": "a-fr", "document_id": doc_a}),
    )
    .await;

    // Document B: en only.
    let (_, b_en) = post_post(&app, "en", json!({"title": "B-en", "slug": "b-en"})).await;
    let doc_b = b_en["document_id"].as_str().unwrap().to_string();

    // List in fr: A → its fr row; B → falls back to its en row. 2 rows total.
    let resp = app
        .admin(app.client.get(app.url("/api/post?locale=fr")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["meta"]["locale"], "fr", "{body}");
    assert_eq!(body["meta"]["total"], 2, "{body}");

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 2, "{body}");

    // Find A's row → must be the fr title; B's row → fallback en title.
    let row_a = data
        .iter()
        .find(|r| r["document_id"] == json!(doc_a))
        .expect("doc A in list");
    assert_eq!(row_a["title"], "A-fr", "A resolves to its fr row: {body}");
    assert_eq!(row_a["locale"], "fr", "{body}");

    let row_b = data
        .iter()
        .find(|r| r["document_id"] == json!(doc_b))
        .expect("doc B in list");
    assert_eq!(row_b["title"], "B-en", "B falls back to its en row: {body}");
    assert_eq!(row_b["locale"], "en", "{body}");
}

// ---------------------------------------------------------------------------
// 7. Localize an existing (non-localized) type via PATCH; existing rows are
//    backfilled (document_id = id, locale = default).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn localize_existing_type() {
    let app = TestApp::spawn().await;
    add_locale(&app, "fr", "French").await; // hydrate registry with en (+fr)

    // Non-localized `note` with a title.
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "note",
            "display_name": "Note",
            "fields": [{"name": "title", "kind": "string", "required": true}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // Create a row (non-localized → row id).
    let resp = app
        .admin(app.client.post(app.url("/api/note")))
        .json(&json!({"title": "first"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    // PATCH to enable localization → backfills existing rows.
    let resp = app
        .admin(app.client.patch(app.url("/admin/content-types/note")))
        .json(&json!({ "options": { "localized": true } }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // GET by what is now the document_id (== old id), locale en.
    let resp = app
        .admin(
            app.client
                .get(app.url(&format!("/api/note/{id}?locale=en"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let got: Value = resp.json().await.unwrap();
    assert_eq!(got["title"], "first", "{got}");
    assert_eq!(
        got["document_id"],
        json!(id),
        "backfill sets document_id = id: {got}"
    );
    assert_eq!(got["locale"], "en", "backfill sets locale = default: {got}");
}

// ---------------------------------------------------------------------------
// 8. Per-locale publish independence.
//
// NOTE: the publish endpoint is `POST /api/:type/:id/publish` keyed by ROW id
// and takes NO `?locale=`. Publish-by-locale is NOT wired: the path id is a
// content-type ROW id (the underlying `publish()` SQL targets `WHERE id = $1`),
// not a document_id. This test asserts the ACTUAL current behavior: publishing
// a specific locale's ROW id flips only that one row's published_at, leaving
// the other locale row a draft. It also confirms a localized list with
// `?status=published` then returns only the published locale's row (or its
// fallback). Document-id-scoped publish is a known gap, documented here.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn per_locale_publish_independence() {
    let app = TestApp::spawn().await;
    add_locale(&app, "fr", "French").await;

    // Localized type WITH draft_publish.
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "options": { "localized": true, "draft_publish": true },
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "slug", "kind": "slug", "unique": true, "kind_meta": {}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // en row → both rows start as drafts (published_at null).
    let (status, en) = post_post(&app, "en", json!({"title": "Hello", "slug": "hello"})).await;
    assert_eq!(status, 201, "{en}");
    let document_id = en["document_id"].as_str().unwrap().to_string();
    let en_row_id = en["id"].as_str().unwrap().to_string();
    assert!(en["published_at"].is_null(), "{en}");

    let (status, fr) = post_post(
        &app,
        "fr",
        json!({"title": "Bonjour", "slug": "bonjour", "document_id": document_id}),
    )
    .await;
    assert_eq!(status, 201, "{fr}");
    let fr_row_id = fr["id"].as_str().unwrap().to_string();
    assert!(fr["published_at"].is_null(), "{fr}");

    // Publish ONLY the fr row (by its row id — the publish endpoint is row-keyed).
    let resp = app
        .admin(
            app.client
                .post(app.url(&format!("/api/post/{fr_row_id}/publish"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let pubd: Value = resp.json().await.unwrap();
    assert!(
        !pubd["published_at"].is_null(),
        "fr row now published: {pubd}"
    );
    assert_eq!(pubd["locale"], "fr", "{pubd}");

    // The en row is still a draft (independent publish state).
    assert_ne!(en_row_id, fr_row_id);
    let resp = app
        .admin(app.client.get(app.url("/api/post?locale=en&status=draft")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let drafts: Value = resp.json().await.unwrap();
    // en row is a draft → present under status=draft for locale en.
    assert_eq!(drafts["meta"]["total"], 1, "en row still draft: {drafts}");
    assert_eq!(drafts["data"][0]["locale"], "en", "{drafts}");

    // Localized published list in fr returns the published fr row.
    let resp = app
        .admin(
            app.client
                .get(app.url("/api/post?locale=fr&status=published")),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let published: Value = resp.json().await.unwrap();
    assert_eq!(published["meta"]["total"], 1, "{published}");
    assert_eq!(published["data"][0]["locale"], "fr", "{published}");
    assert_eq!(published["data"][0]["title"], "Bonjour", "{published}");
}

// ---------------------------------------------------------------------------
// 9. GraphQL locale matches REST: list with locale "fr" returns the fr row,
//    falling back to en where fr is absent.
//
// For localized types the GraphQL output object (`build_output_object`) now
// registers `document_id` and `locale` as selectable String fields, mirroring
// the REST surface (row_to_json). We assert locale resolution both directly via
// the `locale` field (A's row resolves to "fr", B falls back to "en") and via
// the locale-resolved `title` value.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn graphql_locale_matches_rest() {
    let app = TestApp::spawn().await;
    add_locale(&app, "fr", "French").await;
    make_localized_post(&app).await;

    // Document A: en + fr.
    let (_, a_en) = post_post(&app, "en", json!({"title": "A-en", "slug": "a-en"})).await;
    let doc_a = a_en["document_id"].as_str().unwrap().to_string();
    post_post(
        &app,
        "fr",
        json!({"title": "A-fr", "slug": "a-fr", "document_id": doc_a}),
    )
    .await;

    // Document B: en only.
    post_post(&app, "en", json!({"title": "B-en", "slug": "b-en"})).await;

    // GraphQL: posts(locale: "fr"). document_id/locale are now selectable on
    // localized types, so read them directly.
    let r = app
        .admin(app.client.post(app.url("/api/graphql")))
        .json(&json!({
            "query": "{ posts(locale:\"fr\"){ data{ id document_id locale title } meta{ total } } }",
            "variables": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "{}", r.text().await.unwrap());
    let body: Value = r.json().await.unwrap();
    assert!(body["errors"].is_null(), "{body}");
    // One row per document (locale-collapsed), matching REST.
    assert_eq!(body["data"]["posts"]["meta"]["total"], 2, "{body}");

    let rows = body["data"]["posts"]["data"].as_array().unwrap();
    let titles: Vec<&str> = rows.iter().filter_map(|r| r["title"].as_str()).collect();
    // A resolves to its fr title; B falls back to its en title — same resolution
    // as the REST `list_returns_one_row_per_document` test.
    assert!(titles.contains(&"A-fr"), "GraphQL A resolves fr: {body}");
    assert!(
        titles.contains(&"B-en"),
        "GraphQL B falls back to en: {body}"
    );
    assert!(
        !titles.contains(&"A-en"),
        "fr row must shadow en row: {body}"
    );

    // The newly-exposed `locale` field reflects the resolved locale per row:
    // A's row resolved to fr, B fell back to en. document_id matches doc_a for A.
    let row_a = rows
        .iter()
        .find(|r| r["document_id"] == json!(doc_a))
        .unwrap_or_else(|| panic!("document A present with selectable document_id: {body}"));
    assert_eq!(row_a["locale"], "fr", "A's row resolved to fr: {body}");
    assert_eq!(row_a["title"], "A-fr", "{body}");

    let row_b = rows
        .iter()
        .find(|r| r["title"] == json!("B-en"))
        .unwrap_or_else(|| panic!("document B present: {body}"));
    assert_eq!(row_b["locale"], "en", "B falls back to en: {body}");
    assert!(
        row_b["document_id"].is_string(),
        "B exposes a document_id: {body}"
    );
}

// ---------------------------------------------------------------------------
// 10. /admin/locales CRUD (admin-gated).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn admin_locales_crud() {
    let app = TestApp::spawn().await;

    // GET → seeded default `en` is present.
    let resp = app
        .admin(app.client.get(app.url("/admin/locales")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let codes: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["code"].as_str())
        .collect();
    assert!(codes.contains(&"en"), "seeded en present: {body}");

    // POST fr.
    let fr = add_locale(&app, "fr", "French").await;
    assert_eq!(fr["code"], "fr", "{fr}");
    assert_eq!(fr["name"], "French", "{fr}");

    // GET again → fr present.
    let resp = app
        .admin(app.client.get(app.url("/admin/locales")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let codes: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["code"].as_str())
        .collect();
    assert!(codes.contains(&"fr"), "fr present after upsert: {body}");

    // DELETE the default en → 422 (cannot delete default).
    let resp = app
        .admin(app.client.delete(app.url("/admin/locales/en")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());

    // DELETE fr → 204.
    let resp = app
        .admin(app.client.delete(app.url("/admin/locales/fr")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204, "{}", resp.text().await.unwrap());

    // GET again → fr gone, en remains.
    let resp = app
        .admin(app.client.get(app.url("/admin/locales")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let codes: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["code"].as_str())
        .collect();
    assert!(!codes.contains(&"fr"), "fr removed: {body}");
    assert!(codes.contains(&"en"), "{body}");
}
