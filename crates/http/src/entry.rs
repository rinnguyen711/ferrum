//! Validate request bodies for entry CRUD and decode rows back into JSON
//! objects using schema metadata.

use rustapi_core::{
    is_system_column, BoundValue, ContentType, Error, Field, FieldKind, ValidationErrors,
};
use serde_json::{Map, Value};
use sqlx::{postgres::PgRow, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

/// One pending existence check emitted by `body_to_binds` per non-null
/// relation field value. Handler groups these by `target` and runs a single
/// `id = ANY($1)` per target. `field` is the relation field name (not the
/// physical column) so a missing-id error surfaces under the JSON key the
/// client used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationCheck {
    pub field: String,
    pub target: String,
    pub id: Uuid,
}

pub fn body_to_binds(
    ct: &ContentType,
    mut body: Map<String, Value>,
    require_required: bool,
) -> Result<(BTreeMap<String, BoundValue>, Vec<RelationCheck>), Error> {
    for sys in &["id", "created_at", "updated_at"] {
        body.remove(*sys);
    }

    let allowed: std::collections::HashSet<&str> =
        ct.fields.iter().map(|f| f.name.as_str()).collect();
    for k in body.keys() {
        if !allowed.contains(k.as_str()) {
            return Err(Error::Validation(ValidationErrors::single(format!(
                "unknown field `{k}`"
            ))));
        }
    }

    let mut out = BTreeMap::new();
    let mut checks: Vec<RelationCheck> = Vec::new();
    for f in &ct.fields {
        match body.get(&f.name) {
            Some(v) => {
                if v.is_null() && f.required {
                    return Err(Error::Validation(ValidationErrors::field(&f.name, "required")));
                }
                if f.kind == FieldKind::Relation {
                    coerce_relation(f, v, &mut out, &mut checks)?;
                    continue;
                }
                if f.kind == FieldKind::String {
                    if let Value::String(s) = v {
                        if (s.chars().count() as u32) > f.effective_max_length() {
                            return Err(Error::Validation(ValidationErrors::field(
                                &f.name,
                                "exceeds max_length",
                            )));
                        }
                    }
                }
                let bv = BoundValue::from_json(f.kind, v).map_err(|e| match e {
                    rustapi_core::CoerceError::BadEmail => Error::BadEmail,
                    rustapi_core::CoerceError::BadUrl => Error::BadUrl,
                    rustapi_core::CoerceError::BadSlug => Error::BadSlug,
                    other => Error::Validation(ValidationErrors::field(&f.name, other.to_string())),
                })?;
                if f.kind == FieldKind::Enum {
                    if let BoundValue::Str(s) = &bv {
                        let meta = f.enum_meta().ok_or_else(|| {
                            Error::Validation(ValidationErrors::field(&f.name, "missing enum kind_meta"))
                        })?;
                        if !meta.values.iter().any(|v| v == s) {
                            return Err(Error::EnumValueNotAllowed {
                                field: f.name.clone(),
                                value: s.clone(),
                                allowed: meta.values.clone(),
                            });
                        }
                    }
                }
                out.insert(f.name.clone(), bv);
            }
            None => {
                if require_required && f.required && f.default.is_null() {
                    return Err(Error::Validation(ValidationErrors::field(&f.name, "required")));
                }
            }
        }
    }
    Ok((out, checks))
}

/// Coerce a JSON value (uuid string | null) for a `FieldKind::Relation` field.
/// Pushes the bind under `f.name` (DML resolves to physical column on its own)
/// and registers a pending existence check for non-null values.
fn coerce_relation(
    f: &Field,
    v: &Value,
    out: &mut BTreeMap<String, BoundValue>,
    checks: &mut Vec<RelationCheck>,
) -> Result<(), Error> {
    let meta = f.relation_meta().ok_or_else(|| {
        Error::Validation(ValidationErrors::field(&f.name, "missing relation kind_meta"))
    })?;
    match v {
        Value::Null => {
            out.insert(f.name.clone(), BoundValue::Null(FieldKind::Uuid));
        }
        Value::String(s) => {
            let id = Uuid::parse_str(s).map_err(|_| {
                Error::Validation(ValidationErrors::field(&f.name, "invalid uuid"))
            })?;
            out.insert(f.name.clone(), BoundValue::Uuid(id));
            checks.push(RelationCheck {
                field: f.name.clone(),
                target: meta.target.clone(),
                id,
            });
        }
        _ => {
            return Err(Error::Validation(ValidationErrors::field(
                &f.name,
                "relation value must be a uuid string or null",
            )));
        }
    }
    Ok(())
}

pub fn row_to_json(ct: &ContentType, row: &PgRow) -> Result<Value, Error> {
    let mut obj = Map::new();

    let id: Uuid = row.try_get("id").map_err(decode)?;
    obj.insert("id".into(), Value::String(id.to_string()));

    let ca: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(decode)?;
    obj.insert("created_at".into(), Value::String(ca.to_rfc3339()));

    let ua: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(decode)?;
    obj.insert("updated_at".into(), Value::String(ua.to_rfc3339()));

    for f in &ct.fields {
        if is_system_column(&f.name) {
            continue;
        }
        let v = decode_field(row, f)?;
        obj.insert(f.name.clone(), v);
    }

    Ok(Value::Object(obj))
}

