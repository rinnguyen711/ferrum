//! Field types and value coercion.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum FieldKind {
    String,
    Text,
    Integer,
    Float,
    Boolean,
    Datetime,
    /// Postgres UUID. Used internally for the `id` system column AND for
    /// relation FK columns (phase 2.4+). Not directly user-declarable as a
    /// field kind — users declare `Relation` and the FK column infers `Uuid`.
    Uuid,
    /// Phase 2.4: declares a foreign-key reference to another content type.
    /// Configuration lives in `Field.kind_meta`; see `RelationMeta`.
    Relation,
    /// Phase 2.5: closed set of strings. Values declared in `Field.kind_meta`.
    Enum,
    /// Phase 2.5: arbitrary JSON stored as jsonb. No schema validation.
    Json,
    /// Phase 2.5: text validated against an email regex at write time.
    Email,
    /// Phase 2.5: text parsed as an http/https URL at write time.
    Url,
    /// Phase 2.5: text validated against a kebab slug regex at write time.
    Slug,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoundValue {
    /// Carries the field kind so the sqlx binder can emit a kind-typed NULL
    /// (avoids PG `42804 datatype mismatch` on placeholder cast).
    Null(FieldKind),
    Str(String),
    I64(i64),
    F64(f64),
    Bool(bool),
    DateTime(DateTime<Utc>),
    Uuid(uuid::Uuid),
    Json(serde_json::Value),
}

impl BoundValue {
    pub fn from_json(kind: FieldKind, v: &serde_json::Value) -> Result<Self, CoerceError> {
        use serde_json::Value as V;
        if v.is_null() {
            return Ok(BoundValue::Null(kind));
        }
        match (kind, v) {
            (FieldKind::String | FieldKind::Text, V::String(s)) => Ok(BoundValue::Str(s.clone())),
            (FieldKind::Integer, V::Number(n)) => n
                .as_i64()
                .map(BoundValue::I64)
                .ok_or(CoerceError::OutOfRange),
            (FieldKind::Float, V::Number(n)) => n
                .as_f64()
                .map(BoundValue::F64)
                .ok_or(CoerceError::OutOfRange),
            (FieldKind::Boolean, V::Bool(b)) => Ok(BoundValue::Bool(*b)),
            (FieldKind::Datetime, V::String(s)) => DateTime::parse_from_rfc3339(s)
                .map(|dt| BoundValue::DateTime(dt.with_timezone(&Utc)))
                .map_err(|_| CoerceError::BadDatetime),
            (FieldKind::Uuid, V::String(s)) => uuid::Uuid::parse_str(s)
                .map(BoundValue::Uuid)
                .map_err(|_| CoerceError::BadUuid),
            (FieldKind::Relation, _) => Err(CoerceError::TypeMismatch),
            (FieldKind::Json, v) => Ok(BoundValue::Json(v.clone())),
            (FieldKind::Email, V::String(s)) => {
                if crate::validators::is_valid_email(s) {
                    Ok(BoundValue::Str(s.clone()))
                } else {
                    Err(CoerceError::BadEmail)
                }
            }
            (FieldKind::Url, V::String(s)) => {
                if crate::validators::is_valid_http_url(s) {
                    Ok(BoundValue::Str(s.clone()))
                } else {
                    Err(CoerceError::BadUrl)
                }
            }
            (FieldKind::Slug, V::String(s)) => {
                if crate::validators::is_valid_slug(s) {
                    Ok(BoundValue::Str(s.clone()))
                } else {
                    Err(CoerceError::BadSlug)
                }
            }
            (FieldKind::Enum, V::String(s)) => Ok(BoundValue::Str(s.clone())),
            _ => Err(CoerceError::TypeMismatch),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum CoerceError {
    #[error("type mismatch")]
    TypeMismatch,
    #[error("value out of range")]
    OutOfRange,
    #[error("invalid RFC3339 datetime")]
    BadDatetime,
    #[error("invalid UUID")]
    BadUuid,
    #[error("invalid email")]
    BadEmail,
    #[error("invalid URL (must be http or https)")]
    BadUrl,
    #[error("invalid slug (use lowercase letters, digits, single dashes; <=200 chars)")]
    BadSlug,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coerce_string() {
        assert_eq!(
            BoundValue::from_json(FieldKind::String, &json!("hi")).unwrap(),
            BoundValue::Str("hi".into())
        );
    }

    #[test]
    fn coerce_integer_rejects_float() {
        assert!(matches!(
            BoundValue::from_json(FieldKind::Integer, &json!(1.5)),
            Err(CoerceError::OutOfRange) | Err(CoerceError::TypeMismatch)
        ));
    }

    #[test]
    fn coerce_float_accepts_int() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Float, &json!(7)).unwrap(),
            BoundValue::F64(7.0)
        );
    }

