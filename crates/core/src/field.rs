//! Field types and value coercion.

use crate::reserved::{is_reserved, is_valid_ident};
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoundValue {
    Null,
    Str(String),
    I64(i64),
    F64(f64),
    Bool(bool),
    DateTime(DateTime<Utc>),
}

impl BoundValue {
    pub fn from_json(kind: FieldKind, v: &serde_json::Value) -> Result<Self, CoerceError> {
        use serde_json::Value as V;
        if v.is_null() {
            return Ok(BoundValue::Null);
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
            BoundValue::Null
        );
    }

    #[test]
    fn coerce_type_mismatch() {
        assert_eq!(
            BoundValue::from_json(FieldKind::Boolean, &json!("nope")).unwrap_err(),
            CoerceError::TypeMismatch
        );
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
    #[error("kind_meta must be empty in v1")]
    KindMetaNotEmpty,
    #[error("default value does not match kind")]
    BadDefault,
}

impl Field {
    pub fn validate(&self) -> Result<(), FieldError> {
        if !is_valid_ident(&self.name) {
            return Err(FieldError::BadName);
        }
        if is_reserved(&self.name) {
            return Err(FieldError::Reserved);
        }
        match self.max_length {
            Some(n) if !(1..=10_000).contains(&n) => return Err(FieldError::BadMaxLength),
            _ => {}
        }
        if !is_empty_obj(&self.kind_meta) {
            return Err(FieldError::KindMetaNotEmpty);
        }
        if !self.default.is_null() {
            BoundValue::from_json(self.kind, &self.default).map_err(|_| FieldError::BadDefault)?;
        }
        Ok(())
    }

    pub fn effective_max_length(&self) -> u32 {
        self.max_length.unwrap_or(255)
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
