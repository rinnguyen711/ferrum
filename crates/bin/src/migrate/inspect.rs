use crate::migrate::map::{infer, Mapping};
use sqlx::PgPool;

/// A column as discovered in the source DB.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SourceColumn {
    pub column_name: String,
    pub pg_type: String,
    pub udt_name: String,
    pub is_nullable: bool,
    pub is_fk: bool,
    pub fk_table: Option<String>,
    pub mapping: Mapping,
    pub enum_values: Vec<String>,
}

/// A table as discovered in the source DB.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SourceTable {
    pub table_name: String,
    pub columns: Vec<SourceColumn>,
}

#[allow(dead_code)]
const CT_PREFIX: &str = "ct_";

#[allow(dead_code)]
const RUSTAPI_TABLES: &[&str] = &[
    "_sqlx_migrations",
    "_media_assets",
    "_api_tokens",
    "_webhook_endpoints",
    "_webhook_deliveries",
    "content_types",
    "components",
    "users",
];

#[allow(dead_code)]
pub async fn list_tables(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_type = 'BASE TABLE'
        ORDER BY table_name
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter(|t| {
            !t.starts_with(CT_PREFIX)
                && !RUSTAPI_TABLES.contains(&t.as_str())
                && !t.starts_with("pg_")
        })
        .collect())
}

#[allow(dead_code)]
async fn load_fk_map(
    pool: &PgPool,
    table_name: &str,
) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT kcu.column_name, ccu.table_name AS referenced_table
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
          AND tc.table_schema = kcu.table_schema
        JOIN information_schema.constraint_column_usage ccu
          ON ccu.constraint_name = tc.constraint_name
          AND ccu.table_schema = tc.table_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
          AND tc.table_name = $1
          AND tc.table_schema = 'public'
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().collect())
}

#[allow(dead_code)]
async fn load_enum_values(pool: &PgPool, udt_name: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT enumlabel::text
        FROM pg_enum
        JOIN pg_type ON pg_type.oid = pg_enum.enumtypid
        WHERE pg_type.typname = $1
        ORDER BY enumsortorder
        "#,
    )
    .bind(udt_name)
    .fetch_all(pool)
    .await
}

#[allow(dead_code)]
pub async fn inspect_table(pool: &PgPool, table_name: &str) -> Result<SourceTable, sqlx::Error> {
    let fk_map = load_fk_map(pool, table_name).await?;

    let rows = sqlx::query_as::<_, (String, String, String, String)>(
        r#"
        SELECT column_name, data_type, udt_name, is_nullable
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = $1
        ORDER BY ordinal_position
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await?;

    let mut columns = Vec::new();
    for (column_name, pg_type, udt_name, is_nullable) in rows {
        if matches!(column_name.as_str(), "id" | "created_at" | "updated_at") {
            continue;
        }

        let is_fk = fk_map.contains_key(&column_name);
        let fk_table = fk_map.get(&column_name).cloned();
        let mapping = infer(&pg_type, &udt_name, is_fk);

        let enum_values = if mapping == Mapping::Field(rustapi_core::field::FieldKind::Enum) {
            load_enum_values(pool, &udt_name).await.unwrap_or_default()
        } else {
            vec![]
        };

        columns.push(SourceColumn {
            column_name,
            pg_type,
            udt_name,
            is_nullable: is_nullable == "YES",
            is_fk,
            fk_table,
            mapping,
            enum_values,
        });
    }

    Ok(SourceTable {
        table_name: table_name.to_string(),
        columns,
    })
}
