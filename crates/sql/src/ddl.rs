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
        if !f.is_stored_column() {
            continue;
        }
        cols.push(column_def(&ct.name, f)?);
    }
    if ct.draft_publish() {
        cols.push(r#""published_at" TIMESTAMPTZ"#.into());
    }
    let body = cols.join(", ");
    Ok(format!("CREATE TABLE {table} ({body})"))
}

/// `ALTER TABLE ct_<name> ADD COLUMN ...`
pub fn add_column(ct_name: &str, field: &Field) -> Result<String, DdlError> {
    // Many-to-many fields have no row column; the caller manages the join
    // table separately (see SchemaService). Reaching here with one is a bug.
    if field.relation_meta().is_some() && !field.is_stored_column() {
        return Err(DdlError::Ident(IdentError(format!(
            "add_column called for non-stored field `{}`",
            field.name
        ))));
    }
    if field.kind == FieldKind::Media && !field.is_stored_column() {
        return Err(DdlError::Ident(IdentError(format!(
            "add_column called for non-stored media field `{}`",
            field.name
        ))));
    }
    let table = table_name(ct_name)?;
    let def = column_def(ct_name, field)?;
    Ok(format!("ALTER TABLE {table} ADD COLUMN {def}"))
}

/// `ALTER TABLE ct_<name> ADD COLUMN "published_at" TIMESTAMPTZ` — used when
/// Draft & Publish is enabled on an existing type. Nullable: existing rows
/// become drafts (NULL).
pub fn add_published_at_column(ct_name: &str) -> Result<String, DdlError> {
    let table = table_name(ct_name)?;
    Ok(format!(
        "ALTER TABLE {table} ADD COLUMN \"published_at\" TIMESTAMPTZ"
    ))
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

/// Emits paired DROP + ADD CONSTRAINT statements to update an enum
/// field's CHECK constraint to a new (extended) values list. The
/// physical table is `ct_<ct_name>`; the constraint name is
/// `<ct_name>_<col>_enum_chk`.
pub fn alter_enum_values(
    ct_name: &str,
    col: &str,
    all_values: &[String],
) -> Result<String, DdlError> {
    let table = table_name(ct_name)?;
    let col_q = quote_ident(col)?;
    let constraint_q = quote_ident(&format!("{ct_name}_{col}_enum_chk"))?;
    let values_lit = all_values
        .iter()
        .map(|v| format!("'{}'", v.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "ALTER TABLE {table} DROP CONSTRAINT {constraint_q}; \
         ALTER TABLE {table} ADD CONSTRAINT {constraint_q} \
         CHECK ({col_q} IS NULL OR {col_q} IN ({values_lit}))"
    ))
}

/// Build the `CREATE TABLE` + `CREATE INDEX` statements for a many-to-many
/// join table on `owner.<field>` targeting `target`. Returns
/// `(create_table_sql, create_index_sql)`. Column names are `<owner>_id` and
/// `<target>_id`; both FKs cascade on delete so removing a linked entry drops
/// its links.
pub fn create_join_table(
    owner: &str,
    field: &str,
    target: &str,
) -> Result<(String, String), DdlError> {
    let jt = crate::ident::join_table_name(owner, field)?;
    let owner_tbl = table_name(owner)?;
    let target_tbl = table_name(target)?;
    let owner_col = quote_ident(&format!("{owner}_id"))?;
    let target_col = quote_ident(&format!("{target}_id"))?;
    let create = format!(
        "CREATE TABLE {jt} (\
{owner_col} uuid NOT NULL REFERENCES {owner_tbl}(\"id\") ON DELETE CASCADE, \
{target_col} uuid NOT NULL REFERENCES {target_tbl}(\"id\") ON DELETE CASCADE, \
PRIMARY KEY ({owner_col}, {target_col}))"
    );
    let index = format!("CREATE INDEX ON {jt} ({target_col})");
    Ok((create, index))
}

/// `DROP TABLE <join table for owner.field>`.
pub fn drop_join_table(owner: &str, field: &str) -> Result<String, DdlError> {
    let jt = crate::ident::join_table_name(owner, field)?;
    Ok(format!("DROP TABLE {jt}"))
}

/// Build the `CREATE TABLE` + `CREATE INDEX` statements for an ordered
/// multiple-media join table `j_media_<ct>_<field>`. Returns
/// `(create_table_sql, create_index_sql)`. The owner FK is `<ct>_id` (cascades
/// when the entry is deleted); `asset_id` references `_media_assets` and
/// cascades when the asset is deleted. `position` orders the gallery.
pub fn create_media_join_table(ct: &str, field: &str) -> Result<(String, String), DdlError> {
    let jt = crate::ident::media_join_table_name(ct, field)?;
    let owner_tbl = table_name(ct)?;
    let owner_col = quote_ident(&format!("{ct}_id"))?;
    let create = format!(
        "CREATE TABLE {jt} (\
{owner_col} uuid NOT NULL REFERENCES {owner_tbl}(\"id\") ON DELETE CASCADE, \
\"asset_id\" uuid NOT NULL REFERENCES \"_media_assets\"(\"id\") ON DELETE CASCADE, \
\"position\" int NOT NULL, \
PRIMARY KEY ({owner_col}, \"asset_id\"))"
    );
    let index = format!("CREATE INDEX ON {jt} ({owner_col}, \"position\")");
    Ok((create, index))
}

/// `DROP TABLE <media join table for ct.field>`.
pub fn drop_media_join_table(ct: &str, field: &str) -> Result<String, DdlError> {
    let jt = crate::ident::media_join_table_name(ct, field)?;
    Ok(format!("DROP TABLE {jt}"))
}

fn column_def(ct_name: &str, f: &Field) -> Result<String, DdlError> {
    if f.kind == FieldKind::Relation {
        return relation_column_def(f);
    }
    if f.kind == FieldKind::Media {
        let col = quote_ident(&f.physical_column())?;
        return Ok(format!(
            "{col} uuid REFERENCES \"_media_assets\"(\"id\") ON DELETE SET NULL"
        ));
    }
    if f.kind == FieldKind::Enum {
        let meta = f.enum_meta().ok_or_else(|| {
            IdentError("enum field missing/invalid kind_meta".into())
        })?;
        let col = quote_ident(&f.name)?;
        let default_clause = if !f.default.is_null() {
            format!(" DEFAULT {}", render_default(f))
        } else {
            String::new()
        };
        let not_null = if f.required { " NOT NULL" } else { "" };
        let unique = if f.unique { " UNIQUE" } else { "" };
        let values_lit = meta
            .values
            .iter()
            .map(|v| format!("'{}'", v.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(", ");
        let constraint_name = quote_ident(&format!("{ct_name}_{}_enum_chk", f.name))?;
        return Ok(format!(
            "{col} text{default_clause}{not_null}{unique} CONSTRAINT {constraint_name} CHECK ({col} IS NULL OR {col} IN ({values_lit}))"
        ));
    }
    if f.kind == FieldKind::Json || f.kind == FieldKind::RichText || f.kind == FieldKind::Component {
        let col = quote_ident(&f.name)?;
        let default_clause = if !f.default.is_null() {
            format!(" DEFAULT {}", render_default(f))
        } else {
            String::new()
        };
        let not_null = if f.required { " NOT NULL" } else { "" };
        return Ok(format!("{col} jsonb{default_clause}{not_null}"));
    }
    if matches!(
        f.kind,
        FieldKind::Email | FieldKind::Url | FieldKind::Slug
    ) {
        let col = quote_ident(&f.name)?;
        let default_clause = if !f.default.is_null() {
            format!(" DEFAULT {}", render_default(f))
        } else {
            String::new()
        };
        let not_null = if f.required { " NOT NULL" } else { "" };
        let unique = if f.unique { " UNIQUE" } else { "" };
        return Ok(format!("{col} text{default_clause}{not_null}{unique}"));
    }
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

fn relation_column_def(f: &Field) -> Result<String, DdlError> {
    use rustapi_core::Cardinality;
    let meta = f.relation_meta().ok_or_else(|| {
        IdentError("relation field missing/invalid kind_meta".into())
    })?;
    let col = quote_ident(&f.physical_column())?;
    let target = table_name(&meta.target)?;
    let not_null = if f.required { " NOT NULL" } else { "" };
    let unique = if meta.cardinality == Cardinality::OneToOne { " UNIQUE" } else { "" };
    Ok(format!(
        "{col} uuid{not_null}{unique} REFERENCES {target}(\"id\") ON DELETE RESTRICT"
    ))
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
        (
            FieldKind::String
            | FieldKind::Text
            | FieldKind::Email
            | FieldKind::Url
            | FieldKind::Slug
            | FieldKind::Enum,
            serde_json::Value::String(s),
        ) => {
            let escaped = s.replace('\'', "''");
            format!("'{escaped}'")
        }
        (FieldKind::Datetime, serde_json::Value::String(s)) => {
            let escaped = s.replace('\'', "''");
            format!("'{escaped}'::timestamptz")
        }
        (FieldKind::Integer, v) | (FieldKind::Float, v) => v.to_string(),
        (FieldKind::Boolean, serde_json::Value::Bool(b)) => if *b { "TRUE" } else { "FALSE" }.into(),
        (FieldKind::Json, v) if !v.is_null() => {
            let s = v.to_string().replace('\'', "''");
            format!("'{s}'::jsonb")
        }
        (FieldKind::RichText, v) if !v.is_null() => {
            let s = v.to_string().replace('\'', "''");
            format!("'{s}'::jsonb")
        }
        (FieldKind::Component, v) if !v.is_null() => {
            let s = v.to_string().replace('\'', "''");
            format!("'{s}'::jsonb")
        }
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
            options: json!({}),
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

    #[test]
    fn create_table_emits_relation_fk_nullable() {
        let mut f = field("author", FieldKind::Relation);
        f.kind_meta = json!({"target":"user","cardinality":"many_to_one"});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(
            sql.contains("\"author_id\" uuid REFERENCES \"ct_user\"(\"id\") ON DELETE RESTRICT"),
            "got: {sql}"
        );
        // Nullable: no NOT NULL after the uuid keyword for this column.
        assert!(!sql.contains("\"author_id\" uuid NOT NULL"), "got: {sql}");
    }

    #[test]
    fn create_table_emits_relation_fk_not_null_when_required() {
        let mut f = field("author", FieldKind::Relation);
        f.required = true;
        f.kind_meta = json!({"target":"user","cardinality":"many_to_one"});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(
            sql.contains("\"author_id\" uuid NOT NULL REFERENCES \"ct_user\"(\"id\") ON DELETE RESTRICT"),
            "got: {sql}"
        );
    }

    #[test]
    fn add_column_emits_nullable_fk_for_relation() {
        let mut f = field("author", FieldKind::Relation);
        f.kind_meta = json!({"target":"user","cardinality":"many_to_one"});
        let sql = add_column("post", &f).unwrap();
        assert_eq!(
            sql,
            "ALTER TABLE \"ct_post\" ADD COLUMN \"author_id\" uuid REFERENCES \"ct_user\"(\"id\") ON DELETE RESTRICT"
        );
    }

    #[test]
    fn create_table_emits_enum_check() {
        let f = Field {
            name: "status".into(),
            kind: FieldKind::Enum,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({"values": ["draft", "published"]}),
        };
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(sql.contains("\"status\" text"));
        assert!(sql.contains("CONSTRAINT \"post_status_enum_chk\""));
        assert!(sql.contains("CHECK (\"status\" IS NULL OR \"status\" IN ('draft', 'published'))"));
    }

    #[test]
    fn create_table_emits_enum_check_with_default() {
        let f = Field {
            name: "status".into(),
            kind: FieldKind::Enum,
            required: false,
            unique: false,
            default: json!("draft"),
            max_length: None,
            kind_meta: json!({"values": ["draft", "published"]}),
        };
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(sql.contains("DEFAULT 'draft'"));
    }

    #[test]
    fn create_table_emits_json_jsonb() {
        let f = Field {
            name: "meta".into(),
            kind: FieldKind::Json,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        };
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(sql.contains("\"meta\" jsonb"));
    }

    #[test]
    fn create_table_emits_rich_text_jsonb() {
        let f = Field {
            name: "body".into(),
            kind: FieldKind::RichText,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        };
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(sql.contains("\"body\" jsonb"), "got: {sql}");
    }

    #[test]
    fn create_table_emits_text_for_email_url_slug() {
        for (kind, name) in [
            (FieldKind::Email, "e"),
            (FieldKind::Url, "u"),
            (FieldKind::Slug, "s"),
        ] {
            let f = Field {
                name: name.into(),
                kind,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({}),
            };
            let sql = create_table(&ct(vec![f])).unwrap();
            assert!(sql.contains(&format!("\"{name}\" text")), "{kind:?}");
        }
    }

    #[test]
    fn alter_enum_values_emits_drop_and_add() {
        let sql = alter_enum_values("post", "status", &[
            "draft".to_string(),
            "published".to_string(),
            "archived".to_string(),
        ])
        .unwrap();
        assert!(sql.contains("DROP CONSTRAINT \"post_status_enum_chk\""));
        assert!(sql.contains("ADD CONSTRAINT \"post_status_enum_chk\""));
        assert!(sql.contains("'draft', 'published', 'archived'"));
    }

    #[test]
    fn add_column_emits_enum_constraint() {
        let f = Field {
            name: "status".into(),
            kind: FieldKind::Enum,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({"values": ["a", "b"]}),
        };
        let sql = add_column("post", &f).unwrap();
        assert!(sql.contains("ALTER TABLE"));
        assert!(sql.contains("ADD COLUMN \"status\" text"));
        assert!(sql.contains("CONSTRAINT \"post_status_enum_chk\""));
    }

    #[test]
    fn create_table_one_to_one_emits_unique_fk() {
        let mut f = field("profile", FieldKind::Relation);
        f.kind_meta = json!({"target":"profile","cardinality":"one_to_one"});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(
            sql.contains("\"profile_id\" uuid UNIQUE REFERENCES \"ct_profile\"(\"id\") ON DELETE RESTRICT"),
            "got: {sql}"
        );
    }

    #[test]
    fn many_to_one_still_has_no_unique() {
        let mut f = field("author", FieldKind::Relation);
        f.kind_meta = json!({"target":"user","cardinality":"many_to_one"});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(!sql.contains("\"author_id\" uuid UNIQUE"), "got: {sql}");
    }

    #[test]
    fn create_table_required_one_to_one_emits_not_null_unique() {
        let mut f = field("profile", FieldKind::Relation);
        f.required = true;
        f.kind_meta = json!({"target":"profile","cardinality":"one_to_one"});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(
            sql.contains("\"profile_id\" uuid NOT NULL UNIQUE REFERENCES \"ct_profile\"(\"id\") ON DELETE RESTRICT"),
            "got: {sql}"
        );
    }

    #[test]
    fn relation_fk_rejects_invalid_target() {
        // RelationMeta::from_value enforces is_valid_ident for target, so
        // a well-formed Field cannot reach column_def with an invalid target.
        // But defend the emitter anyway: if someone passes a Field whose
        // kind_meta says target="Bad" (uppercase rejected by is_valid_ident),
        // table_name() should error.
        let f = Field {
            name: "author".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({"target":"Bad","cardinality":"many_to_one"}),
        };
        assert!(add_column("post", &f).is_err());
    }

    #[test]
    fn create_join_table_emits_table_and_index() {
        let (create, index) =
            create_join_table("post", "tags", "tag").unwrap();
        assert_eq!(
            create,
            "CREATE TABLE \"j_post_tags\" (\
\"post_id\" uuid NOT NULL REFERENCES \"ct_post\"(\"id\") ON DELETE CASCADE, \
\"tag_id\" uuid NOT NULL REFERENCES \"ct_tag\"(\"id\") ON DELETE CASCADE, \
PRIMARY KEY (\"post_id\", \"tag_id\"))"
        );
        assert_eq!(
            index,
            "CREATE INDEX ON \"j_post_tags\" (\"tag_id\")"
        );
    }

    #[test]
    fn drop_join_table_works() {
        assert_eq!(drop_join_table("post", "tags").unwrap(), "DROP TABLE \"j_post_tags\"");
    }

    #[test]
    fn create_table_skips_many_to_many_columns() {
        let mut f = field("tags", FieldKind::Relation);
        f.kind_meta = json!({"target":"tag","cardinality":"many_to_many"});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(!sql.contains("tags"), "got: {sql}");
    }

    #[test]
    fn add_column_rejects_many_to_many() {
        let mut f = field("tags", FieldKind::Relation);
        f.kind_meta = json!({"target":"tag","cardinality":"many_to_many"});
        assert!(add_column("post", &f).is_err());
    }

    #[test]
    fn add_column_rejects_multiple_media() {
        let mut f = field("gallery", FieldKind::Media);
        f.kind_meta = json!({"multiple": true});
        assert!(add_column("post", &f).is_err());
    }

    #[test]
    fn create_table_emits_media_single_fk_set_null() {
        let mut f = field("hero", FieldKind::Media);
        f.kind_meta = json!({"multiple": false});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(
            sql.contains("\"hero_id\" uuid REFERENCES \"_media_assets\"(\"id\") ON DELETE SET NULL"),
            "got: {sql}"
        );
        assert!(!sql.contains("\"hero_id\" uuid NOT NULL"), "got: {sql}");
    }

    #[test]
    fn create_table_skips_multiple_media_column() {
        let mut f = field("gallery", FieldKind::Media);
        f.kind_meta = json!({"multiple": true});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(!sql.contains("gallery"), "got: {sql}");
    }

    #[test]
    fn add_column_emits_media_single_fk() {
        let mut f = field("hero", FieldKind::Media);
        f.kind_meta = json!({"multiple": false});
        let sql = add_column("post", &f).unwrap();
        assert_eq!(
            sql,
            "ALTER TABLE \"ct_post\" ADD COLUMN \"hero_id\" uuid REFERENCES \"_media_assets\"(\"id\") ON DELETE SET NULL"
        );
    }

    #[test]
    fn create_media_join_table_emits_ordered_table_and_index() {
        let (create, index) = create_media_join_table("post", "gallery").unwrap();
        assert_eq!(
            create,
            "CREATE TABLE \"j_media_post_gallery\" (\
\"post_id\" uuid NOT NULL REFERENCES \"ct_post\"(\"id\") ON DELETE CASCADE, \
\"asset_id\" uuid NOT NULL REFERENCES \"_media_assets\"(\"id\") ON DELETE CASCADE, \
\"position\" int NOT NULL, \
PRIMARY KEY (\"post_id\", \"asset_id\"))"
        );
        assert_eq!(
            index,
            "CREATE INDEX ON \"j_media_post_gallery\" (\"post_id\", \"position\")"
        );
    }

    #[test]
    fn drop_media_join_table_works() {
        assert_eq!(
            drop_media_join_table("post", "gallery").unwrap(),
            "DROP TABLE \"j_media_post_gallery\""
        );
    }

    #[test]
    fn create_table_emits_published_at_when_draft_publish() {
        let mut c = ct(vec![field("title", FieldKind::String)]);
        c.options = json!({ "draft_publish": true });
        let sql = create_table(&c).unwrap();
        assert!(sql.contains("\"published_at\" TIMESTAMPTZ"), "got: {sql}");
    }

    #[test]
    fn create_table_omits_published_at_when_disabled() {
        let sql = create_table(&ct(vec![field("title", FieldKind::String)])).unwrap();
        assert!(!sql.contains("published_at"), "got: {sql}");
    }

    #[test]
    fn add_published_at_column_builds_alter() {
        let sql = add_published_at_column("post").unwrap();
        assert_eq!(
            sql,
            "ALTER TABLE \"ct_post\" ADD COLUMN \"published_at\" TIMESTAMPTZ"
        );
    }
}
