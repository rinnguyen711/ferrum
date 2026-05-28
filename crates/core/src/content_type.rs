//! Content type definitions.

use crate::field::{Field, FieldError};
use crate::reserved::is_valid_ident;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentType {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub fields: Vec<Field>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NewContentType {
    pub name: String,
    pub display_name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ContentTypeError {
    #[error("invalid type name")]
    BadName,
    #[error("display_name must not be empty")]
    EmptyDisplayName,
    #[error("must have at least one field")]
    NoFields,
    #[error("duplicate field name: {0}")]
    DuplicateField(String),
    #[error("invalid field `{name}`: {source}")]
    BadField {
        name: String,
        #[source]
        source: FieldError,
    },
}

impl NewContentType {
    pub fn validate(&self) -> Result<(), ContentTypeError> {
        if !is_valid_ident(&self.name) {
            return Err(ContentTypeError::BadName);
        }
        if self.display_name.trim().is_empty() {
            return Err(ContentTypeError::EmptyDisplayName);
        }
        if self.fields.is_empty() {
            return Err(ContentTypeError::NoFields);
        }
        let mut seen = std::collections::HashSet::new();
        for f in &self.fields {
            if !seen.insert(f.name.as_str()) {
                return Err(ContentTypeError::DuplicateField(f.name.clone()));
            }
            f.validate().map_err(|e| ContentTypeError::BadField {
                name: f.name.clone(),
                source: e,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldKind;
    use serde_json::json;

    fn field(name: &str) -> Field {
        Field {
            name: name.into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        }
    }

    fn nct(name: &str, fields: Vec<Field>) -> NewContentType {
        NewContentType {
            name: name.into(),
            display_name: "Display".into(),
            fields,
        }
    }

    #[test]
    fn valid_type() {
        assert!(nct("post", vec![field("title")]).validate().is_ok());
    }

    #[test]
    fn bad_name() {
        assert_eq!(
            nct("Bad", vec![field("title")]).validate().unwrap_err(),
            ContentTypeError::BadName
        );
    }

    #[test]
    fn empty_display() {
        let mut x = nct("post", vec![field("title")]);
        x.display_name = "  ".into();
        assert_eq!(x.validate().unwrap_err(), ContentTypeError::EmptyDisplayName);
    }

    #[test]
    fn no_fields() {
        assert_eq!(
            nct("post", vec![]).validate().unwrap_err(),
            ContentTypeError::NoFields
        );
    }

    #[test]
    fn duplicate_fields() {
        assert_eq!(
            nct("post", vec![field("title"), field("title")]).validate().unwrap_err(),
            ContentTypeError::DuplicateField("title".into())
        );
    }

    #[test]
    fn field_error_propagated() {
        let bad = Field {
            name: "id".into(),
            ..field("title")
        };
        let err = nct("post", vec![bad]).validate().unwrap_err();
        assert!(matches!(err, ContentTypeError::BadField { .. }));
    }
}
