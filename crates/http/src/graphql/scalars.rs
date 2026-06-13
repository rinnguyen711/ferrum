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
/// on write, matching the read shape. Relation/media are written as scalar
/// UUID id(s) — only the output side surfaces them as object refs.
pub fn input_type_ref(field: &Field) -> TypeRef {
    wrap_ref(input_base_type_name(field), is_list(field), field.required)
}

/// Base GraphQL type name for a field on the INPUT side. Identical to
/// `base_type_name` except relation/media stay scalar UUID id(s) — output
/// objects can't be used as input types, and writes take the target/asset id.
fn input_base_type_name(field: &Field) -> String {
    match field.kind {
        FieldKind::Relation | FieldKind::Media => UUID_SCALAR.to_string(),
        _ => base_type_name(field),
    }
}

/// Base GraphQL type name for a field's value (before list/non-null wrapping).
/// Relation fields are typed as the target content type's object (PascalCase);
/// media fields as the shared `Media` object. List-ness (m2m relation /
/// multiple media) is applied by `is_list`. build.rs registers an object for
/// every content type (incl. Single targets) and the `Media` object, so these
/// refs never dangle on `Schema::finish()`. Relation/media fields are populated
/// one level deep via the selection set; unpopulated relations resolve to null.
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
        // Relation → the target type's object (PascalCase). Media → the shared
        // `Media` object. Both are registered for every content type in
        // build.rs, so the ref never dangles (even for Single-type targets).
        // List-ness (m2m / multiple) is applied by `is_list` in `wrap_ref`.
        FieldKind::Relation => crate::graphql::build::pascal(
            &field.relation_meta().map(|m| m.target).unwrap_or_default(),
        ),
        FieldKind::Media => "Media".to_string(),
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
    fn relation_base_is_target_object() {
        // many_to_one relation → single object ref named after the target (PascalCase)
        let mto = f(
            FieldKind::Relation,
            false,
            json!({ "target": "writer", "cardinality": "many_to_one" }),
        );
        assert_eq!(base_type_name(&mto), "Writer");
        assert!(!is_list(&mto));

        // many_to_many → still object ref (Tag), but is_list = true
        let mtm = f(
            FieldKind::Relation,
            false,
            json!({ "target": "tag", "cardinality": "many_to_many" }),
        );
        assert_eq!(base_type_name(&mtm), "Tag");
        assert!(is_list(&mtm));
    }
    #[test]
    fn media_base_is_media_object() {
        let single = f(FieldKind::Media, false, json!({ "multiple": false }));
        assert_eq!(base_type_name(&single), "Media");
        assert!(!is_list(&single));

        let multiple = f(FieldKind::Media, false, json!({ "multiple": true }));
        assert_eq!(base_type_name(&multiple), "Media");
        assert!(is_list(&multiple));
    }
    #[test]
    fn enum_name_is_field_pascal_plus_enum() {
        let mut e = f(FieldKind::Enum, false, json!({"values":["a","b"]}));
        e.name = "status".into();
        assert_eq!(enum_type_name(&e), "StatusEnum");
    }
    #[test]
    fn input_list_matches_output_list() {
        // A non-relation/media field has identical input + output type refs.
        let multi = f(FieldKind::Json, false, json!({}));
        assert_eq!(
            format!("{:?}", super::input_type_ref(&multi)),
            format!("{:?}", super::field_type_ref(&multi))
        );
        // For relations the list/non-null SHAPE matches even though the base
        // differs (input = scalar UUID, output = target object).
        let many = f(
            FieldKind::Relation,
            false,
            json!({"target":"tag","cardinality":"many_to_many"}),
        );
        let inp = format!("{:?}", super::input_type_ref(&many));
        let out = format!("{:?}", super::field_type_ref(&many));
        assert!(inp.contains("UUID"), "input relation is scalar uuid: {inp}");
        assert!(
            out.contains("Tag"),
            "output relation is target object: {out}"
        );
        // both are non-null lists (m2m): same wrapper kind.
        assert!(inp.contains("List"));
        assert!(out.contains("List"));
    }
}
