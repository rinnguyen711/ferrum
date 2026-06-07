//! Assembles the full OpenAPI 3.1 document from live AppState.

use crate::openapi::{schema, static_paths};
use crate::state::AppState;
use serde_json::{json, Value};

/// Build the OpenAPI document. Pure assembly over already-loaded registry
/// data plus config; performs the async registry read but no other I/O.
pub async fn build(state: &AppState) -> Value {
    let cfg = &state.config;

    // paths = static block + per-content-type dynamic block.
    let mut paths = static_paths::static_paths();
    let mut components = static_paths::static_components();

    // Borrow both target maps mutably for the merge. Dynamic content-type
    // path keys are always `/api/{name}` / `/api/{name}/{id}`; the `/api/`
    // prefix plus ident validation (names can't contain `/`) means they
    // never collide with the static paths inserted above.
    {
        let paths_map = paths.as_object_mut().expect("static_paths is an object");
        let schemas_map = components["schemas"]
            .as_object_mut()
            .expect("components.schemas is an object");

        for ct in state.schemas.registry().list().await {
            if let Value::Object(ct_paths) = schema::content_type_paths(&ct) {
                for (k, v) in ct_paths {
                    paths_map.insert(k, v);
                }
            }
            if let Value::Object(ct_schemas) = schema::content_type_schemas(&ct) {
                for (k, v) in ct_schemas {
                    schemas_map.insert(k, v);
                }
            }
        }
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "rustapi",
            "version": cfg.api_version,
            "description": "Auto-generated API reference. Dynamic /api/{type} endpoints reflect the live content-type registry."
        },
        "servers": [{ "url": cfg.public_base_url }],
        "paths": paths,
        "components": components
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AlwaysAllow, AppConfig, NoopHook, NoopSink};
    use rustapi_core::{ContentType, Field};
    use rustapi_core::field::FieldKind;
    use rustapi_schema::{SchemaRegistry, SchemaService};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // Builds AppState backed by an in-memory registry only. The DB pool and
    // storage provider are never touched by `build`, so we use lazy/never
    // connections. If `PgPool` cannot be constructed without a live DB in this
    // crate's test context, move this assertion to the integration test in
    // Task 7 and keep only the pure schema/static tests as units.
    #[tokio::test]
    async fn build_includes_dynamic_paths_and_omits_input_system_fields() {
        let registry = SchemaRegistry::new();
        registry
            .insert(ContentType {
                id: uuid::Uuid::nil(),
                name: "article".into(),
                display_name: "Article".into(),
                fields: vec![Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: true,
                    unique: false,
                    default: serde_json::Value::Null,
                    max_length: None,
                    kind_meta: json!({}),
                }],
                options: json!({}),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .await;

        // Construct AppState with a lazily-connected pool (no I/O until used).
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://invalid/never-used")
            .expect("lazy pool");
        let schemas = SchemaService::new(pool.clone(), registry);
        let storage: Arc<RwLock<Arc<dyn rustapi_media::StorageProvider>>> =
            Arc::new(RwLock::new(Arc::new(rustapi_media::LocalProvider::new(
                std::env::temp_dir(),
            ))));
        let state = AppState {
            pool,
            schemas,
            authz: Arc::new(AlwaysAllow),
            events: Arc::new(NoopSink),
            hooks: Arc::new(NoopHook),
            config: AppConfig {
                jwt_secret: "x".repeat(32),
                jwt_ttl_secs: 3600,
                page_size_max: 100,
                docs_enabled: true,
                api_version: "9.9.9".into(),
                public_base_url: "/".into(),
            },
            storage,
            secret_key: None,
        };

        let doc = build(&state).await;
        assert_eq!(doc["openapi"], "3.1.0");
        assert_eq!(doc["info"]["version"], "9.9.9");
        assert!(doc["paths"]["/api/article"]["get"].is_object());
        assert!(doc["paths"]["/auth/login"]["post"].is_object());
        assert!(doc["components"]["schemas"]["Article"].is_object());
        assert!(doc["components"]["schemas"]["ArticleInput"]["properties"]["id"].is_null());
    }
}
