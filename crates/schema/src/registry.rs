//! In-memory cache of all content types. The HTTP layer reads from here on
//! every request; only the SchemaService mutates it.

use rustapi_core::Cardinality;
use rustapi_core::ContentType;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Default)]
pub struct SchemaRegistry {
    inner: Arc<RwLock<HashMap<String, ContentType>>>,
}

impl SchemaRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(&self, name: &str) -> Option<ContentType> {
        self.inner.read().await.get(name).cloned()
    }

    pub async fn list(&self) -> Vec<ContentType> {
        let mut out: Vec<_> = self.inner.read().await.values().cloned().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub async fn insert(&self, ct: ContentType) {
        self.inner.write().await.insert(ct.name.clone(), ct);
    }

    pub async fn remove(&self, name: &str) {
        self.inner.write().await.remove(name);
    }

    /// Used at boot and (eventually) on LISTEN/NOTIFY in phase 7.
    pub async fn reload_from_db(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        let rows = sqlx::query_as::<_, RawCt>(
            "SELECT id, name, display_name, fields, options, kind, created_at, updated_at FROM _content_types",
        )
        .fetch_all(pool)
        .await?;
        let mut map = HashMap::with_capacity(rows.len());
        for r in rows {
            let ct = r.into_content_type();
            map.insert(ct.name.clone(), ct);
        }
        *self.inner.write().await = map;
        Ok(())
    }

    /// Many-to-many fields *owned by* `owner` → `(field_name, target)` pairs.
    /// Used by SchemaService to create/drop join tables for a type.
    pub async fn m2m_targets(&self, owner: &str) -> Vec<(String, String)> {
        let map = self.inner.read().await;
        let mut out = Vec::new();
        if let Some(ct) = map.get(owner) {
            for f in &ct.fields {
                let Some(meta) = f
                    .relation_meta()
                    .filter(|m| m.cardinality == Cardinality::ManyToMany)
                else {
                    continue;
                };
                out.push((f.name.clone(), meta.target));
            }
        }
        out
    }

    /// Many-to-many fields *targeting* `target` → `(owner_type, field_name)`.
    /// Used to drop dependent join tables before dropping a target type.
    pub async fn m2m_referencing(&self, target: &str) -> Vec<(String, String)> {
        let map = self.inner.read().await;
        let mut out = Vec::new();
        for ct in map.values() {
            for f in &ct.fields {
                let Some(meta) = f
                    .relation_meta()
                    .filter(|m| m.cardinality == Cardinality::ManyToMany)
                else {
                    continue;
                };
                if meta.target == target {
                    out.push((ct.name.clone(), f.name.clone()));
                }
            }
        }
        out
    }

    /// Resolve an inverse populate name against many-to-many relations. Returns
    /// `(owner_type, field_name)` where `owner.field` is the M:N relation whose
    /// `inverse` matches and whose `target` is `target_name`.
    pub async fn inverse_lookup_m2m(
        &self,
        target_name: &str,
        inverse_name: &str,
    ) -> Option<(String, String)> {
        let map = self.inner.read().await;
        for ct in map.values() {
            for f in &ct.fields {
                let Some(meta) = f
                    .relation_meta()
                    .filter(|m| m.cardinality == Cardinality::ManyToMany)
                else {
                    continue;
                };
                if meta.target == target_name && meta.inverse.as_deref() == Some(inverse_name) {
                    return Some((ct.name.clone(), f.name.clone()));
                }
            }
        }
        None
    }

    /// Walks all registered content types looking for a relation field on any
    /// source type whose `target` matches `target_name` and whose `inverse`
    /// matches `inverse_name`. Returns `(source_type_name, fk_column)` if found.
    /// O(types × fields) — small enough for v1 scale.
    pub async fn inverse_lookup(
        &self,
        target_name: &str,
        inverse_name: &str,
    ) -> Option<(String, String)> {
        let map = self.inner.read().await;
        for ct in map.values() {
            for f in &ct.fields {
                let Some(meta) = f.relation_meta() else { continue };
                if meta.target == target_name && meta.inverse.as_deref() == Some(inverse_name) {
                    return Some((ct.name.clone(), f.physical_column()));
                }
            }
        }
        None
    }
}

