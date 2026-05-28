//! Application state and pluggable traits (authz, event sink).

use async_trait::async_trait;
use rustapi_core::{Action, Event, Principal};
use rustapi_schema::SchemaService;
use sqlx::PgPool;
use std::sync::Arc;

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
    pub admin_key: String,
    pub page_size_max: u32,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub schemas: SchemaService,
    pub authz: Arc<dyn Authz>,
    pub events: Arc<dyn EventSink>,
    pub config: AppConfig,
}
