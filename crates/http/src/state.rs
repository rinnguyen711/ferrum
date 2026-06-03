//! Application state and pluggable traits (authz, event sink).

use async_trait::async_trait;
use rustapi_core::{role_allows, Action, Event, Principal};
use rustapi_media::StorageProvider;
use rustapi_schema::SchemaService;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait Authz: Send + Sync + 'static {
    async fn can(&self, principal: &Principal, action: Action, content_type: &str) -> bool;
}

pub struct AlwaysAllow;

#[async_trait]
impl Authz for AlwaysAllow {
    async fn can(&self, _p: &Principal, _a: Action, _ct: &str) -> bool {
        true
    }
}

/// Production authorizer: unions the hardcoded permissions of a user's roles.
pub struct RoleAuthz;

#[async_trait]
impl Authz for RoleAuthz {
    async fn can(&self, principal: &Principal, action: Action, _content_type: &str) -> bool {
        match principal {
            Principal::User { roles, .. } => roles.iter().any(|r| role_allows(r, action)),
        }
    }
}

#[async_trait]
pub trait EventSink: Send + Sync + 'static {
    async fn emit(&self, event: Event);
}

pub struct NoopSink;

#[async_trait]
impl EventSink for NoopSink {
    async fn emit(&self, _event: Event) {}
}

#[derive(Clone)]
pub struct AppConfig {
    /// HS256 signing secret for JWTs.
    pub jwt_secret: String,
    /// Access-token lifetime in seconds.
    pub jwt_ttl_secs: i64,
    pub page_size_max: u32,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub schemas: SchemaService,
    pub authz: Arc<dyn Authz>,
    pub events: Arc<dyn EventSink>,
    pub config: AppConfig,
    /// Active media storage provider, hot-swappable when settings change.
    pub storage: Arc<RwLock<Arc<dyn StorageProvider>>>,
    /// 32-byte key for encrypting secret provider-config fields. `None`
    /// disables saving providers that declare secret fields.
    pub secret_key: Option<[u8; 32]>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustapi_core::Principal;
    use uuid::Uuid;

    fn user(roles: &[&str]) -> Principal {
        Principal::User {
            id: Uuid::nil(),
            email: "a@b.c".into(),
            roles: roles.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn role_authz_admin_can_write_schema() {
        let az = RoleAuthz;
        assert!(az.can(&user(&["admin"]), Action::SchemaWrite, "x").await);
    }

    #[tokio::test]
    async fn role_authz_viewer_cannot_write() {
        let az = RoleAuthz;
        assert!(!az.can(&user(&["viewer"]), Action::ContentWrite, "x").await);
        assert!(az.can(&user(&["viewer"]), Action::ContentRead, "x").await);
    }

    #[tokio::test]
    async fn role_authz_union_of_roles() {
        let az = RoleAuthz;
        // editor + viewer → still no schema write
        assert!(!az.can(&user(&["editor", "viewer"]), Action::SchemaWrite, "x").await);
        assert!(az.can(&user(&["editor", "viewer"]), Action::ContentWrite, "x").await);
    }
}