    #[test]
    fn coerce_bool() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Boolean, &json!(true)).unwrap(),
            BoundValue::Bool(true)
        );
    }

    #[test]
    fn coerce_datetime() {
        let v = BoundValue::from_json(FieldKind::Datetime, &json!("2026-05-28T10:00:00Z")).unwrap();
        assert!(matches!(v, BoundValue::DateTime(_)));
    }

    #[test]
    fn coerce_datetime_bad() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Datetime, &json!("not-a-date")).unwrap_err(),
            CoerceError::BadDatetime
        );
    }

    #[test]
    fn coerce_null_passes_through() {
        assert_eq!(
            BoundValue::from_json(FieldKind::String, &serde_json::Value::Null).unwrap(),
            BoundValue::Null(FieldKind::String)
        );
    }

    #[test]
    fn null_carries_kind() {
        // Each call returns a BoundValue::Null tagged with the requested kind.
        for kind in [FieldKind::Integer, FieldKind::Boolean, FieldKind::Datetime] {
            assert_eq!(
                BoundValue::from_json(kind, &serde_json::Value::Null).unwrap(),
                BoundValue::Null(kind)
            );
        }
    }

    #[test]
    fn coerce_type_mismatch() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Boolean, &json!("nope")).unwrap_err(),
            CoerceError::TypeMismatch
        );
    }

    #[test]
    fn coerce_uuid_ok() {
        let s = "550e8400-e29b-41d4-a716-446655440000";
        let v = BoundValue::from_json(FieldKind::Uuid, &serde_json::json!(s)).unwrap();
        match v {
            BoundValue::Uuid(u) => assert_eq!(u.to_string(), s),
            _ => panic!("expected BoundValue::Uuid"),
        }
    }

    #[test]
    fn coerce_uuid_bad() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Uuid, &serde_json::json!("not-a-uuid")).unwrap_err(),
            CoerceError::BadUuid
        );
    }

    #[test]
    fn coerce_uuid_rejects_non_string() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Uuid, &serde_json::json!(123)).unwrap_err(),
            CoerceError::TypeMismatch
        );
    }

    #[test]
    fn coerce_json_accepts_any_value() {
        let v = BoundValue::from_json(FieldKind::Json, &serde_json::json!({"k": [1, 2]})).unwrap();
        match v {
            BoundValue::Json(serde_json::Value::Object(_)) => {}
            other => panic!("expected Json(Object), got {other:?}"),
        }
        let v = BoundValue::from_json(FieldKind::Json, &serde_json::json!([1, 2, 3])).unwrap();
        assert!(matches!(v, BoundValue::Json(serde_json::Value::Array(_))));
        let v = BoundValue::from_json(FieldKind::Json, &serde_json::json!(42)).unwrap();
        assert!(matches!(v, BoundValue::Json(_)));
    }

    #[test]
    fn coerce_email_ok() {
        let v = BoundValue::from_json(FieldKind::Email, &serde_json::json!("a@b.co")).unwrap();
        assert!(matches!(v, BoundValue::Str(s) if s == "a@b.co"));
    }

    #[test]
    fn coerce_email_bad() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Email, &serde_json::json!("nope")).unwrap_err(),
            CoerceError::BadEmail
        );
    }

    #[test]
    fn coerce_email_rejects_non_string() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Email, &serde_json::json!(123)).unwrap_err(),
            CoerceError::TypeMismatch
        );
    }

    #[test]
    fn coerce_url_ok() {
        let v = BoundValue::from_json(FieldKind::Url, &serde_json::json!("https://x.io/p")).unwrap();
        assert!(matches!(v, BoundValue::Str(_)));
    }

    #[test]
    fn coerce_url_bad() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Url, &serde_json::json!("ftp://x.io")).unwrap_err(),
            CoerceError::BadUrl
        );
    }

    #[test]
    fn coerce_slug_ok() {
        let v = BoundValue::from_json(FieldKind::Slug, &serde_json::json!("hello-world")).unwrap();
        assert!(matches!(v, BoundValue::Str(s) if s == "hello-world"));
    }

    #[test]
    fn coerce_slug_bad() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Slug, &serde_json::json!("Bad Slug!")).unwrap_err(),
            CoerceError::BadSlug
        );
    }

    #[test]
    fn coerce_enum_returns_str() {
        // Enum coercion does not check membership (no kind_meta access).
        // Service layer validates after coerce. Just confirm it produces Str.
        let v = BoundValue::from_json(FieldKind::Enum, &serde_json::json!("draft")).unwrap();
        assert!(matches!(v, BoundValue::Str(s) if s == "draft"));
    }

    #[test]
    fn coerce_enum_rejects_non_string() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Enum, &serde_json::json!(42)).unwrap_err(),
            CoerceError::TypeMismatch
        );
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationMeta {
    pub target: String,
    pub cardinality: String,
    pub inverse: Option<String>,
}

