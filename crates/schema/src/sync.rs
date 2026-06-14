//! Declarative schema sync: load content types from TOML file(s) and reconcile
//! the database to match on startup. See
//! docs/superpowers/specs/2026-06-14-schema-as-code-toml-sync-design.md.

// ContentType import added when diff/apply logic lands in a later task.
use rustapi_core::{Error, Field, NewContentType, ValidationErrors};
use serde::Deserialize;

/// How aggressively sync reconciles the DB toward the TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    /// Create missing types, add missing fields. Never drop. (default)
    #[default]
    Additive,
    /// Also drop types/fields absent from the TOML.
    Full,
}

impl SyncMode {
    /// Parse from the `RUSTAPI_SCHEMA_SYNC` env value. Unknown/empty → Additive.
    pub fn from_env_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "full" => SyncMode::Full,
            _ => SyncMode::Additive,
        }
    }
}

/// One TOML file's worth of content types.
// consumed by load_desired in a later task
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SchemaFile {
    #[serde(default, rename = "content_type")]
    content_types: Vec<TomlContentType>,
}

/// A content type as declared in TOML. Maps onto `NewContentType`; `field` is
/// renamed so the TOML key is `[[content_type.field]]`.
// consumed by parse_toml/plan_sync in a later task
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TomlContentType {
    name: String,
    display_name: String,
    #[serde(default)]
    kind: rustapi_core::ContentTypeKind,
    #[serde(default)]
    options: serde_json::Value,
    #[serde(default, rename = "field")]
    fields: Vec<Field>,
}

// consumed by parse_toml in a later task
#[allow(dead_code)]
impl From<TomlContentType> for NewContentType {
    fn from(t: TomlContentType) -> Self {
        NewContentType {
            name: t.name,
            display_name: t.display_name,
            fields: t.fields,
            options: t.options,
            kind: t.kind,
        }
    }
}

/// Parse a single TOML document into content types.
// consumed by load_desired/plan_sync in a later task
#[allow(dead_code)]
pub(crate) fn parse_toml(doc: &str) -> Result<Vec<NewContentType>, Error> {
    let parsed: SchemaFile = toml::from_str(doc)
        .map_err(|e| Error::Validation(ValidationErrors::single(format!("schema TOML parse: {e}"))))?;
    Ok(parsed.content_types.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustapi_core::FieldKind;

    #[test]
    fn parses_type_with_fields() {
        let doc = r#"
[[content_type]]
name = "post"
display_name = "Post"
kind = "collection"
options = { draft_publish = true }

  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true
"#;
        let cts = parse_toml(doc).expect("parse");
        assert_eq!(cts.len(), 1);
        assert_eq!(cts[0].name, "post");
        assert_eq!(cts[0].fields.len(), 1);
        assert_eq!(cts[0].fields[0].name, "title");
        assert_eq!(cts[0].fields[0].kind, FieldKind::String);
        assert!(cts[0].fields[0].required);
    }

    #[test]
    fn sync_mode_from_env() {
        assert_eq!(SyncMode::from_env_str("full"), SyncMode::Full);
        assert_eq!(SyncMode::from_env_str("FULL"), SyncMode::Full);
        assert_eq!(SyncMode::from_env_str("additive"), SyncMode::Additive);
        assert_eq!(SyncMode::from_env_str("garbage"), SyncMode::Additive);
    }
}
