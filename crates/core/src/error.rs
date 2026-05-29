//! Top-level error type returned across crate boundaries.

use crate::content_type::{ContentTypeError, PatchError};
use crate::field::CoerceError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("validation failed")]
    Validation(ValidationErrors),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ValidationErrors {
    pub fields: Vec<FieldValidation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db: Option<DbInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FieldValidation {
    pub field: String,
    pub reason: String,
}

/// Surfaces a Postgres error to the client per spec §5.6
/// (`error.details.db = {code, message}`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DbInfo {
    pub code: String,
    pub message: String,
}

impl ValidationErrors {
    pub fn single(msg: impl Into<String>) -> Self {
        Self {
            fields: vec![],
            message: Some(msg.into()),
            db: None,
        }
    }

    pub fn field(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            fields: vec![FieldValidation {
                field: name.into(),
                reason: reason.into(),
            }],
            message: None,
            db: None,
        }
    }

    pub fn db(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            fields: vec![],
            message: Some("database rejected the operation".into()),
            db: Some(DbInfo {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

impl From<ContentTypeError> for Error {
    fn from(e: ContentTypeError) -> Self {
        Error::Validation(ValidationErrors::single(e.to_string()))
    }
}

impl From<PatchError> for Error {
    fn from(e: PatchError) -> Self {
        Error::Validation(ValidationErrors::single(e.to_string()))
    }
}

impl From<CoerceError> for Error {
    fn from(e: CoerceError) -> Self {
        Error::Validation(ValidationErrors::single(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_field_helper() {
        let v = ValidationErrors::field("title", "required");
        assert_eq!(v.fields.len(), 1);
        assert_eq!(v.fields[0].field, "title");
    }
}
