//! /admin/content-types/* handlers.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Extension, Json, Router};
use ferrum_core::{
    Actor, AuditEntry, ContentType, Error, Event, NewContentType, PatchContentType, Principal,
    RequestContext,
};
use serde::Deserialize;

/// Rebuild the cached GraphQL schema from the current content-type registry.
/// Called after every content-type mutation. Non-fatal: a rebuild failure logs
/// but does not fail the mutation — the registry is already consistent and the
/// previous GraphQL schema keeps serving until the next successful rebuild.
async fn rebuild_gql(state: &AppState) {
    let types = state.schemas.registry().list().await;
    if let Err(e) = state.gql.rebuild(&types).await {
        tracing::error!(error = %e, "failed to rebuild GraphQL schema after content-type change");
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/content-types", get(list).post(create))
        .route(
            "/admin/content-types/:name",
            get(get_one).delete(delete_one),
        )
        .route("/admin/content-types/:name", patch(patch_one))
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<ContentType>>, ApiError> {
    let cts = state.schemas.registry().list().await;
    let mut out = Vec::with_capacity(cts.len());
    for ct in cts {
        out.push(inject_component_fields(&state, ct).await);
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ContentType>, ApiError> {
    let ct = state
        .schemas
        .registry()
        .get(&name)
        .await
        .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(inject_component_fields(&state, ct).await))
}

async fn create(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Extension(ctx): Extension<RequestContext>,
    Json(payload): Json<NewContentType>,
) -> Result<(StatusCode, Json<ContentType>), ApiError> {
    let ct = state.schemas.create(payload).await?;
    rebuild_gql(&state).await;
    state
        .events
        .emit(Event::SchemaCreated {
            name: ct.name.clone(),
        })
        .await;
    state
        .audit
        .record(
            AuditEntry::new("schema.create", Actor::from_principal(&principal, None))
                .target("settings", ct.name.clone(), ct.name.clone())
                .ctx(ctx),
        )
        .await;
    Ok((StatusCode::CREATED, Json(ct)))
}

async fn patch_one(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Extension(ctx): Extension<RequestContext>,
    Path(name): Path<String>,
    Json(payload): Json<PatchContentType>,
) -> Result<Json<ContentType>, ApiError> {
    if let Some(existing) = state.schemas.registry().get(&name).await {
        if existing.managed() {
            return Err(ApiError(Error::Conflict(format!(
                "content type `{name}` is managed by a schema file; edit the TOML instead"
            ))));
        }
    }
    let ct = state.schemas.patch(&name, payload).await?;
    rebuild_gql(&state).await;
    state
        .events
        .emit(Event::SchemaUpdated {
            name: ct.name.clone(),
        })
        .await;
    state
        .audit
        .record(
            AuditEntry::new("schema.update", Actor::from_principal(&principal, None))
                .target("settings", ct.name.clone(), ct.name.clone())
                .ctx(ctx),
        )
        .await;
    Ok(Json(ct))
}

#[derive(Deserialize)]
struct DeleteQuery {
    confirm: Option<bool>,
}

async fn delete_one(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Extension(ctx): Extension<RequestContext>,
    Path(name): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> Result<StatusCode, ApiError> {
    if q.confirm != Some(true) {
        return Err(ApiError(Error::Validation(
            ferrum_core::ValidationErrors::single("confirm_required: pass ?confirm=true"),
        )));
    }
    if let Some(existing) = state.schemas.registry().get(&name).await {
        if existing.managed() {
            return Err(ApiError(Error::Conflict(format!(
                "content type `{name}` is managed by a schema file; edit the TOML instead"
            ))));
        }
    }
    state.schemas.delete(&name).await?;
    rebuild_gql(&state).await;
    state
        .audit
        .record(
            AuditEntry::new("schema.delete", Actor::from_principal(&principal, None))
                .target("settings", name.clone(), name.clone())
                .ctx(ctx),
        )
        .await;
    state.events.emit(Event::SchemaDeleted { name }).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Inject `_component_fields` into every component-kind field on a ContentType.
async fn inject_component_fields(
    state: &AppState,
    mut ct: ferrum_core::ContentType,
) -> ferrum_core::ContentType {
    use ferrum_core::FieldKind;
    use serde_json::json;

    for f in &mut ct.fields {
        if f.kind != FieldKind::Component {
            continue;
        }
        let Some(meta) = f.component_meta() else {
            continue;
        };
        if let Some(comp) = state.components.get(&meta.component).await {
            let fields_json = serde_json::to_value(&comp.fields).unwrap_or(json!([]));
            if let serde_json::Value::Object(ref mut m) = f.kind_meta {
                m.insert("_component_fields".into(), fields_json);
            } else {
                f.kind_meta = json!({
                    "component": meta.component,
                    "multiple": meta.multiple,
                    "_component_fields": fields_json,
                });
            }
        }
    }
    ct
}
