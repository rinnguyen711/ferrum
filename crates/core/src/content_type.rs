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

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PatchContentType {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub add_fields: Vec<Field>,
    #[serde(default)]
    pub drop_fields: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PatchError {
    #[error("display_name must not be empty")]
    EmptyDisplayName,
    #[error("invalid field `{name}`: {source}")]
    BadField {
        name: String,
        #[source]
        source: FieldError,
    },
    #[error("cannot drop unknown field `{0}`")]
    UnknownDropField(String),
    #[error("cannot drop system column `{0}`")]
    DropSystemField(String),
    #[error("field `{0}` already exists")]
    DuplicateAddField(String),
    #[error("patch is a no-op")]
    NoOp,
}

impl PatchContentType {
    pub fn validate(&self, existing: &ContentType) -> Result<(), PatchError> {
        if self.display_name.is_none() && self.add_fields.is_empty() && self.drop_fields.is_empty() {
            return Err(PatchError::NoOp);
        }
        if let Some(d) = &self.display_name {
            if d.trim().is_empty() {
                return Err(PatchError::EmptyDisplayName);
            }
        }
        let existing_names: std::collections::HashSet<&str> =
            existing.fields.iter().map(|f| f.name.as_str()).collect();

        for f in &self.add_fields {
            f.validate().map_err(|e| PatchError::BadField {
                name: f.name.clone(),
                source: e,
            })?;
            if existing_names.contains(f.name.as_str()) {
                return Err(PatchError::DuplicateAddField(f.name.clone()));
            }
        }
        for name in &self.drop_fields {
            if crate::system::is_system_column(name) {
                return Err(PatchError::DropSystemField(name.clone()));
            }
            if !existing_names.contains(name.as_str()) {
                return Err(PatchError::UnknownDropField(name.clone()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod patch_tests {
    use super::*;
    use crate::field::FieldKind;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn existing() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "title".into(),
                kind: FieldKind::String,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn noop_rejected() {
        let p = PatchContentType { display_name: None, add_fields: vec![], drop_fields: vec![] };
        assert_eq!(p.validate(&existing()).unwrap_err(), PatchError::NoOp);
    }

    #[test]
    fn drop_unknown() {
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec!["missing".into()],
        };
        assert!(matches!(p.validate(&existing()).unwrap_err(), PatchError::UnknownDropField(_)));
    }

    #[test]
    fn drop_system() {
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec!["id".into()],
        };
        assert!(matches!(p.validate(&existing()).unwrap_err(), PatchError::DropSystemField(_)));
    }

    #[test]
    fn duplicate_add() {
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![Field {
                name: "title".into(),
                kind: FieldKind::String,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({}),
            }],
            drop_fields: vec![],
        };
        assert!(matches!(p.validate(&existing()).unwrap_err(), PatchError::DuplicateAddField(_)));
    }
}