#[derive(sqlx::FromRow)]
struct RawCt {
    id: uuid::Uuid,
    name: String,
    display_name: String,
    fields: sqlx::types::Json<Vec<rustapi_core::Field>>,
    options: sqlx::types::Json<serde_json::Value>,
    kind: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl RawCt {
    fn into_content_type(self) -> ContentType {
        let kind = match self.kind.as_str() {
            "collection" => rustapi_core::ContentTypeKind::Collection,
            "single" => rustapi_core::ContentTypeKind::Single,
            other => panic!("unknown content_type kind in database: {other:?}"),
        };
        ContentType {
            id: self.id,
            name: self.name,
            display_name: self.display_name,
            fields: self.fields.0,
            options: self.options.0,
            kind,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::{ContentTypeKind, Field, FieldKind};
    use serde_json::json;
    use uuid::Uuid;

    fn ct(name: &str) -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: name.into(),
            display_name: "X".into(),
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

    #[tokio::test]
    async fn insert_get_remove() {
        let r = SchemaRegistry::new();
        r.insert(ct("post")).await;
        assert_eq!(r.get("post").await.unwrap().name, "post");
        r.remove("post").await;
        assert!(r.get("post").await.is_none());
    }

    #[tokio::test]
    async fn list_sorted_by_name() {
        let r = SchemaRegistry::new();
        r.insert(ct("z")).await;
        r.insert(ct("a")).await;
        let names: Vec<_> = r.list().await.into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["a", "z"]);
    }

    #[tokio::test]
    async fn inverse_lookup_finds_registered_pair() {
        use rustapi_core::ContentType;

        let reg = SchemaRegistry::new();
        let user = ContentType {
            id: Uuid::new_v4(),
            name: "user".into(),
            display_name: "User".into(),
            fields: vec![],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let post = ContentType {
            id: Uuid::new_v4(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "author".into(),
                kind: FieldKind::Relation,
                required: false,
                unique: false,
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: json!({
                    "target": "user",
                    "cardinality": "many_to_one",
                    "inverse": "posts"
                }),
            }],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        reg.insert(user).await;
        reg.insert(post).await;

        let hit = reg.inverse_lookup("user", "posts").await;
        assert_eq!(
            hit.as_ref().map(|(s, c)| (s.as_str(), c.as_str())),
            Some(("post", "author_id"))
        );

        assert!(reg.inverse_lookup("user", "nope").await.is_none());
        assert!(reg.inverse_lookup("nope", "posts").await.is_none());
    }

    #[tokio::test]
    async fn inverse_lookup_ignores_relation_without_inverse() {
        use rustapi_core::ContentType;

        let reg = SchemaRegistry::new();
        reg.insert(ContentType {
            id: Uuid::new_v4(),
            name: "user".into(),
            display_name: "User".into(),
            fields: vec![],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await;
        reg.insert(ContentType {
            id: Uuid::new_v4(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "author".into(),
                kind: FieldKind::Relation,
                required: false,
                unique: false,
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
            }],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await;
        // Relation has no inverse declared — lookup against "posts" finds nothing.
        assert!(reg.inverse_lookup("user", "posts").await.is_none());
    }

    fn m2m_field(name: &str, target: &str, inverse: Option<&str>) -> Field {
        let mut meta = serde_json::Map::new();
        meta.insert("target".into(), json!(target));
        meta.insert("cardinality".into(), json!("many_to_many"));
        if let Some(i) = inverse {
            meta.insert("inverse".into(), json!(i));
        }
        Field {
            name: name.into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::Value::Object(meta),
        }
    }

    #[tokio::test]
    async fn m2m_targets_lists_owned_join_tables() {
        let reg = SchemaRegistry::new();
        let mut post = ct("post");
        post.fields.push(m2m_field("tags", "tag", None));
        reg.insert(post).await;
        let hits = reg.m2m_targets("post").await;
        assert_eq!(hits, vec![("tags".to_string(), "tag".to_string())]);
    }

    #[tokio::test]
    async fn m2m_referencing_finds_join_tables_pointing_at_type() {
        let reg = SchemaRegistry::new();
        let mut post = ct("post");
        post.fields.push(m2m_field("tags", "tag", None));
        reg.insert(post).await;
        let hits = reg.m2m_referencing("tag").await;
        assert_eq!(hits, vec![("post".to_string(), "tags".to_string())]);
    }

    #[tokio::test]
    async fn inverse_lookup_resolves_m2m() {
        let reg = SchemaRegistry::new();
        reg.insert(ct("tag")).await;
        let mut post = ct("post");
        post.fields.push(m2m_field("tags", "tag", Some("posts")));
        reg.insert(post).await;
        let hit = reg.inverse_lookup_m2m("tag", "posts").await;
        assert_eq!(hit, Some(("post".to_string(), "tags".to_string())));
    }

    #[tokio::test]
    async fn m2m_targets_unknown_type_is_empty() {
        let reg = SchemaRegistry::new();
        assert!(reg.m2m_targets("nope").await.is_empty());
    }

    #[tokio::test]
    async fn m2m_referencing_ignores_many_to_one() {
        let reg = SchemaRegistry::new();
        reg.insert(ct("user")).await;
        let mut post = ct("post");
        // many_to_one author → user, NOT m2m: must not appear in m2m_referencing.
        post.fields.push(Field {
            name: "author".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"target":"user","cardinality":"many_to_one"}),
        });
        reg.insert(post).await;
        assert!(reg.m2m_referencing("user").await.is_empty());
    }

    #[tokio::test]
    async fn inverse_lookup_m2m_wrong_name_is_none() {
        let reg = SchemaRegistry::new();
        let mut post = ct("post");
        post.fields.push(m2m_field("tags", "tag", Some("posts")));
        reg.insert(post).await;
        assert!(reg.inverse_lookup_m2m("tag", "wrong").await.is_none());
    }

    #[tokio::test]
    async fn inverse_lookup_skips_primitive_fields() {
        use rustapi_core::ContentType;

        let reg = SchemaRegistry::new();
        reg.insert(ContentType {
            id: Uuid::new_v4(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "title".into(),
                kind: FieldKind::String,
                required: false,
                unique: false,
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: json!({}),
            }],
            options: json!({}),
            kind: ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await;
        assert!(reg.inverse_lookup("user", "anything").await.is_none());
    }
}
