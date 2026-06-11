//! Integration test for `rustapi migrate`.
//! Spins two ephemeral Postgres containers: source (user tables) and target (Rustapi DB).

mod common;
use common::TestApp;
use rustapi::migrate::{
    apply::{apply_schema, copy_rows},
    inspect::{inspect_table, list_tables},
    map::Mapping,
    prompt::{ColumnDecision, TablePlan},
};
use rustapi_core::field::FieldKind;
use sqlx::postgres::PgPoolOptions;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres as PgImage;

async fn spawn_source(seed_sql: &str) -> (sqlx::PgPool, testcontainers::ContainerAsync<PgImage>) {
    let container = PgImage::default().start().await.unwrap();
    let url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap(),
    );
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    // Execute each statement separately (sqlx prepared statements don't support multi-statement strings).
    for stmt in seed_sql.split(';') {
        let stmt = stmt.trim();
        if !stmt.is_empty() {
            sqlx::query(stmt).execute(&pool).await.unwrap();
        }
    }
    (pool, container)
}

#[tokio::test]
async fn test_list_tables_excludes_system_tables() {
    let (pool, _c) = spawn_source(
        "CREATE TABLE articles (id SERIAL PRIMARY KEY, title TEXT, body TEXT);\
         CREATE TABLE pg_custom (x INT);",
    )
    .await;

    let tables = list_tables(&pool).await.unwrap();
    assert!(tables.contains(&"articles".to_string()));
    assert!(!tables.iter().any(|t| t.starts_with("pg_")));
}

#[tokio::test]
async fn test_inspect_table_basic() {
    let (pool, _c) = spawn_source(
        "CREATE TABLE posts (id SERIAL PRIMARY KEY, title TEXT NOT NULL, views INT, created_at TIMESTAMPTZ DEFAULT now(), updated_at TIMESTAMPTZ DEFAULT now());",
    )
    .await;

    let table = inspect_table(&pool, "posts").await.unwrap();
    assert_eq!(table.table_name, "posts");
    assert!(!table.columns.iter().any(|c| c.column_name == "id"));
    assert!(!table.columns.iter().any(|c| c.column_name == "created_at"));

    let title_col = table
        .columns
        .iter()
        .find(|c| c.column_name == "title")
        .unwrap();
    assert_eq!(title_col.mapping, Mapping::Field(FieldKind::String));
    assert!(!title_col.is_nullable);

    let views_col = table
        .columns
        .iter()
        .find(|c| c.column_name == "views")
        .unwrap();
    assert_eq!(views_col.mapping, Mapping::Field(FieldKind::Integer));
}

#[tokio::test]
async fn test_inspect_table_fk_becomes_relation() {
    let (pool, _c) = spawn_source(
        "CREATE TABLE authors (id SERIAL PRIMARY KEY, name TEXT);\
         CREATE TABLE posts (id SERIAL PRIMARY KEY, title TEXT, author_id INT REFERENCES authors(id));",
    )
    .await;

    let table = inspect_table(&pool, "posts").await.unwrap();
    let author_col = table
        .columns
        .iter()
        .find(|c| c.column_name == "author_id")
        .unwrap();
    assert_eq!(author_col.mapping, Mapping::Relation);
    assert_eq!(author_col.fk_table.as_deref(), Some("authors"));
}

#[tokio::test]
async fn test_inspect_table_enum() {
    let (pool, _c) = spawn_source(
        "CREATE TYPE status_enum AS ENUM ('draft', 'published', 'archived');\
         CREATE TABLE articles (id SERIAL PRIMARY KEY, status status_enum);",
    )
    .await;

    let table = inspect_table(&pool, "articles").await.unwrap();
    let status_col = table
        .columns
        .iter()
        .find(|c| c.column_name == "status")
        .unwrap();
    assert_eq!(status_col.mapping, Mapping::Field(FieldKind::Enum));
    assert_eq!(
        status_col.enum_values,
        vec!["draft", "published", "archived"]
    );
}

#[tokio::test]
async fn test_apply_schema_creates_content_type() {
    let app = TestApp::spawn().await;

    let plan = TablePlan {
        source_table: "articles".to_string(),
        content_type_name: "article".to_string(),
        display_name: "Article".to_string(),
        columns: vec![
            ColumnDecision {
                source_name: "title".to_string(),
                field_name: "title".to_string(),
                mapping: Mapping::Field(FieldKind::String),
                relation_target: None,
                required: true,
                enum_values: vec![],
            },
            ColumnDecision {
                source_name: "views".to_string(),
                field_name: "views".to_string(),
                mapping: Mapping::Field(FieldKind::Integer),
                relation_target: None,
                required: false,
                enum_values: vec![],
            },
        ],
    };

    apply_schema(&app.schemas, &[plan]).await.unwrap();

    let ct = app.schemas.registry().get("article").await;
    assert!(ct.is_some(), "content type 'article' should exist");
    let ct = ct.unwrap();
    assert!(ct
        .fields
        .iter()
        .any(|f| f.name == "title" && f.kind == FieldKind::String));
    assert!(ct
        .fields
        .iter()
        .any(|f| f.name == "views" && f.kind == FieldKind::Integer));
}

#[tokio::test]
async fn test_copy_rows_migrates_data() {
    // Source has TEXT columns only — copy_rows binds all values as Option<String>,
    // so non-text source columns would cause type-mismatch errors in the prepared
    // statement even for NULL values. This test uses text-only columns to exercise
    // the row-copy path without hitting that limitation.
    let (source_pool, _c) = spawn_source(
        "CREATE TABLE articles (id SERIAL PRIMARY KEY, title TEXT, slug TEXT, created_at TIMESTAMPTZ DEFAULT now(), updated_at TIMESTAMPTZ DEFAULT now());\
         INSERT INTO articles (title, slug) VALUES ('Hello', 'hello'), ('World', 'world');",
    )
    .await;

    let app = TestApp::spawn().await;

    let plan = TablePlan {
        source_table: "articles".to_string(),
        content_type_name: "article".to_string(),
        display_name: "Article".to_string(),
        columns: vec![
            ColumnDecision {
                source_name: "title".to_string(),
                field_name: "title".to_string(),
                mapping: Mapping::Field(FieldKind::String),
                relation_target: None,
                required: false,
                enum_values: vec![],
            },
            ColumnDecision {
                source_name: "slug".to_string(),
                field_name: "slug".to_string(),
                mapping: Mapping::Field(FieldKind::String),
                relation_target: None,
                required: false,
                enum_values: vec![],
            },
        ],
    };

    apply_schema(&app.schemas, &[plan.clone()]).await.unwrap();
    copy_rows(&source_pool, &app.pool, &[plan]).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ct_article")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
}
