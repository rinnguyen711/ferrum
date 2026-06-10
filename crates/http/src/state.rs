//! Application state and pluggable traits (authz, event sink).

use async_trait::async_trait;
use rustapi_core::{action_to_scope, role_allows, Action, Error, Event, Principal};
use rustapi_media::StorageProvider;
use rustapi_schema::{ComponentService, SchemaService};
use serde_json::{Map, Value};
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
    async fn can(&self, principal: &Principal, action: Action, content_type: &str) -> bool {
        match principal {
            Principal::User { roles, .. } => roles.iter().any(|r| role_allows(r, action)),
            Principal::ApiToken { scopes, .. } => {
                let base = action_to_scope(action); // e.g. "content:read"
                scopes.iter().any(|s| {
                    s == base  // wildcard: content:read grants all types
                        || (!content_type.is_empty()
                            && s == &format!("{}:{}", base, content_type)) // specific: content:read:article
                })
            }
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

/// Which content write a hook is being invoked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOp {
    Create,
    Update,
}

/// Context passed to a `WriteHook`. Borrows live only for the duration of the
/// hook call. `content_type` is the registry name (e.g. `"article"`); the hook
/// dispatches per type itself.
pub struct WriteContext<'a> {
    pub content_type: &'a str,
    pub operation: WriteOp,
    pub principal: &'a Principal,
}

/// Developer extension point around content writes. Wired into `AppState` like
/// `EventSink`; the default `NoopHook` leaves behavior unchanged.
#[async_trait]
pub trait WriteHook: Send + Sync + 'static {
    /// Runs after authz and JSON parse, before schema validation
    /// (`body_to_binds`). May add, remove, or rewrite fields, or return `Err`
    /// to reject the request. The returned body is validated against the
    /// schema by the framework, so injected values must satisfy it.
    async fn before_write(
        &self,
        ctx: &WriteContext<'_>,
        body: Map<String, Value>,
    ) -> Result<Map<String, Value>, Error> {
        let _ = ctx;
        Ok(body)
    }

    /// Runs after the write commits, with the final saved record (after
    /// `row_to_json`, before populate/media-embed). The write is already
    /// durable; returning `Err` surfaces as an error response but does not roll
    /// back. Prefer `Error::Internal` here — a 4xx variant (e.g. `Validation`)
    /// would tell the client the request was rejected even though it persisted.
    /// For fire-and-forget fan-out (webhooks, cache bust) use `EventSink`
    /// instead.
    async fn after_write(
        &self,
        ctx: &WriteContext<'_>,
        record: &Value,
    ) -> Result<(), Error> {
        let _ = (ctx, record);
        Ok(())
    }
}

/// Default no-op hook. Both methods keep their trait defaults.
pub struct NoopHook;

#[async_trait]
impl WriteHook for NoopHook {}

#[derive(Clone)]
pub struct AppConfig {
    /// HS256 signing secret for JWTs.
    pub jwt_secret: String,
    /// Access-token lifetime in seconds.
    pub jwt_ttl_secs: i64,
    pub page_size_max: u32,
    /// When false, `/openapi.json` and `/docs` are not mounted (prod opt-out).
    pub docs_enabled: bool,
    /// Reported as `info.version` in the OpenAPI doc.
    pub api_version: String,
    /// Reported as the single `servers[0].url` in the OpenAPI doc.
    pub public_base_url: String,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub schemas: SchemaService,
    pub components: ComponentService,
    pub authz: Arc<dyn Authz>,
    pub events: Arc<dyn EventSink>,
    pub hooks: Arc<dyn WriteHook>,
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
