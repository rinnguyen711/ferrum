//! In-memory component registry + transactional service.

use ferrum_core::{Error, Field, FieldKind, ValidationErrors};
use ferrum_sql::{Component, ComponentStore};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Allowed inner field kinds for component definitions.
const ALLOWED_INNER_KINDS: &[FieldKind] = &[
    FieldKind::String,
    FieldKind::Text,
    FieldKind::Integer,
    FieldKind::Float,
    FieldKind::Boolean,
    FieldKind::Datetime,
    FieldKind::Email,
    FieldKind::Url,
    FieldKind::Slug,
    FieldKind::Enum,
    FieldKind::Json,
    FieldKind::RichText,
    FieldKind::Media,
];

/// In-memory cache of all components. Keyed by uid.
#[derive(Clone, Default)]
pub struct ComponentRegistry {
    inner: Arc<RwLock<HashMap<String, Component>>>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(&self, uid: &str) -> Option<Component> {
        self.inner.read().await.get(uid).cloned()
    }

    pub async fn list(&self) -> Vec<Component> {
        let mut out: Vec<_> = self.inner.read().await.values().cloned().collect();
        out.sort_by(|a, b| a.uid.cmp(&b.uid));
        out
    }

    pub async fn insert(&self, c: Component) {
        self.inner.write().await.insert(c.uid.clone(), c);
    }

    pub async fn remove(&self, uid: &str) {
        self.inner.write().await.remove(uid);
    }

    pub async fn reload_from_db(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        let store = ComponentStore::new(pool.clone());
        let all = store.list().await?;
        let mut map = HashMap::with_capacity(all.len());
        for c in all {
            map.insert(c.uid.clone(), c);
        }
        *self.inner.write().await = map;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ComponentService {
    store: ComponentStore,
    registry: ComponentRegistry,
}

impl ComponentService {
    pub fn new(pool: PgPool, registry: ComponentRegistry) -> Self {
        Self {
            store: ComponentStore::new(pool),
            registry,
        }
    }

    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }

    pub async fn list(&self) -> Vec<Component> {
        self.registry.list().await
    }

    pub async fn get(&self, uid: &str) -> Option<Component> {
        self.registry.get(uid).await
    }

    pub async fn create(
        &self,
        uid: &str,
        display_name: &str,
        fields: Vec<Field>,
        managed: bool,
    ) -> Result<Component, Error> {
        validate_uid(uid)?;
        validate_inner_fields(&fields)?;
        for f in &fields {
            f.validate()
                .map_err(|e| Error::Validation(ValidationErrors::field(&f.name, e.to_string())))?;
        }
        if self.registry.get(uid).await.is_some() {
            return Err(Error::Conflict(format!("component `{uid}` already exists")));
        }
        let c = self
            .store
            .create(uid, display_name, &fields, managed)
            .await
            .map_err(internal)?;
        self.registry.insert(c.clone()).await;
        Ok(c)
    }

    pub async fn update(
        &self,
        uid: &str,
        display_name: &str,
        fields: Vec<Field>,
        managed: bool,
    ) -> Result<Component, Error> {
        validate_inner_fields(&fields)?;
        for f in &fields {
            f.validate()
                .map_err(|e| Error::Validation(ValidationErrors::field(&f.name, e.to_string())))?;
        }
        let c = self
            .store
            .update(uid, display_name, &fields, managed)
            .await
            .map_err(internal)?
            .ok_or(Error::NotFound)?;
        self.registry.insert(c.clone()).await;
        Ok(c)
    }

    pub async fn delete(&self, uid: &str, referencing_types: &[String]) -> Result<(), Error> {
        if !referencing_types.is_empty() {
            return Err(Error::Conflict(format!(
                "component `{}` is referenced by: {}",
                uid,
                referencing_types.join(", ")
            )));
        }
        let deleted = self.store.delete(uid).await.map_err(internal)?;
        if !deleted {
            return Err(Error::NotFound);
        }
        self.registry.remove(uid).await;
        Ok(())
    }
}

/// uid must match `category.name` — two dot-separated lowercase ident segments.
fn validate_uid(uid: &str) -> Result<(), Error> {
    let parts: Vec<&str> = uid.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(Error::Validation(ValidationErrors::field(
            "uid",
            "uid must be two dot-separated segments, e.g. \"shared.hero_block\"",
        )));
    }
    for p in &parts {
        if !ferrum_core::reserved::is_valid_ident(p) {
            return Err(Error::Validation(ValidationErrors::field(
                "uid",
                format!("uid segment `{p}` is not a valid identifier (^[a-z][a-z0-9_]{{0,62}}$)"),
            )));
        }
    }
    Ok(())
}

fn validate_inner_fields(fields: &[Field]) -> Result<(), Error> {
    for f in fields {
        if !ALLOWED_INNER_KINDS.contains(&f.kind) {
            return Err(Error::Validation(ValidationErrors::field(
                &f.name,
                format!(
                    "field kind `{:?}` is not allowed inside a component; use scalar or media kinds",
                    f.kind
                ),
            )));
        }
    }
    Ok(())
}

fn internal(e: sqlx::Error) -> Error {
    Error::Internal(anyhow::anyhow!(e))
}
