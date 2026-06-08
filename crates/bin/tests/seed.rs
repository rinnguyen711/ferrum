//! Seeding integration test. Boots a real Postgres via testcontainers, runs
//! the seed functions against a fresh DB, and asserts the default types and
//! sample rows exist with working relations. Also asserts idempotency.

mod common;
use common::TestApp;
use serde_json::Value;

#[tokio::test]
async fn seeds_default_types_and_data() {
    let app = TestApp::spawn().await;

    // Seed through the SAME SchemaService the router uses (shared registry),
    // so the in-process API observes the newly created types. A fresh DB means
    // the registry starts empty, so seeding runs.
    let schemas = &app.schemas;

    // First run seeds; returns Ok and creates types.
    rustapi::seed::seed_if_empty(&app.pool, schemas, true)
        .await
        .unwrap();

    // 3 content types exist.
    let resp = app
        .admin(app.client.get(app.url("/admin/content-types")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let types: Value = resp.json().await.unwrap();
    let names: Vec<&str> = types
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"author"), "author type: {names:?}");
    assert!(names.contains(&"category"), "category type: {names:?}");
    assert!(names.contains(&"article"), "article type: {names:?}");

    // Row counts.
    let count = |path: &str| {
        let app = &app;
        let path = path.to_string();
        async move {
            let r = app.admin(app.client.get(app.url(&path))).send().await.unwrap();
            assert_eq!(r.status(), 200, "{path}");
            let v: Value = r.json().await.unwrap();
            v["meta"]["total"].as_i64().unwrap()
        }
    };
    assert_eq!(count("/api/author").await, 4);
    assert_eq!(count("/api/category").await, 5);
    assert_eq!(count("/api/article?status=all").await, 10);

    // A populated article resolves its author to a real object.
    let r = app
        .admin(app.client.get(app.url("/api/article?populate=author&pageSize=1")))
        .send()
        .await
        .unwrap();
    let v: Value = r.json().await.unwrap();
    let first = &v["data"][0];
    assert!(
        first["author"]["name"].is_string(),
        "author should populate: {first}"
    );

    // Idempotent: second run is a no-op (returns without error, no duplicates).
    rustapi::seed::seed_if_empty(&app.pool, schemas, true)
        .await
        .unwrap();
    assert_eq!(count("/api/author").await, 4, "no duplicate authors");
}