impl RelationMeta {
    pub fn from_value(v: &serde_json::Value) -> Result<Self, FieldError> {
        let obj = v.as_object().ok_or(FieldError::RelationMetaShape)?;
        let target = obj
            .get("target")
            .and_then(|x| x.as_str())
            .ok_or(FieldError::RelationMetaShape)?
            .to_string();
        if !crate::reserved::is_valid_ident(&target) {
            return Err(FieldError::RelationMetaShape);
        }
        let cardinality = obj
            .get("cardinality")
            .and_then(|x| x.as_str())
            .ok_or(FieldError::RelationMetaShape)?
            .to_string();
        if cardinality != "many_to_one" {
            return Err(FieldError::BadCardinality);
        }
        let inverse = match obj.get("inverse") {
            None => None,
            Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(s)) => {
                if !crate::reserved::is_valid_ident(s) || crate::reserved::is_reserved(s) {
                    return Err(FieldError::InverseNameInvalid);
                }
                Some(s.clone())
            }
            _ => return Err(FieldError::RelationMetaShape),
        };
        // Reject unknown keys to keep the surface tight.
        for key in obj.keys() {
            if !matches!(key.as_str(), "target" | "cardinality" | "inverse") {
                return Err(FieldError::RelationMetaShape);
            }
        }
        Ok(Self { target, cardinality, inverse })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumMeta {
    pub values: Vec<String>,
}

impl EnumMeta {
    pub fn from_value(v: &serde_json::Value) -> Result<Self, FieldError> {
        let obj = v.as_object().ok_or(FieldError::EnumMetaShape)?;
        for key in obj.keys() {
            if key != "values" {
                return Err(FieldError::EnumMetaShape);
            }
        }
        let arr = obj
            .get("values")
            .and_then(|x| x.as_array())
            .ok_or(FieldError::EnumMetaShape)?;
        if arr.is_empty() {
            return Err(FieldError::EnumValuesEmpty);
        }
        let mut values = Vec::with_capacity(arr.len());
        let mut seen = std::collections::HashSet::new();
        for item in arr {
            let s = item.as_str().ok_or(FieldError::EnumMetaShape)?;
            if !crate::reserved::is_valid_ident(s) {
                return Err(FieldError::EnumValueInvalidIdent(s.to_string()));
            }
            if !seen.insert(s.to_string()) {
                return Err(FieldError::EnumValueDuplicate(s.to_string()));
            }
            values.push(s.to_string());
        }
        Ok(Self { values })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub max_length: Option<u32>,
    #[serde(default = "default_kind_meta", skip_serializing_if = "is_empty_obj")]
    pub kind_meta: serde_json::Value,
}

fn default_kind_meta() -> serde_json::Value {
    serde_json::json!({})
}

fn is_empty_obj(v: &serde_json::Value) -> bool {
    v.as_object().is_some_and(|o| o.is_empty())
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FieldError {
    #[error("invalid name: must match ^[a-z][a-z0-9_]{{0,62}}$")]
    BadName,
    #[error("reserved name")]
    Reserved,
    #[error("max_length must be 1..=10000")]
    BadMaxLength,
    #[error("kind_meta must be empty for primitive kinds")]
    KindMetaNotEmpty,
    #[error("default value does not match kind")]
    BadDefault,
    #[error("relation kind_meta must have {{target, cardinality, inverse?}} with valid ident target")]
    RelationMetaShape,
    #[error("cardinality must be \"many_to_one\" in v2.4")]
    BadCardinality,
    #[error("inverse name invalid or reserved")]
    InverseNameInvalid,
    #[error("relation field cannot be unique")]
    RelationFieldUniqueUnsupported,
    #[error("relation field cannot have a default")]
    RelationFieldDefaultUnsupported,
    #[error("enum kind_meta must be {{values: [..]}} of valid idents")]
    EnumMetaShape,
    #[error("enum values list must contain at least one value")]
    EnumValuesEmpty,
    #[error("enum value `{0}` is not a valid identifier")]
    EnumValueInvalidIdent(String),
    #[error("enum value `{0}` appears more than once")]
    EnumValueDuplicate(String),
    #[error("enum default is not in the values list")]
    EnumDefaultNotInValues,
    #[error("json field cannot be unique")]
    JsonUniqueUnsupported,
}

impl Field {
    pub fn validate(&self) -> Result<(), FieldError> {
        if !crate::reserved::is_valid_ident(&self.name) {
            return Err(FieldError::BadName);
        }
        if crate::reserved::is_reserved(&self.name) {
            return Err(FieldError::Reserved);
        }
        match self.max_length {
            Some(n) if !(1..=10_000).contains(&n) => return Err(FieldError::BadMaxLength),
            _ => {}
        }
        if self.kind == FieldKind::Relation {
            if self.unique {
                return Err(FieldError::RelationFieldUniqueUnsupported);
            }
            if !self.default.is_null() {
                return Err(FieldError::RelationFieldDefaultUnsupported);
            }
            let _ = RelationMeta::from_value(&self.kind_meta)?;
            return Ok(());
        }
        // Primitive kinds: kind_meta must remain empty (existing v1 rule).
        if !is_empty_obj(&self.kind_meta) {
            return Err(FieldError::KindMetaNotEmpty);
        }
        if !self.default.is_null() {
            BoundValue::from_json(self.kind, &self.default)
                .map_err(|_| FieldError::BadDefault)?;
        }
        Ok(())
    }

    pub fn effective_max_length(&self) -> u32 {
        self.max_length.unwrap_or(255)
    }

    /// Resolve the physical SQL column name for this field. Primitives use the
    /// declared name; relation fields suffix `_id`.
    pub fn physical_column(&self) -> String {
        if self.kind == FieldKind::Relation {
            format!("{}_id", self.name)
        } else {
            self.name.clone()
        }
    }

    /// Returns the relation meta if this is a relation field, otherwise `None`.
    pub fn relation_meta(&self) -> Option<RelationMeta> {
        if self.kind == FieldKind::Relation {
            RelationMeta::from_value(&self.kind_meta).ok()
        } else {
            None
        }
    }

    pub fn enum_meta(&self) -> Option<EnumMeta> {
        if self.kind == FieldKind::Enum {
            EnumMeta::from_value(&self.kind_meta).ok()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod field_tests {
    use super::*;
    use serde_json::json;

    fn f(name: &str, kind: FieldKind) -> Field {
        Field {
            name: name.into(),
            kind,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: default_kind_meta(),
        }
    }

    #[test]
    fn valid_string_field() {
        assert!(f("title", FieldKind::String).validate().is_ok());
    }

    #[test]
    fn reject_reserved_name() {
        assert_eq!(f("id", FieldKind::String).validate().unwrap_err(), FieldError::Reserved);
    }

    #[test]
    fn reject_bad_name() {
        assert_eq!(f("Bad", FieldKind::String).validate().unwrap_err(), FieldError::BadName);
    }

    #[test]
    fn reject_bad_max_length() {
        let mut x = f("title", FieldKind::String);
        x.max_length = Some(0);
        assert_eq!(x.validate().unwrap_err(), FieldError::BadMaxLength);
        x.max_length = Some(20_000);
        assert_eq!(x.validate().unwrap_err(), FieldError::BadMaxLength);
    }

    #[test]
    fn reject_non_empty_kind_meta() {
        let mut x = f("title", FieldKind::String);
        x.kind_meta = json!({"x":1});
        assert_eq!(x.validate().unwrap_err(), FieldError::KindMetaNotEmpty);
    }

    #[test]
    fn reject_default_kind_mismatch() {
        let mut x = f("count", FieldKind::Integer);
        x.default = json!("not-int");
        assert_eq!(x.validate().unwrap_err(), FieldError::BadDefault);
    }

    #[test]
    fn accept_valid_default() {
        let mut x = f("count", FieldKind::Integer);
        x.default = json!(7);
        assert!(x.validate().is_ok());
    }
}

#[cfg(test)]
mod relation_meta_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_minimal_meta() {
        let m = RelationMeta::from_value(&json!({
            "target": "user",
            "cardinality": "many_to_one"
        }))
        .unwrap();
        assert_eq!(m.target, "user");
        assert_eq!(m.cardinality, "many_to_one");
        assert!(m.inverse.is_none());
    }

    #[test]
    fn parse_with_inverse() {
        let m = RelationMeta::from_value(&json!({
            "target": "user",
            "cardinality": "many_to_one",
            "inverse": "posts"
        }))
        .unwrap();
        assert_eq!(m.inverse.as_deref(), Some("posts"));
    }

    #[test]
    fn reject_missing_target() {
        assert_eq!(
            RelationMeta::from_value(&json!({"cardinality": "many_to_one"})).unwrap_err(),
            FieldError::RelationMetaShape
        );
    }

    #[test]
    fn reject_bad_cardinality() {
        assert_eq!(
            RelationMeta::from_value(&json!({"target":"user","cardinality":"one_to_many"}))
                .unwrap_err(),
            FieldError::BadCardinality
        );
        assert_eq!(
            RelationMeta::from_value(&json!({"target":"user","cardinality":"many_to_many"}))
                .unwrap_err(),
            FieldError::BadCardinality
        );
        assert_eq!(
            RelationMeta::from_value(&json!({"target":"user","cardinality":"nonsense"}))
                .unwrap_err(),
            FieldError::BadCardinality
        );
    }

    #[test]
    fn reject_inverse_bad_ident() {
        assert_eq!(
            RelationMeta::from_value(&json!({
                "target":"user","cardinality":"many_to_one","inverse":"Bad"
            }))
            .unwrap_err(),
            FieldError::InverseNameInvalid
        );
    }

    #[test]
    fn reject_inverse_reserved() {
        assert_eq!(
            RelationMeta::from_value(&json!({
                "target":"user","cardinality":"many_to_one","inverse":"id"
            }))
            .unwrap_err(),
            FieldError::InverseNameInvalid
        );
    }

    #[test]
    fn validate_relation_field_basic() {
        let f = Field {
            name: "author".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
        };
        assert!(f.validate().is_ok());
    }

    #[test]
    fn validate_relation_rejects_unique() {
        let mut f = Field {
            name: "author".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: true,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
        };
        assert_eq!(f.validate().unwrap_err(), FieldError::RelationFieldUniqueUnsupported);
        f.unique = false;
        f.default = json!("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(f.validate().unwrap_err(), FieldError::RelationFieldDefaultUnsupported);
    }

    #[test]
    fn validate_primitive_still_rejects_non_empty_kind_meta() {
        let f = Field {
            name: "title".into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"x":1}),
        };
        assert_eq!(f.validate().unwrap_err(), FieldError::KindMetaNotEmpty);
    }
}

#[cfg(test)]
mod enum_meta_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_minimal() {
        let m = EnumMeta::from_value(&json!({"values": ["draft", "published"]})).unwrap();
        assert_eq!(m.values, vec!["draft".to_string(), "published".to_string()]);
    }

    #[test]
    fn reject_missing_values() {
        assert_eq!(
            EnumMeta::from_value(&json!({})).unwrap_err(),
            FieldError::EnumMetaShape
        );
    }

    #[test]
    fn reject_empty_values() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": []})).unwrap_err(),
            FieldError::EnumValuesEmpty
        );
    }

    #[test]
    fn reject_non_string_value() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": ["a", 1]})).unwrap_err(),
            FieldError::EnumMetaShape
        );
    }

    #[test]
    fn reject_invalid_ident() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": ["Bad-Value"]})).unwrap_err(),
            FieldError::EnumValueInvalidIdent("Bad-Value".into())
        );
    }

    #[test]
    fn reject_duplicate() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": ["a", "b", "a"]})).unwrap_err(),
            FieldError::EnumValueDuplicate("a".into())
        );
    }

    #[test]
    fn reject_extra_keys() {
        assert_eq!(
            EnumMeta::from_value(&json!({"values": ["a"], "extra": 1})).unwrap_err(),
            FieldError::EnumMetaShape
        );
    }
}
