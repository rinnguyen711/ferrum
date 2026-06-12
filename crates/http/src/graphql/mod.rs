pub mod build;
pub mod handler;
pub mod resolve;
pub mod scalars;

use async_graphql::dynamic::{Schema, SchemaError};
use rustapi_core::ContentType;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Cached dynamic GraphQL schema, rebuilt on content-type CRUD. Mirrors
/// RoleRegistry. The cached Schema has NO data baked in — AppState and
/// Principal are injected per request at execute time (see graphql::handler),
/// because AppState owns this GqlRegistry (a baked-in clone would be cyclic /
/// go stale). Resolvers read them via ctx.data::<AppState>() etc.
#[derive(Clone, Default)]
pub struct GqlRegistry {
    inner: Arc<RwLock<Option<Schema>>>,
}

impl GqlRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild the schema from the given content types. Call at boot and after
    /// any content-type create/patch/delete.
    pub async fn rebuild(&self, types: &[ContentType]) -> Result<(), SchemaError> {
        let schema = build::build_schema(types)?;
        *self.inner.write().await = Some(schema);
        Ok(())
    }

    /// Current schema clone for execution. None if not yet built.
    pub async fn current(&self) -> Option<Schema> {
        self.inner.read().await.clone()
    }
}
