//! End-to-end TOML sync against an ephemeral Postgres (testcontainers).

use rustapi_core::{Field, FieldKind, PatchContentType};
use rustapi_schema::{SchemaRegistry, SchemaService, SyncMode};
use rustapi_schema::sync::sync_from_path;
use sqlx::PgPool;
use std::io::Write;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres as PgImage;

/// Spawn ephemeral Postgres, run MIGRATOR, return a pool.
/// Harness copied from crates/bin/tests/common/mod.rs.
async fn setup_pool() -> PgPool {
    let pg = PgImage::default().start().await.expect("pg start");
    let port = pg.get_host_port_ipv4(5432).await.expect("pg port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("pool");

    rustapi_schema::MIGRATOR.run(&pool).await.expect("migrate");

    // Leak the container so it stays alive for the duration of the test.
    std::mem::forget(pg);

    pool
}

async fn service(pool: &PgPool) -> SchemaService {
    let registry = SchemaRegistry::new();
    registry.reload_from_db(pool).await.unwrap();
    SchemaService::new(pool.clone(), registry)
}

fn write_blog_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut a = std::fs::File::create(dir.path().join("author.toml")).unwrap();
    write!(
        a,
        r#"
[[content_type]]
name = "author"
display_name = "Author"
  [[content_type.field]]
  name = "name"
  kind = "string"
  required = true
"#
    )
    .unwrap();
    let mut p = std::fs::File::create(dir.path().join("post.toml")).unwrap();
    write!(
        p,
        r#"
[[content_type]]
name = "post"
display_name = "Post"
  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true
  [[content_type.field]]
  name = "author"
  kind = "relation"
  kind_meta = {{ target = "author", cardinality = "many_to_one" }}
"#
    )
    .unwrap();
    dir
}

#[tokio::test]
async fn sync_creates_types_marked_managed_and_idempotent() {
    let pool = setup_pool().await;
    let svc = service(&pool).await;
    let dir = write_blog_dir();
    let path = dir.path().to_str().unwrap();

    sync_from_path(&svc, path, SyncMode::Additive)
        .await
        .expect("first sync");
    let author = svc.registry().get("author").await.expect("author created");
    let post = svc.registry().get("post").await.expect("post created");
    assert!(author.managed(), "synced type must be marked managed");
    assert!(
        post.fields.iter().any(|f| f.name == "author"),
        "relation field present"
    );

    sync_from_path(&svc, path, SyncMode::Additive)
        .await
        .expect("second sync no-op");
    assert!(svc.registry().get("post").await.is_some());
}

#[tokio::test]
async fn additive_ignores_db_only_field_full_drops_it() {
    let pool = setup_pool().await;
    let svc = service(&pool).await;
    let dir = write_blog_dir();
    let path = dir.path().to_str().unwrap();
    sync_from_path(&svc, path, SyncMode::Additive).await.unwrap();

    let patch = PatchContentType {
        display_name: None,
        add_fields: vec![Field {
            name: "nickname".into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::json!({}),
        }],
        drop_fields: vec![],
        extend_enum_values: vec![],
        options: None,
    };
    svc.patch("author", patch).await.unwrap();

    sync_from_path(&svc, path, SyncMode::Additive).await.unwrap();
    assert!(svc
        .registry()
        .get("author")
        .await
        .unwrap()
        .fields
        .iter()
        .any(|f| f.name == "nickname"));

    sync_from_path(&svc, path, SyncMode::Full).await.unwrap();
    assert!(!svc
        .registry()
        .get("author")
        .await
        .unwrap()
        .fields
        .iter()
        .any(|f| f.name == "nickname"));
}

#[tokio::test]
async fn bad_toml_returns_error() {
    let pool = setup_pool().await;
    let svc = service(&pool).await;
    let dir = tempfile::tempdir().unwrap();
    let mut f = std::fs::File::create(dir.path().join("bad.toml")).unwrap();
    write!(f, "[[content_type]]\nname = \"x\"\n").unwrap();
    let err = sync_from_path(&svc, dir.path().to_str().unwrap(), SyncMode::Additive).await;
    assert!(err.is_err(), "invalid TOML must error (fail-fast on boot)");
}