fn decode_field(row: &PgRow, f: &Field) -> Result<Value, Error> {
    match f.kind {
        FieldKind::String | FieldKind::Text => {
            let v: Option<String> = row.try_get(f.name.as_str()).map_err(decode)?;
            Ok(v.map(Value::String).unwrap_or(Value::Null))
        }
        FieldKind::Integer => {
            let v: Option<i64> = row.try_get(f.name.as_str()).map_err(decode)?;
            Ok(v.map(|n| Value::Number(n.into())).unwrap_or(Value::Null))
        }
        FieldKind::Float => {
            let v: Option<f64> = row.try_get(f.name.as_str()).map_err(decode)?;
            Ok(v.and_then(|n| serde_json::Number::from_f64(n).map(Value::Number))
                .unwrap_or(Value::Null))
        }
        FieldKind::Boolean => {
            let v: Option<bool> = row.try_get(f.name.as_str()).map_err(decode)?;
            Ok(v.map(Value::Bool).unwrap_or(Value::Null))
        }
        FieldKind::Datetime => {
            let v: Option<chrono::DateTime<chrono::Utc>> =
                row.try_get(f.name.as_str()).map_err(decode)?;
            Ok(v.map(|t| Value::String(t.to_rfc3339())).unwrap_or(Value::Null))
        }
        FieldKind::Relation => {
            // Phase 2.4: relation FKs live in `<name>_id` physical column;
            // surface the raw uuid string under the relation field's JSON key.
            // Populate (Task 11+) replaces this scalar with an object when
            // the field is in `?populate=`.
            let col = f.physical_column();
            let v: Option<Uuid> = row.try_get(col.as_str()).map_err(decode)?;
            Ok(v.map(|u| Value::String(u.to_string())).unwrap_or(Value::Null))
        }
        _ => Ok(Value::Null),
    }
}

fn decode(e: sqlx::Error) -> Error {
    Error::Internal(anyhow::anyhow!(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn ct() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![
                Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: true,
                    unique: false,
                    default: json!(null),
                    max_length: Some(10),
                    kind_meta: json!({}),
                },
                Field {
                    name: "count".into(),
                    kind: FieldKind::Integer,
                    required: false,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn body_strips_system_cols_and_coerces() {
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({
            "id": "ignored",
            "title": "hi",
            "count": 7,
            "created_at": "ignored"
        }))
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
        let (out, checks) = body_to_binds(&ct(), body, true).unwrap();
        assert_eq!(out.get("title").unwrap(), &BoundValue::Str("hi".into()));
        assert_eq!(out.get("count").unwrap(), &BoundValue::I64(7));
        assert!(checks.is_empty());
    }

    #[test]
    fn body_rejects_unknown_key() {
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({"title": "x", "extra": 1}))
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        assert!(matches!(body_to_binds(&ct(), body, true), Err(Error::Validation(_))));
    }

    #[test]
    fn body_required_missing_rejected_on_post() {
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({"count": 1}))
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        assert!(matches!(body_to_binds(&ct(), body, true), Err(Error::Validation(_))));
    }

    #[test]
    fn body_required_missing_allowed_when_not_required() {
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({"count": 1}))
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        assert!(body_to_binds(&ct(), body, false).is_ok());
    }

    #[test]
    fn body_string_max_length_enforced() {
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({"title": "way too long, definitely"}))
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        assert!(matches!(body_to_binds(&ct(), body, true), Err(Error::Validation(_))));
    }

    fn ct_with_relation() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "author".into(),
                kind: FieldKind::Relation,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn relation_uuid_string_coerces_and_registers_check() {
        let id = Uuid::new_v4();
        let body: Map<String, Value> =
            serde_json::from_value::<Value>(json!({"author": id.to_string()}))
                .unwrap()
                .as_object()
                .unwrap()
                .clone();
        let (out, checks) = body_to_binds(&ct_with_relation(), body, true).unwrap();
        assert_eq!(out.get("author").unwrap(), &BoundValue::Uuid(id));
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].field, "author");
        assert_eq!(checks[0].target, "user");
        assert_eq!(checks[0].id, id);
    }

    #[test]
    fn relation_null_writes_typed_null_no_check() {
        let body: Map<String, Value> =
            serde_json::from_value::<Value>(json!({"author": serde_json::Value::Null}))
                .unwrap()
                .as_object()
                .unwrap()
                .clone();
        let (out, checks) = body_to_binds(&ct_with_relation(), body, true).unwrap();
        assert_eq!(
            out.get("author").unwrap(),
            &BoundValue::Null(FieldKind::Uuid)
        );
        assert!(checks.is_empty());
    }

    #[test]
    fn relation_bad_uuid_rejected() {
        let body: Map<String, Value> =
            serde_json::from_value::<Value>(json!({"author": "not-a-uuid"}))
                .unwrap()
                .as_object()
                .unwrap()
                .clone();
        assert!(matches!(
            body_to_binds(&ct_with_relation(), body, true),
            Err(Error::Validation(_))
        ));
    }

    #[test]
    fn relation_non_string_non_null_rejected() {
        let body: Map<String, Value> =
            serde_json::from_value::<Value>(json!({"author": 123}))
                .unwrap()
                .as_object()
                .unwrap()
                .clone();
        assert!(matches!(
            body_to_binds(&ct_with_relation(), body, true),
            Err(Error::Validation(_))
        ));
    }

    #[test]
    fn relation_required_null_rejected() {
        let mut c = ct_with_relation();
        c.fields[0].required = true;
        let body: Map<String, Value> =
            serde_json::from_value::<Value>(json!({"author": serde_json::Value::Null}))
                .unwrap()
                .as_object()
                .unwrap()
                .clone();
        assert!(matches!(body_to_binds(&c, body, true), Err(Error::Validation(_))));
    }
}
