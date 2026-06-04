//! Maps the content-type field model to OpenAPI/JSON Schema fragments.

use rustapi_core::field::{Field, FieldKind};
use serde_json::{json, Value};

/// Build a JSON Schema fragment for a single field's value type.
pub fn field_to_schema(field: &Field) -> Value {
    let mut schema = match field.kind {
        FieldKind::String | FieldKind::Text => {
            json!({ "type": "string", "maxLength": field.effective_max_length() })
        }
        FieldKind::Integer => json!({ "type": "integer", "format": "int64" }),
        FieldKind::Float => json!({ "type": "number", "format": "double" }),
        FieldKind::Boolean => json!({ "type": "boolean" }),
        FieldKind::Datetime => json!({ "type": "string", "format": "date-time" }),
        FieldKind::Uuid => json!({ "type": "string", "format": "uuid" }),
        FieldKind::Email => json!({ "type": "string", "format": "email" }),
        FieldKind::Url => json!({ "type": "string", "format": "uri" }),
        FieldKind::Slug => {
            json!({ "type": "string", "pattern": "^[a-z0-9]+(?:-[a-z0-9]+)*$" })
        }
        FieldKind::Enum => {
            let values = field.enum_meta().map(|m| m.values).unwrap_or_default();
            json!({ "type": "string", "enum": values })
        }
        FieldKind::Json => json!({}),
        FieldKind::Relation => {
            let many = field
                .relation_meta()
                .map(|m| {
                    matches!(m.cardinality, rustapi_core::field::Cardinality::ManyToMany)
                })
                .unwrap_or(false);
            if many {
                json!({ "type": "array", "items": { "type": "string", "format": "uuid" } })
            } else {
                json!({ "type": "string", "format": "uuid" })
            }
        }
        FieldKind::Media => {
            let multiple = field.media_meta().map(|m| m.multiple).unwrap_or(false);
            if multiple {
                json!({ "type": "array", "items": { "type": "string", "format": "uuid" } })
            } else {
                json!({ "type": "string", "format": "uuid" })
            }
        }
        // FieldKind is #[non_exhaustive]; stay permissive for future kinds.
        _ => json!({}),
    };
    if !field.default.is_null() {
        if let Value::Object(ref mut map) = schema {
            map.insert("default".into(), field.default.clone());
        }
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustapi_core::field::Field;
    use serde_json::json;

    fn f(kind: FieldKind, kind_meta: Value) -> Field {
        Field {
            name: "x".into(),
            kind,
            required: false,
            unique: false,
            default: Value::Null,
            max_length: None,
            kind_meta,
        }
    }

    #[test]
    fn string_has_maxlength() {
        let s = field_to_schema(&f(FieldKind::String, json!({})));
        assert_eq!(s["type"], "string");
        assert_eq!(s["maxLength"], 255);
    }

    #[test]
    fn integer_float_bool() {
        assert_eq!(field_to_schema(&f(FieldKind::Integer, json!({})))["format"], "int64");
        assert_eq!(field_to_schema(&f(FieldKind::Float, json!({})))["format"], "double");
        assert_eq!(field_to_schema(&f(FieldKind::Boolean, json!({})))["type"], "boolean");
    }

    #[test]
    fn datetime_uuid_email_url() {
        assert_eq!(field_to_schema(&f(FieldKind::Datetime, json!({})))["format"], "date-time");
        assert_eq!(field_to_schema(&f(FieldKind::Uuid, json!({})))["format"], "uuid");
        assert_eq!(field_to_schema(&f(FieldKind::Email, json!({})))["format"], "email");
        assert_eq!(field_to_schema(&f(FieldKind::Url, json!({})))["format"], "uri");
    }

    #[test]
    fn slug_has_pattern() {
        let s = field_to_schema(&f(FieldKind::Slug, json!({})));
        assert!(s["pattern"].is_string());
    }

    #[test]
    fn enum_lists_values() {
        let s = field_to_schema(&f(FieldKind::Enum, json!({ "values": ["draft", "published"] })));
        assert_eq!(s["enum"], json!(["draft", "published"]));
    }

    #[test]
    fn json_is_any() {
        assert_eq!(field_to_schema(&f(FieldKind::Json, json!({}))), json!({}));
    }

    #[test]
    fn relation_single_vs_many() {
        let one = field_to_schema(&f(FieldKind::Relation, json!({ "target": "user", "cardinality": "many_to_one" })));
        assert_eq!(one["format"], "uuid");
        let many = field_to_schema(&f(FieldKind::Relation, json!({ "target": "tag", "cardinality": "many_to_many" })));
        assert_eq!(many["type"], "array");
    }

    #[test]
    fn media_single_vs_multiple() {
        let one = field_to_schema(&f(FieldKind::Media, json!({ "multiple": false })));
        assert_eq!(one["format"], "uuid");
        let many = field_to_schema(&f(FieldKind::Media, json!({ "multiple": true })));
        assert_eq!(many["type"], "array");
    }

    #[test]
    fn default_is_emitted() {
        let mut field = f(FieldKind::Integer, json!({}));
        field.default = json!(7);
        assert_eq!(field_to_schema(&field)["default"], json!(7));
    }
}
