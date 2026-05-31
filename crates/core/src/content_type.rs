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
    #[error("field name `{0}` collides with physical column of another field")]
    ColumnCollision(String),
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
        let mut cols: std::collections::HashSet<String> = std::collections::HashSet::new();
        for f in &self.fields {
            let col = f.physical_column();
            if !cols.insert(col.clone()) {
                return Err(ContentTypeError::ColumnCollision(col));
            }
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

    #[test]
    fn reject_relation_id_collides_with_primitive_field() {
        // Relation field `author` produces column `author_id`; primitive field
        // named `author_id` would collide.
        let relation = Field {
            name: "author".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
        };
        let dup = Field {
            name: "author_id".into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        };
        let err = nct("post", vec![relation, dup]).validate().unwrap_err();
        assert_eq!(err, ContentTypeError::ColumnCollision("author_id".into()));
    }

    #[test]
    fn reject_two_relations_with_same_physical_column() {
        // Two relations whose physical columns collide is impossible without
        // identical names (caught by DuplicateField), but include this as a
        // sanity check anyway: relation `author` + relation `author` would
        // hit DuplicateField, not ColumnCollision. We confirm ColumnCollision
        // fires only when names differ but columns match.
        let r1 = Field {
            name: "x".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
        };
        let dup_name = Field {
            name: "x_id".into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        };
        let err = nct("post", vec![r1, dup_name]).validate().unwrap_err();
        assert_eq!(err, ContentTypeError::ColumnCollision("x_id".into()));
    }

    #[test]
    fn relation_field_without_collision_validates() {
        let relation = Field {
            name: "author".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
        };
        assert!(nct("post", vec![field("title"), relation]).validate().is_ok());
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
    #[error("field name `{0}` collides with physical column of another field")]
    ColumnCollision(String),
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
        let drop_set: std::collections::HashSet<&str> =
            self.drop_fields.iter().map(|s| s.as_str()).collect();

        for f in &self.add_fields {
            f.validate().map_err(|e| PatchError::BadField {
                name: f.name.clone(),
                source: e,
            })?;
            // Reject drop+add of the same name in one patch: this would be a
            // rename or kind change, both unsupported in v1 per spec §4.1.
            if drop_set.contains(f.name.as_str()) {
                return Err(PatchError::DuplicateAddField(f.name.clone()));
            }
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
        // Post-mutation field set = (existing − drops) + adds. The drop-then-readd
        // case is already rejected above by DuplicateAddField, so no overlap risk.
        let mut cols: std::collections::HashSet<String> = std::collections::HashSet::new();
        for f in existing.fields.iter().filter(|f| !drop_set.contains(f.name.as_str())) {
            let col = f.physical_column();
            if !cols.insert(col.clone()) {
                // This would only fire if an EXISTING type already had a collision —
                // shouldn't happen since the type was previously validated. Treat
                // as a defensive error.
                return Err(PatchError::ColumnCollision(col));
            }
        }
        for f in &self.add_fields {
            let col = f.physical_column();
            if !cols.insert(col.clone()) {
                return Err(PatchError::ColumnCollision(col));
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

    #[test]
    fn drop_then_add_same_name_rejected() {
        // Spec §4.1: rename and kind change are unsupported in v1.
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![Field {
                name: "title".into(),
                kind: FieldKind::Text,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({}),
            }],
            drop_fields: vec!["title".into()],
        };
        assert!(matches!(
            p.validate(&existing()).unwrap_err(),
            PatchError::DuplicateAddField(_)
        ));
    }

    #[test]
    fn patch_add_rejects_relation_colliding_with_existing_primitive() {
        // Existing fixture has `title`. Add a primitive `author_id` first,
        // then attempt to also add a relation `author` (whose physical column
        // is `author_id`). The post-mutation field set would contain both
        // `author_id` (primitive) and `author` (relation → column author_id).
        let mut existing_with_author_id = existing();
        existing_with_author_id.fields.push(Field {
            name: "author_id".into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        });
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![Field {
                name: "author".into(),
                kind: FieldKind::Relation,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
            }],
            drop_fields: vec![],
        };
        let err = p.validate(&existing_with_author_id).unwrap_err();
        assert_eq!(err, PatchError::ColumnCollision("author_id".into()));
    }

    #[test]
    fn patch_drop_then_add_clears_collision() {
        // Existing has `title` and a primitive `author_id`. Drop `author_id`,
        // add relation `author`. Post-mutation set has only `author` whose
        // column is `author_id`. No collision.
        let mut existing_with_author_id = existing();
        existing_with_author_id.fields.push(Field {
            name: "author_id".into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        });
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![Field {
                name: "author".into(),
                kind: FieldKind::Relation,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
            }],
            drop_fields: vec!["author_id".into()],
        };
        assert!(p.validate(&existing_with_author_id).is_ok());
    }
}
