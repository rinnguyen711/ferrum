//! First-boot seeding: default Article/Author/Category types + sample data.
//! Idempotent — skips entirely if any content type already exists.
// Helpers are wired into startup by the next task; allow until then.
#![allow(dead_code)]

use anyhow::Result;
use rustapi_core::{Field, FieldKind, NewContentType};
use rustapi_schema::SchemaService;
use serde_json::json;

fn field(name: &str, kind: FieldKind, required: bool) -> Field {
    Field {
        name: name.into(),
        kind,
        required,
        unique: false,
        default: serde_json::Value::Null,
        max_length: None,
        kind_meta: json!({}),
    }
}

fn enum_field(name: &str, values: &[&str], required: bool) -> Field {
    Field {
        kind_meta: json!({ "values": values }),
        ..field(name, FieldKind::Enum, required)
    }
}

/// many_to_one relation `name` -> `target`, optionally registering an inverse
/// field `inverse` on the target type.
fn relation_field(name: &str, target: &str, inverse: Option<&str>, required: bool) -> Field {
    let mut meta = json!({ "target": target, "cardinality": "many_to_one" });
    if let Some(inv) = inverse {
        meta["inverse"] = json!(inv);
    }
    Field {
        kind_meta: meta,
        ..field(name, FieldKind::Relation, required)
    }
}

fn author_type() -> NewContentType {
    NewContentType {
        name: "author".into(),
        display_name: "Author".into(),
        fields: vec![
            field("name", FieldKind::String, true),
            field("role", FieldKind::String, false),
            field("bio", FieldKind::Text, false),
        ],
    }
}

fn category_type() -> NewContentType {
    NewContentType {
        name: "category".into(),
        display_name: "Category".into(),
        fields: vec![
            field("name", FieldKind::String, true),
            field("slug", FieldKind::Slug, true),
            field("color", FieldKind::String, false),
            field("description", FieldKind::Text, false),
        ],
    }
}

fn article_type() -> NewContentType {
    NewContentType {
        name: "article".into(),
        display_name: "Article".into(),
        fields: vec![
            field("title", FieldKind::String, true),
            field("slug", FieldKind::Slug, true),
            enum_field("status", &["draft", "review", "published"], true),
            field("excerpt", FieldKind::Text, false),
            field("body", FieldKind::Text, false),
            // inverse "articles" registers the Author<->Article back-reference.
            relation_field("author", "author", Some("articles"), false),
            field("featured", FieldKind::Boolean, false),
            field("read_time", FieldKind::Integer, false),
            field("published_at", FieldKind::Datetime, false),
        ],
    }
}

/// Create the three default types in dependency order. Returns Ok(true) if
/// types were created, Ok(false) if the DB already had content types.
pub async fn seed_types(schemas: &SchemaService) -> Result<bool> {
    if !schemas.registry().list().await.is_empty() {
        return Ok(false);
    }
    for ct in [author_type(), category_type(), article_type()] {
        schemas
            .create(ct)
            .await
            .map_err(|e| anyhow::anyhow!("seed create type failed: {e}"))?;
    }
    Ok(true)
}
