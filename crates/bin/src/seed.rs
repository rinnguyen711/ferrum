//! First-boot seeding: default Article/Author/Category types + sample data.
//! Idempotent — skips entirely if any content type already exists.

use anyhow::Result;
use rustapi_core::{ContentType, Field, FieldKind, NewContentType};
use rustapi_http::entry::body_to_binds;
use rustapi_schema::bind::bind_all;
use rustapi_schema::SchemaService;
use serde_json::{json, Map, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

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
        options: serde_json::Value::Null,
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
        options: serde_json::Value::Null,
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
        options: serde_json::Value::Null,
    }
}

/// Insert one row for `ct` from a JSON object, returning the new row id.
async fn insert_entry(pool: &PgPool, ct: &ContentType, body: Map<String, Value>) -> Result<Uuid> {
    // Seed types use only many_to_one relations, so the m2m link plan is
    // always empty here.
    let (binds, _checks, _links, _mc, _ml) = body_to_binds(ct, body, true)
        .map_err(|e| anyhow::anyhow!("seed body_to_binds: {e}"))?;
    let (sql, bind_vals) = rustapi_sql::insert(ct, &binds)
        .map_err(|e| anyhow::anyhow!("seed insert sql: {e}"))?;
    let row = bind_all(sqlx::query(&sql), &bind_vals)
        .fetch_one(pool)
        .await?;
    let id: Uuid = row.try_get("id")?;
    Ok(id)
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

/// Insert sample authors, categories, and articles (articles link a real
/// author). Assumes the three types were just created.
pub async fn seed_rows(pool: &PgPool, schemas: &SchemaService) -> Result<()> {
    let author_ct = schemas.registry().get("author").await
        .ok_or_else(|| anyhow::anyhow!("author type missing during seed"))?;
    let category_ct = schemas.registry().get("category").await
        .ok_or_else(|| anyhow::anyhow!("category type missing during seed"))?;
    let article_ct = schemas.registry().get("article").await
        .ok_or_else(|| anyhow::anyhow!("article type missing during seed"))?;

    // --- authors --- (name, role, bio) -> capture id by name
    let authors = [
        ("Mara Velez", "Editor in chief", "Runs the desk. Twelve years in long-form science journalism."),
        ("Idris Bello", "Staff writer", "Covers climate, energy, and the people in between."),
        ("Saoirse Lynch", "Contributor", "Essayist. Writes about cities, memory, and maps."),
        ("Tomas Reier", "Photo editor", "Pictures first, words later."),
    ];
    let mut author_id = std::collections::HashMap::new();
    for (name, role, bio) in authors {
        let mut b = Map::new();
        b.insert("name".into(), json!(name));
        b.insert("role".into(), json!(role));
        b.insert("bio".into(), json!(bio));
        let id = insert_entry(pool, &author_ct, b).await?;
        author_id.insert(name, id);
    }

    // --- categories --- (name, slug, color, description)
    let categories = [
        ("Science", "science", "#0E7490", "Research, discovery, and the scientific method."),
        ("Climate", "climate", "#15803D", "Energy, environment, and a changing planet."),
        ("Culture", "culture", "#7C3AED", "Arts, ideas, and how we live."),
        ("Cities", "cities", "#C2410C", "Urban life and the built environment."),
        ("Interviews", "interviews", "#B45309", "Long-form conversations."),
    ];
    for (name, slug, color, description) in categories {
        let mut b = Map::new();
        b.insert("name".into(), json!(name));
        b.insert("slug".into(), json!(slug));
        b.insert("color".into(), json!(color));
        b.insert("description".into(), json!(description));
        insert_entry(pool, &category_ct, b).await?;
    }

    // --- articles --- (title, slug, status, excerpt, author-name, featured, read_time, published_at)
    let articles = [
        ("The quiet reinvention of the tidal turbine", "tidal-turbine-reinvention", "published", "A new generation of low-speed rotors is making estuary power viable for the first time.", "Idris Bello", true, 9, Some("2026-05-28T09:00:00Z")),
        ("What a city remembers when its river is gone", "city-remembers-river", "published", "Walking the buried waterways of four cities that paved over their founding streams.", "Saoirse Lynch", false, 14, Some("2026-05-27T07:30:00Z")),
        ("The lab growing coral in the dark", "coral-in-the-dark", "draft", "Inside a basement aquarium where bleaching has been reversed — for now.", "Idris Bello", false, 7, None),
        ("Forty years of the same weather diary", "weather-diary-forty-years", "published", "A retired postmaster recorded the sky every morning. The data turned out to matter.", "Mara Velez", false, 11, Some("2026-05-24T06:00:00Z")),
        ("The mapmakers who refuse to draw borders", "mapmakers-no-borders", "review", "A small cartography collective is redrawing the world without nation-states.", "Saoirse Lynch", false, 8, None),
        ("Why your bread tastes different at altitude", "bread-at-altitude", "published", "Pressure, yeast, and the chemistry of a mountain-town bakery.", "Mara Velez", false, 5, Some("2026-05-21T08:00:00Z")),
        ("An interview with the last lighthouse keeper", "last-lighthouse-keeper", "draft", "Forty-one years on a rock in the North Atlantic, in his own words.", "Mara Velez", false, 16, None),
        ("The return of the night train", "return-of-night-train", "published", "Europe rebuilt its sleeper network. We rode it for a week to see if it works.", "Idris Bello", true, 10, Some("2026-05-19T07:00:00Z")),
        ("A field guide to urban lichen", "urban-lichen-field-guide", "review", "The pollution map hiding in plain sight on every old stone wall.", "Tomas Reier", false, 6, None),
        ("The economics of a free public sauna", "free-public-sauna", "published", "One northern city bet that warmth should be a commons. The numbers are surprising.", "Saoirse Lynch", false, 12, Some("2026-05-17T08:30:00Z")),
    ];
    for (title, slug, status, excerpt, author_name, featured, read_time, published_at) in articles {
        let mut b = Map::new();
        b.insert("title".into(), json!(title));
        b.insert("slug".into(), json!(slug));
        b.insert("status".into(), json!(status));
        b.insert("excerpt".into(), json!(excerpt));
        b.insert("featured".into(), json!(featured));
        b.insert("read_time".into(), json!(read_time));
        if let Some(pa) = published_at {
            b.insert("published_at".into(), json!(pa));
        }
        if let Some(aid) = author_id.get(author_name) {
            b.insert("author".into(), json!(aid.to_string()));
        }
        insert_entry(pool, &article_ct, b).await?;
    }

    Ok(())
}

/// Top-level entry point: seed types + rows when the DB is empty and seeding
/// is enabled. Non-fatal on row errors — logs and continues so the server boots.
pub async fn seed_if_empty(pool: &PgPool, schemas: &SchemaService, enabled: bool) -> Result<()> {
    if !enabled {
        return Ok(());
    }
    match seed_types(schemas).await {
        Ok(false) => {
            tracing::debug!("seed: content types already present, skipping");
            return Ok(());
        }
        Ok(true) => {}
        Err(e) => {
            tracing::warn!(error = %e, "seed: type creation failed, skipping data seed");
            return Ok(());
        }
    }
    if let Err(e) = seed_rows(pool, schemas).await {
        tracing::warn!(error = %e, "seed: sample data insert failed (types still created)");
        return Ok(());
    }
    tracing::info!("seed: created default types (author, category, article) + sample data");
    Ok(())
}
