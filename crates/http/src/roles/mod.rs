//! In-memory cache of roles and their permissions. Hydrated at boot and after
//! every roles CRUD mutation; `RoleAuthz` reads from here so authorization never
//! hits the DB per request. Mirrors `rustapi_schema::SchemaRegistry`.

use rustapi_sql::{list_roles, load_all, RolePermission};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Default)]
struct Inner {
    /// role key → set of (content_type, verb) grants.
    perms: HashMap<String, HashSet<(String, String)>>,
    /// role keys that are system roles (locked, code-enforced).
    system: HashSet<String>,
}

#[derive(Clone, Default)]
pub struct RoleRegistry {
    inner: Arc<RwLock<Inner>>,
}

impl RoleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// True if the role is one of the locked system roles.
    pub async fn is_system(&self, key: &str) -> bool {
        self.inner.read().await.system.contains(key)
    }

    /// True if the role grants `verb` on `content_type`.
    pub async fn grants(&self, role: &str, content_type: &str, verb: &str) -> bool {
        let g = self.inner.read().await;
        g.perms
            .get(role)
            .map(|set| set.contains(&(content_type.to_string(), verb.to_string())))
            .unwrap_or(false)
    }

    pub async fn reload_from_db(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        let roles = list_roles(pool).await?;
        let all = load_all(pool).await?;
        let mut perms: HashMap<String, HashSet<(String, String)>> = HashMap::new();
        for (key, list) in all {
            perms.insert(
                key,
                list.into_iter()
                    .map(|RolePermission { content_type, action }| (content_type, action))
                    .collect(),
            );
        }
        let system = roles
            .iter()
            .filter(|r| r.is_system)
            .map(|r| r.key.clone())
            .collect();
        let mut g = self.inner.write().await;
        g.perms = perms;
        g.system = system;
        Ok(())
    }
}

#[cfg(test)]
impl RoleRegistry {
    /// Test-only: seed the cache directly without a DB.
    pub fn seeded(
        perms: std::collections::HashMap<String, std::collections::HashSet<(String, String)>>,
        system: std::collections::HashSet<String>,
    ) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::RwLock::new(Inner { perms, system })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_registry_grants_nothing() {
        let reg = RoleRegistry::new();
        assert!(!reg.grants("author", "article", "find").await);
        assert!(!reg.is_system("author").await);
    }

    #[tokio::test]
    async fn seeded_registry_reports_grants_and_system() {
        let mut perms: HashMap<String, HashSet<(String, String)>> = HashMap::new();
        perms.insert(
            "author".into(),
            HashSet::from([("article".into(), "find".into())]),
        );
        let reg = RoleRegistry::seeded(perms, HashSet::from(["admin".into()]));
        assert!(reg.grants("author", "article", "find").await);
        assert!(!reg.grants("author", "article", "delete").await);
        assert!(reg.is_system("admin").await);
        assert!(!reg.is_system("author").await);
    }
}
