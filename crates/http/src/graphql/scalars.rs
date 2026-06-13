//! Maps the content-type field model to async-graphql dynamic TypeRefs,
//! mirroring the decisions in `openapi/schema.rs`. Keep the two in sync.

use async_graphql::dynamic::TypeRef;
use rustapi_core::field::{Cardinality, Field, FieldKind};

/// Custom scalar names registered on the schema.
pub const UUID_SCALAR: &str = "UUID";
pub const DATETIME_SCALAR: &str = "DateTime";
pub const JSON_SCALAR: &str = "JSON";

/// Shared list/non-null wrapping for output and input type refs.
fn wrap_ref(base: String, many: bool, required: bool) -> TypeRef {
    match (many, required) {
        (true, true) => TypeRef::named_nn_list_nn(base),
        (true, false) => TypeRef::named_nn_list(base),
        (false, true) => TypeRef::named_nn(base),
        (false, false) => TypeRef::named(base),
    }
}

/// The GraphQL output type for a field (used in output objects).
/// Non-null when the field is required.
pub fn field_type_ref(field: &Field) -> TypeRef {
    wrap_ref(base_type_name(field), is_list(field), field.required)
}

/// The GraphQL input type for a field. Same list/non-null shape as the output
/// type ref, so list-valued fields (m2m relation, multiple media) accept lists
/// on write, matching the read shape.
pub fn input_type_ref(field: &Field) -> TypeRef {
    // relations/media on input are the scalar id type(s) — base_type_name
    // already yields the right base.
    wrap_ref(base_type_name(field), is_list(field), field.required)
}

/// Base GraphQL type name for a field's value (before list/non-null wrapping).
/// Relation/Media are represented as scalar UUID id(s) in v1 — a relation is
/// the target row's UUID and media is the asset UUID (a list of UUIDs for m2m
/// relations / multiple media, via `is_list`). Nested object population is
/// deferred, so these are NOT object refs: typing them as object refs would
/// dangle when the target isn't surfaced (e.g. a relation to a Single type),
/// breaking `Schema::finish()`.
pub fn base_type_name(field: &Field) -> String {
    match field.kind {
        FieldKind::String
        | FieldKind::Text
        | FieldKind::Slug
        | FieldKind::Email
        | FieldKind::Url => TypeRef::STRING.to_string(),
        FieldKind::Integer => TypeRef::INT.to_string(),
        FieldKind::Float => TypeRef::FLOAT.to_string(),
        FieldKind::Boolean => TypeRef::BOOLEAN.to_string(),
        FieldKind::Datetime => DATETIME_SCALAR.to_string(),
        FieldKind::Uuid => UUID_SCALAR.to_string(),
        FieldKind::Enum => enum_type_name(field),
        FieldKind::Json => JSON_SCALAR.to_string(),
        // Relation/Media surface as UUID id(s); list-ness handled by `is_list`.
        FieldKind::Relation | FieldKind::Media => UUID_SCALAR.to_string(),
        _ => JSON_SCALAR.to_string(),
    }
}

/// True when the field encodes a list (m2m relation or multiple media).
pub fn is_list(field: &Field) -> bool {
    match field.kind {
        FieldKind::Relation => field
            .relation_meta()
            .map(|m| matches!(m.cardinality, Cardinality::ManyToMany))
            .unwrap_or(false),
        FieldKind::Media => field.media_meta().map(|m| m.multiple).unwrap_or(false),
        _ => false,
    }
}

/// Enum GraphQL type name: `<Pascal(field_name)>Enum`. build.rs registers one
/// Enum type per enum field using this same name.
pub fn enum_type_name(field: &Field) -> String {
    format!("{}Enum", crate::graphql::build::pascal(&field.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustapi_core::field::Field;
    use serde_json::{json, Value};

    fn f(kind: FieldKind, required: bool, kind_meta: Value) -> Field {
        Field {
            name: "x".into(),
            kind,
            required,
            unique: false,
            default: Value::Null,
            max_length: None,
            kind_meta,
        }
    }

    #[test]
    fn string_maps_to_string() {
        assert_eq!(
            base_type_name(&f(FieldKind::String, false, json!({}))),
            "String"
        );
    }
    #[test]
    fn integer_float_bool() {
        assert_eq!(
            base_type_name(&f(FieldKind::Integer, false, json!({}))),
            "Int"
        );
        assert_eq!(
            base_type_name(&f(FieldKind::Float, false, json!({}))),
            "Float"
        );
        assert_eq!(
            base_type_name(&f(FieldKind::Boolean, false, json!({}))),
            "Boolean"
        );
    }
    #[test]
    fn datetime_uuid_json_scalars() {
        assert_eq!(
            base_type_name(&f(FieldKind::Datetime, false, json!({}))),
            "DateTime"
        );
        assert_eq!(
            base_type_name(&f(FieldKind::Uuid, false, json!({}))),
            "UUID"
        );
        assert_eq!(
            base_type_name(&f(FieldKind::Json, false, json!({}))),
            "JSON"
        );
    }
    #[test]
    fn relation_single_not_list_many_is_list() {
        // v1: relation fields surface as scalar UUID id(s), not object refs.
        let one = f(
            FieldKind::Relation,
            false,
            json!({"target":"user","cardinality":"many_to_one"}),
        );
        assert!(!is_list(&one));
        assert_eq!(base_type_name(&one), "UUID");
        let many = f(
            FieldKind::Relation,
            false,
            json!({"target":"tag","cardinality":"many_to_many"}),
        );
        assert!(is_list(&many));
        assert_eq!(base_type_name(&many), "UUID");
    }
    #[test]
    fn media_single_vs_multiple() {
        // v1: media fields surface as scalar UUID id(s), not object refs.
        let single = f(FieldKind::Media, false, json!({"multiple": false}));
        assert!(!is_list(&single));
        assert_eq!(base_type_name(&single), "UUID");
        let multiple = f(FieldKind::Media, false, json!({"multiple": true}));
        assert!(is_list(&multiple));
        assert_eq!(base_type_name(&multiple), "UUID");
    }
    #[test]
    fn enum_name_is_field_pascal_plus_enum() {
        let mut e = f(FieldKind::Enum, false, json!({"values":["a","b"]}));
        e.name = "status".into();
        assert_eq!(enum_type_name(&e), "StatusEnum");
    }
    #[test]
    fn input_list_matches_output_list() {
        let many = f(
            FieldKind::Relation,
            false,
            json!({"target":"tag","cardinality":"many_to_many"}),
        );
        // input and output use the same list/non-null wrapping for a field.
        assert_eq!(
            format!("{:?}", super::input_type_ref(&many)),
            format!("{:?}", super::field_type_ref(&many))
        );
    }
}
