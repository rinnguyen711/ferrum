//! DDL string builders. All identifiers go through `ident::quote_ident` /
//! `ident::table_name`. Default values are emitted as SQL literals.

use crate::ident::{quote_ident, table_name, IdentError};
use rustapi_core::{ContentType, Field, FieldKind};

#[derive(Debug, thiserror::Error)]
pub enum DdlError {
    #[error(transparent)]
    Ident(#[from] IdentError),
}

/// `CREATE TABLE ct_<name> ( ... )`
pub fn create_table(ct: &ContentType) -> Result<String, DdlError> {
    let table = table_name(&ct.name)?;
    let mut cols: Vec<String> = vec![
        r#""id" UUID PRIMARY KEY DEFAULT gen_random_uuid()"#.into(),
        r#""created_at" TIMESTAMPTZ NOT NULL DEFAULT now()"#.into(),
        r#""updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()"#.into(),
    ];
    for f in &ct.fields {
        cols.push(column_def(f)?);
    }
    let body = cols.join(", ");
    Ok(format!("CREATE TABLE {table} ({body})"))
}

/// `ALTER TABLE ct_<name> ADD COLUMN ...`
pub fn add_column(ct_name: &str, field: &Field) -> Result<String, DdlError> {
    let table = table_name(ct_name)?;
    let def = column_def(field)?;
    Ok(format!("ALTER TABLE {table} ADD COLUMN {def}"))
}

/// `ALTER TABLE ct_<name> DROP COLUMN "<col>"`
pub fn drop_column(ct_name: &str, col: &str) -> Result<String, DdlError> {
    let table = table_name(ct_name)?;
    let c = quote_ident(col)?;
    Ok(format!("ALTER TABLE {table} DROP COLUMN {c}"))
}

/// `DROP TABLE ct_<name>`
pub fn drop_table(ct_name: &str) -> Result<String, DdlError> {
    let table = table_name(ct_name)?;
    Ok(format!("DROP TABLE {table}"))
}

fn column_def(f: &Field) -> Result<String, DdlError> {
    let col = quote_ident(&f.name)?;
    let ty = sql_type(f);
    let mut s = format!("{col} {ty}");
    if !f.default.is_null() {
        s.push_str(" DEFAULT ");
        s.push_str(&render_default(f));
    }
    if f.required {
        s.push_str(" NOT NULL");
    }
    if f.unique {
        s.push_str(" UNIQUE");
    }
    Ok(s)
}

fn sql_type(f: &Field) -> String {
    match f.kind {
        FieldKind::String => {
            let n = f.effective_max_length();
            format!("VARCHAR({n})")
        }
        FieldKind::Text => "TEXT".into(),
        FieldKind::Integer => "BIGINT".into(),
        FieldKind::Float => "DOUBLE PRECISION".into(),
        FieldKind::Boolean => "BOOLEAN".into(),
        FieldKind::Datetime => "TIMESTAMPTZ".into(),
        // FieldKind is #[non_exhaustive]; future kinds (relation/json/enum) added in phase 2.
        _ => "TEXT".into(),
    }
}

fn render_default(f: &Field) -> String {
    // Safe because Field::validate (called before this) has confirmed
    // default coerces to the field's kind.
    match (&f.kind, &f.default) {
        (FieldKind::String | FieldKind::Text, serde_json::Value::String(s)) => {
            let escaped = s.replace('\'', "''");
            format!("'{escaped}'")
        }
        (FieldKind::Datetime, serde_json::Value::String(s)) => {
            let escaped = s.replace('\'', "''");
            format!("'{escaped}'::timestamptz")
        }
        (FieldKind::Integer, v) | (FieldKind::Float, v) => v.to_string(),
        (FieldKind::Boolean, serde_json::Value::Bool(b)) => if *b { "TRUE" } else { "FALSE" }.into(),
        _ => "NULL".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn field(name: &str, kind: FieldKind) -> Field {
        Field {
            name: name.into(),
            kind,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        }
    }

    fn ct(fields: Vec<Field>) -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn create_table_basic() {
        let sql = create_table(&ct(vec![field("title", FieldKind::String)])).unwrap();
        assert!(sql.starts_with("CREATE TABLE \"ct_post\" ("));
        assert!(sql.contains("\"id\" UUID PRIMARY KEY"));
        assert!(sql.contains("\"created_at\" TIMESTAMPTZ NOT NULL DEFAULT now()"));
        assert!(sql.contains("\"updated_at\" TIMESTAMPTZ NOT NULL DEFAULT now()"));
        assert!(sql.contains("\"title\" VARCHAR(255)"));
    }

    #[test]
    fn create_table_with_required_unique_default() {
        let mut f = field("slug", FieldKind::String);
        f.required = true;
        f.unique = true;
        f.default = json!("untitled");
        f.max_length = Some(64);
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(sql.contains("\"slug\" VARCHAR(64) DEFAULT 'untitled' NOT NULL UNIQUE"));
    }

    #[test]
    fn create_table_escapes_default_quotes() {
        let mut f = field("note", FieldKind::Text);
        f.default = json!("it's fine");
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(sql.contains("DEFAULT 'it''s fine'"));
    }

    #[test]
    fn add_column_alter() {
        let f = field("subtitle", FieldKind::Text);
        let sql = add_column("post", &f).unwrap();
        assert_eq!(sql, "ALTER TABLE \"ct_post\" ADD COLUMN \"subtitle\" TEXT");
    }

    #[test]
    fn drop_column_alter() {
        let sql = drop_column("post", "subtitle").unwrap();
        assert_eq!(sql, "ALTER TABLE \"ct_post\" DROP COLUMN \"subtitle\"");
    }

    #[test]
    fn drop_table_works() {
        assert_eq!(drop_table("post").unwrap(), "DROP TABLE \"ct_post\"");
    }

    #[test]
    fn rejects_bad_table_name() {
        let bad = ContentType { name: "Bad".into(), ..ct(vec![field("title", FieldKind::String)]) };
        assert!(create_table(&bad).is_err());
    }
}
