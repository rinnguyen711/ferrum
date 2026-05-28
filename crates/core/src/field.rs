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
