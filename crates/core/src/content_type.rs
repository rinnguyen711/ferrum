//! Content type definitions.

use crate::field::{Field, FieldError};
use crate::reserved::is_valid_ident;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentTypeKind {
    #[default]
    Collection,
    Single,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentType {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub options: serde_json::Value,
    #[serde(default)]
    pub kind: ContentTypeKind,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NewContentType {
    pub name: String,
    pub display_name: String,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub options: serde_json::Value,
    #[serde(default)]
    pub kind: ContentTypeKind,
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

impl ContentType {
    /// Whether Draft & Publish is enabled. Absent/invalid `options` → false.
    pub fn draft_publish(&self) -> bool {
        self.options
            .get("draft_publish")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Whether this type is managed by a schema file (TOML sync). Absent/invalid
    /// `options` → false. Managed types are read-only in the UI/API.
    pub fn managed(&self) -> bool {
        self.options
            .get("managed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

impl NewContentType {
    /// Resolve effective options for create: `draft_publish` defaults to false
    /// when the client omitted it. Returns the caller's options object with
    /// `draft_publish` filled in, preserving any extra keys (e.g. `managed`).
    pub fn resolved_options(&self) -> serde_json::Value {
        let dp = self
            .options
            .get("draft_publish")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut obj = self
            .options
            .as_object()
            .cloned()
            .unwrap_or_default();
        obj.insert("draft_publish".into(), serde_json::Value::Bool(dp));
        serde_json::Value::Object(obj)
    }

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
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

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
            options: json!({}),
            kind: ContentTypeKind::Collection,
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
        assert_eq!(
            x.validate().unwrap_err(),
            ContentTypeError::EmptyDisplayName
        );
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
            nct("post", vec![field("title"), field("title")])
                .validate()
                .unwrap_err(),
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
        assert!(nct("post", vec![field("title"), relation])
            .validate()
            .is_ok());
    }

    #[test]
    fn managed_defaults_and_reads() {
        use serde_json::json;
        let mut ct = ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![field("title")],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert!(!ct.managed());
        ct.options = json!({ "managed": true });
        assert!(ct.managed());
    }

    #[test]
    fn draft_publish_defaults_and_reads() {
        use serde_json::json;
        let mut ct = ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![field("title")],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert!(!ct.draft_publish());
        ct.options = json!({ "draft_publish": true });
        assert!(ct.draft_publish());
    }

    #[test]
    fn kind_defaults_collection_on_new() {
        let nct = NewContentType {
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![field("title")],
            options: serde_json::json!({}),
            kind: ContentTypeKind::Collection,
        };
        assert_eq!(nct.kind, ContentTypeKind::Collection);
    }

    #[test]
    fn kind_single_roundtrips_json() {
        let ct = ContentType {
            id: Uuid::nil(),
            name: "homepage".into(),
            display_name: "Homepage".into(),
            fields: vec![],
            options: serde_json::json!({}),
            kind: ContentTypeKind::Single,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&ct).unwrap();
        assert!(json.contains("\"single\""));
        let rt: ContentType = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.kind, ContentTypeKind::Single);
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EnumExtension {
    pub field: String,
    pub append: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PatchContentType {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub add_fields: Vec<Field>,
    #[serde(default)]
    pub drop_fields: Vec<String>,
    #[serde(default)]
    pub extend_enum_values: Vec<EnumExtension>,
    #[serde(default)]
    pub options: Option<serde_json::Value>,
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
    #[error("extend_enum_values references unknown field `{0}`")]
    EnumExtendUnknownField(String),
    #[error("extend_enum_values targets non-enum field `{0}`")]
    EnumExtendNotEnum(String),
    #[error("field `{0}` is both modified via drop/add and extend_enum_values in the same patch")]
    EnumExtendConflictWithAddDrop(String),
}

impl PatchContentType {
    pub fn validate(&self, existing: &ContentType) -> Result<(), PatchError> {
        if self.display_name.is_none()
            && self.add_fields.is_empty()
            && self.drop_fields.is_empty()
            && self.extend_enum_values.is_empty()
            && self.options.is_none()
        {
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
        for f in existing
            .fields
            .iter()
            .filter(|f| !drop_set.contains(f.name.as_str()))
        {
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
        let add_names: std::collections::HashSet<&str> =
            self.add_fields.iter().map(|f| f.name.as_str()).collect();
        for ext in &self.extend_enum_values {
            if drop_set.contains(ext.field.as_str()) || add_names.contains(ext.field.as_str()) {
                return Err(PatchError::EnumExtendConflictWithAddDrop(ext.field.clone()));
            }
            let target = match existing.fields.iter().find(|f| f.name == ext.field) {
                Some(f) => f,
                None => return Err(PatchError::EnumExtendUnknownField(ext.field.clone())),
            };
            if target.kind != crate::field::FieldKind::Enum {
                return Err(PatchError::EnumExtendNotEnum(ext.field.clone()));
            }
            if ext.append.is_empty() {
                return Err(PatchError::BadField {
                    name: ext.field.clone(),
                    source: FieldError::EnumValuesEmpty,
                });
            }
            let existing_meta = match target.enum_meta() {
                Some(m) => m,
                None => return Err(PatchError::EnumExtendNotEnum(ext.field.clone())),
            };
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            for v in &ext.append {
                if !crate::reserved::is_valid_ident(v) {
                    return Err(PatchError::BadField {
                        name: ext.field.clone(),
                        source: FieldError::EnumValueInvalidIdent(v.clone()),
                    });
                }
                if existing_meta.values.iter().any(|x| x == v) || !seen.insert(v.clone()) {
                    return Err(PatchError::BadField {
                        name: ext.field.clone(),
                        source: FieldError::EnumValueDuplicate(v.clone()),
                    });
                }
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
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn noop_rejected() {
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec![],
            extend_enum_values: vec![],
            options: None,
        };
        assert_eq!(p.validate(&existing()).unwrap_err(), PatchError::NoOp);
    }

    #[test]
    fn options_only_patch_is_not_noop() {
        use serde_json::json;
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec![],
            extend_enum_values: vec![],
            options: Some(json!({"draft_publish": true})),
        };
        assert!(p.validate(&existing()).is_ok());
    }

    #[test]
    fn drop_unknown() {
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec!["missing".into()],
            extend_enum_values: vec![],
            options: None,
        };
        assert!(matches!(
            p.validate(&existing()).unwrap_err(),
            PatchError::UnknownDropField(_)
        ));
    }

    #[test]
    fn drop_system() {
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec!["id".into()],
            extend_enum_values: vec![],
            options: None,
        };
        assert!(matches!(
            p.validate(&existing()).unwrap_err(),
            PatchError::DropSystemField(_)
        ));
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
            extend_enum_values: vec![],
            options: None,
        };
        assert!(matches!(
            p.validate(&existing()).unwrap_err(),
            PatchError::DuplicateAddField(_)
        ));
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
            extend_enum_values: vec![],
            options: None,
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
            extend_enum_values: vec![],
            options: None,
        };
        let err = p.validate(&existing_with_author_id).unwrap_err();
        assert_eq!(err, PatchError::ColumnCollision("author_id".into()));
    }

    #[test]
    fn patch_extend_enum_values_ok() {
        let existing = ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "status".into(),
                kind: FieldKind::Enum,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({"values": ["draft", "published"]}),
            }],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec![],
            extend_enum_values: vec![EnumExtension {
                field: "status".into(),
                append: vec!["archived".into()],
            }],
            options: None,
        };
        assert!(p.validate(&existing).is_ok());
    }

    #[test]
    fn patch_extend_enum_values_unknown_field() {
        let existing = ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "status".into(),
                kind: FieldKind::Enum,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({"values": ["draft", "published"]}),
            }],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec![],
            extend_enum_values: vec![EnumExtension {
                field: "missing".into(),
                append: vec!["archived".into()],
            }],
            options: None,
        };
        let err = p.validate(&existing).unwrap_err();
        assert!(format!("{err:?}").contains("EnumExtendUnknownField"));
    }

    #[test]
    fn patch_extend_enum_values_not_enum_field() {
        let existing = ContentType {
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
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec![],
            extend_enum_values: vec![EnumExtension {
                field: "title".into(),
                append: vec!["archived".into()],
            }],
            options: None,
        };
        let err = p.validate(&existing).unwrap_err();
        assert!(format!("{err:?}").contains("EnumExtendNotEnum"));
    }

    #[test]
    fn patch_extend_enum_values_duplicate_against_existing() {
        let existing = ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "status".into(),
                kind: FieldKind::Enum,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({"values": ["draft", "published"]}),
            }],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec![],
            extend_enum_values: vec![EnumExtension {
                field: "status".into(),
                append: vec!["draft".into()],
            }],
            options: None,
        };
        let err = p.validate(&existing).unwrap_err();
        assert!(format!("{err:?}").contains("EnumValueDuplicate"));
    }

    #[test]
    fn patch_extend_enum_values_empty_append() {
        let existing = ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "status".into(),
                kind: FieldKind::Enum,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({"values": ["draft", "published"]}),
            }],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec![],
            extend_enum_values: vec![EnumExtension {
                field: "status".into(),
                append: vec![],
            }],
            options: None,
        };
        let err = p.validate(&existing).unwrap_err();
        assert!(format!("{err:?}").contains("EnumValuesEmpty"));
    }

    #[test]
    fn patch_extend_enum_values_conflict_with_drop() {
        let existing = ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "status".into(),
                kind: FieldKind::Enum,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({"values": ["draft", "published"]}),
            }],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let p = PatchContentType {
            display_name: None,
            add_fields: vec![],
            drop_fields: vec!["status".into()],
            extend_enum_values: vec![EnumExtension {
                field: "status".into(),
                append: vec!["archived".into()],
            }],
            options: None,
        };
        let err = p.validate(&existing).unwrap_err();
        assert!(format!("{err:?}").contains("EnumExtendConflictWithAddDrop"));
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
            extend_enum_values: vec![],
            options: None,
        };
        assert!(p.validate(&existing_with_author_id).is_ok());
    }
}
