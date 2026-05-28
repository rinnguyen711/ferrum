//! In-memory cache of all content types. The HTTP layer reads from here on
//! every request; only the SchemaService mutates it.

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
            "SELECT id, name, display_name, fields, created_at, updated_at FROM _content_types",
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
}

#[derive(sqlx::FromRow)]
struct RawCt {
    id: uuid::Uuid,
    name: String,
    display_name: String,
    fields: sqlx::types::Json<Vec<rustapi_core::Field>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl RawCt {
    fn into_content_type(self) -> ContentType {
        ContentType {
            id: self.id,
            name: self.name,
            display_name: self.display_name,
            fields: self.fields.0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::{Field, FieldKind};
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
}
