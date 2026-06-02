//! Top-level error type returned across crate boundaries.

use crate::content_type::{ContentTypeError, PatchError};
use crate::field::CoerceError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("validation failed")]
    Validation(ValidationErrors),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// Postgres 23503 FK violation when a referencing row blocks a delete.
    /// Phase 2.4 relations.
    #[error("relation fk violation")]
    RelationFkViolation { constraint: Option<String> },
    #[error("enum value `{value}` not allowed for field `{field}`")]
    EnumValueNotAllowed {
        field: String,
        value: String,
        allowed: Vec<String>,
    },
    #[error("invalid email")]
    BadEmail,
    #[error("invalid URL")]
    BadUrl,
    #[error("invalid slug")]
    BadSlug,
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
    /// Phase 2.4: list of relation-target ids that didn't resolve. Only set
    /// by `ValidationErrors::relation_target_missing`. Surfaced under
    /// `details.missing_ids` in the HTTP response.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing_ids: Vec<String>,
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
            missing_ids: vec![],
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
            missing_ids: vec![],
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
            missing_ids: vec![],
        }
    }

    /// Phase 2.4: a relation write referenced ids that don't exist in the
    /// target table. Caller supplies the relation field name and the missing
    /// ids; the response body carries them under `details.field` and
    /// `details.missing_ids`.
    pub fn relation_target_missing(
        field: impl Into<String>,
        missing_ids: Vec<String>,
    ) -> Self {
        Self {
            fields: vec![FieldValidation {
                field: field.into(),
                reason: format!("relation target missing: {} id(s)", missing_ids.len()),
            }],
            message: Some("relation target missing".into()),
            db: None,
            missing_ids,
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
