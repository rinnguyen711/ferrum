//! /admin/components/* handlers.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rustapi_core::{Error, Field};
use rustapi_sql::Component;
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/components", get(list).post(create))
        .route("/admin/components/:uid", get(get_one).put(update_one).delete(delete_one))
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<Component>>, ApiError> {
    Ok(Json(state.components.list().await))
}

async fn get_one(
    State(state): State<AppState>,
    Path(uid): Path<String>,
) -> Result<Json<Component>, ApiError> {
    state
        .components
        .get(&uid)
        .await
        .map(Json)
        .ok_or(ApiError(Error::NotFound))
}

#[derive(Debug, Deserialize)]
struct ComponentPayload {
    uid: String,
    display_name: String,
    #[serde(default)]
    fields: Vec<Field>,
}

#[derive(Debug, Deserialize)]
struct UpdatePayload {
    display_name: String,
    #[serde(default)]
    fields: Vec<Field>,
}

async fn create(
    State(state): State<AppState>,
    Json(payload): Json<ComponentPayload>,
) -> Result<(StatusCode, Json<Component>), ApiError> {
    let c = state.components.create(&payload.uid, &payload.display_name, payload.fields).await?;
    Ok((StatusCode::CREATED, Json(c)))
}

async fn update_one(
    State(state): State<AppState>,
    Path(uid): Path<String>,
    Json(payload): Json<UpdatePayload>,
) -> Result<Json<Component>, ApiError> {
    let c = state.components.update(&uid, &payload.display_name, payload.fields).await?;
    Ok(Json(c))
}

#[derive(Deserialize)]
struct DeleteQuery {
    confirm: Option<bool>,
}

async fn delete_one(
    State(state): State<AppState>,
    Path(uid): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> Result<StatusCode, ApiError> {
    if q.confirm != Some(true) {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single("confirm_required: pass ?confirm=true"),
        )));
    }
    let referencing: Vec<String> = state
        .schemas
        .registry()
        .list()
        .await
        .into_iter()
        .filter(|ct| {
            ct.fields.iter().any(|f| {
                f.component_meta()
                    .map(|m| m.component == uid)
                    .unwrap_or(false)
            })
        })
        .map(|ct| ct.name)
        .collect();
    state.components.delete(&uid, &referencing).await?;
    Ok(StatusCode::NO_CONTENT)
}
