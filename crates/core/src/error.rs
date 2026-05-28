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
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FieldValidation {
    pub field: String,
    pub reason: String,
}

impl ValidationErrors {
    pub fn single(msg: impl Into<String>) -> Self {
        Self {
            fields: vec![],
            message: Some(msg.into()),
        }
    }

    pub fn field(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            fields: vec![FieldValidation {
                field: name.into(),
                reason: reason.into(),
            }],
            message: None,
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
