//! Phase 2.4 populate pass. Runs AFTER the main SELECT returns rows.
//!
//! Forward populate (many_to_one): one batched SELECT per target type
//! replaces the FK uuid string under the relation field's JSON key with
//! the full target object.
//!
//! Inverse populate (virtual reverse of a registered relation): lands in
//! Task 12 with a hard per-parent cap.

use rustapi_core::{ContentType, Error, FieldKind, ValidationErrors};
use rustapi_schema::SchemaRegistry;
use serde_json::{Map, Value};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const INVERSE_LIMIT_PER_PARENT: usize = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopulateField {
    Forward {
        field_name: String,
        target: String,
    },
    Inverse {
        field_name: String,
        source: String,
        fk_col: String,
    },
}

/// Parse `?populate=foo,bar` against a content type and registry. Returns
/// the resolved fields in payload order; duplicate names are silently
/// dropped after their first occurrence. Empty input or whitespace-only
/// entries surface as a 400-level validation error via the standard
/// `ValidationErrors` taxonomy (Task 7 deliberately skipped dedicated
/// EmptyPopulate / UnknownPopulateField variants).
pub async fn parse_populate(
    ct: &ContentType,
    registry: &SchemaRegistry,
    raw: &str,
) -> Result<Vec<PopulateField>, Error> {
    if raw.is_empty() {
        return Err(Error::Validation(ValidationErrors::single(
            "populate must not be empty",
        )));
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for piece in raw.split(',') {
        let name = piece.trim();
        if name.is_empty() {
            return Err(Error::Validation(ValidationErrors::single(
                "populate must not contain empty entries",
            )));
        }
        if !seen.insert(name.to_string()) {
            continue;
        }
        if let Some(f) = ct.fields.iter().find(|f| f.name == name) {
            if f.kind == FieldKind::Relation {
                let meta = f.relation_meta().ok_or_else(|| {
                    Error::Validation(ValidationErrors::single(format!(
                        "unknown populate field `{name}`"
                    )))
                })?;
                out.push(PopulateField::Forward {
                    field_name: name.to_string(),
                    target: meta.target,
                });
                continue;
            }
        }
        if let Some((source, fk_col)) = registry.inverse_lookup(&ct.name, name).await {
            out.push(PopulateField::Inverse {
                field_name: name.to_string(),
                source,
                fk_col,
            });
            continue;
        }
        return Err(Error::Validation(ValidationErrors::single(format!(
            "unknown populate field `{name}`"
        ))));
    }
    Ok(out)
}

/// Hydrate one forward relation in-place on `rows`. The relation field on
/// each row carries a uuid string from the main SELECT; replace with the
/// full target object, or leave unchanged when the target id doesn't
/// resolve (deleted concurrently; pre-check is a different code path).
pub async fn apply_forward(
    pool: &PgPool,
    registry: &SchemaRegistry,
    rows: &mut [Map<String, Value>],
    field_name: &str,
    target: &str,
) -> Result<(), Error> {
    let target_ct = registry.get(target).await.ok_or_else(|| {
        Error::Internal(anyhow::anyhow!("populate target vanished: {target}"))
    })?;
    let mut ids: Vec<Uuid> = Vec::new();
    let mut seen: HashSet<Uuid> = HashSet::new();
    for r in rows.iter() {
        if let Some(Value::String(s)) = r.get(field_name) {
            if let Ok(u) = Uuid::parse_str(s) {
                if seen.insert(u) {
                    ids.push(u);
                }
            }
        }
    }
    if ids.is_empty() {
        return Ok(());
    }
    let table = rustapi_sql::table_name(target)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let sql = format!("SELECT * FROM {table} WHERE id = ANY($1)");
    let fetched = sqlx::query(&sql)
        .bind(&ids)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    let mut by_id: HashMap<Uuid, Value> = HashMap::with_capacity(fetched.len());
    for row in &fetched {
        let obj = crate::entry::row_to_json(&target_ct, row)?;
        let id_str = obj.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
        if let Some(s) = id_str {
            if let Ok(u) = Uuid::parse_str(&s) {
                by_id.insert(u, obj);
            }
        }
    }
    for r in rows.iter_mut() {
        let take = match r.get(field_name) {
            Some(Value::String(s)) => Uuid::parse_str(s).ok(),
            _ => None,
        };
        if let Some(u) = take {
            if let Some(obj) = by_id.get(&u).cloned() {
                r.insert(field_name.to_string(), obj);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::Field;
    use serde_json::json;

    fn ct_with_relation() -> ContentType {
        ContentType {
            id: Uuid::new_v4(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![
                Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: false,
                    unique: false,
                    default: serde_json::Value::Null,
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "author".into(),
                    kind: FieldKind::Relation,
                    required: false,
                    unique: false,
                    default: serde_json::Value::Null,
                    max_length: None,
                    kind_meta: json!({
                        "target": "user",
                        "cardinality": "many_to_one",
                        "inverse": "posts"
                    }),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn user_ct() -> ContentType {
        ContentType {
            id: Uuid::new_v4(),
            name: "user".into(),
            display_name: "User".into(),
            fields: vec![Field {
                name: "name".into(),
                kind: FieldKind::String,
                required: false,
                unique: false,
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: json!({}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn parse_forward_field() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let out = parse_populate(&ct, &reg, "author").await.unwrap();
        assert_eq!(out.len(), 1);
        match &out[0] {
            PopulateField::Forward { field_name, target } => {
                assert_eq!(field_name, "author");
                assert_eq!(target, "user");
            }
            other => panic!("expected Forward, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_inverse_via_registry() {
        let reg = SchemaRegistry::new();
        reg.insert(user_ct()).await;
        reg.insert(ct_with_relation()).await;
        let user = reg.get("user").await.unwrap();
        let out = parse_populate(&user, &reg, "posts").await.unwrap();
        assert_eq!(out.len(), 1);
        match &out[0] {
            PopulateField::Inverse { field_name, source, fk_col } => {
                assert_eq!(field_name, "posts");
                assert_eq!(source, "post");
                assert_eq!(fk_col, "author_id");
            }
            other => panic!("expected Inverse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reject_unknown_field() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let err = parse_populate(&ct, &reg, "nope").await.unwrap_err();
        assert!(format!("{err:?}").contains("unknown populate field"));
    }

    #[tokio::test]
    async fn reject_empty() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let err = parse_populate(&ct, &reg, "").await.unwrap_err();
        assert!(format!("{err:?}").contains("must not be empty"));
    }

    #[tokio::test]
    async fn reject_blank_entry() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let err = parse_populate(&ct, &reg, "author, ,title").await.unwrap_err();
        assert!(format!("{err:?}").contains("empty entries"));
    }

    #[tokio::test]
    async fn dedupe_duplicates() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let out = parse_populate(&ct, &reg, "author,author").await.unwrap();
        assert_eq!(out.len(), 1);
    }

    #[tokio::test]
    async fn reject_primitive_field() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let err = parse_populate(&ct, &reg, "title").await.unwrap_err();
        assert!(format!("{err:?}").contains("unknown populate field"));
    }

    #[tokio::test]
    async fn forward_and_inverse_preserve_order() {
        let reg = SchemaRegistry::new();
        reg.insert(user_ct()).await;
        reg.insert(ct_with_relation()).await;
        let post = reg.get("post").await.unwrap();
        // post has forward `author` but no inverse on itself; verify the
        // parse loop hits the forward arm + returns just one entry.
        let out = parse_populate(&post, &reg, "author").await.unwrap();
        assert_eq!(out.len(), 1);
    }
}
