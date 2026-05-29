//! Validate request bodies for entry CRUD and decode rows back into JSON
//! objects using schema metadata.

use rustapi_core::{
    is_system_column, BoundValue, ContentType, Error, Field, FieldKind, ValidationErrors,
};
use serde_json::{Map, Value};
use sqlx::{postgres::PgRow, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn body_to_binds(
    ct: &ContentType,
    mut body: Map<String, Value>,
    require_required: bool,
) -> Result<BTreeMap<String, BoundValue>, Error> {
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
    for f in &ct.fields {
        match body.get(&f.name) {
            Some(v) => {
                if v.is_null() && f.required {
                    return Err(Error::Validation(ValidationErrors::field(&f.name, "required")));
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
                let bv = BoundValue::from_json(f.kind, v)
                    .map_err(|e| Error::Validation(ValidationErrors::field(&f.name, e.to_string())))?;
                out.insert(f.name.clone(), bv);
            }
            None => {
                if require_required && f.required && f.default.is_null() {
                    return Err(Error::Validation(ValidationErrors::field(&f.name, "required")));
                }
            }
        }
    }
    Ok(out)
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
        let out = body_to_binds(&ct(), body, true).unwrap();
        assert_eq!(out.get("title").unwrap(), &BoundValue::Str("hi".into()));
        assert_eq!(out.get("count").unwrap(), &BoundValue::I64(7));
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
}
