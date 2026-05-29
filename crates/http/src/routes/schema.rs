//! /admin/content-types/* handlers.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use rustapi_core::{ContentType, Error, Event, NewContentType, PatchContentType};
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/content-types", get(list).post(create))
        .route("/admin/content-types/:name", get(get_one).delete(delete_one))
        .route("/admin/content-types/:name", patch(patch_one))
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<ContentType>>, ApiError> {
    Ok(Json(state.schemas.registry().list().await))
}

async fn get_one(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ContentType>, ApiError> {
    state
        .schemas
        .registry()
        .get(&name)
        .await
        .map(Json)
        .ok_or(ApiError(Error::NotFound))
}

async fn create(
    State(state): State<AppState>,
    Json(payload): Json<NewContentType>,
) -> Result<(StatusCode, Json<ContentType>), ApiError> {
    let ct = state.schemas.create(payload).await?;
    state.events.emit(Event::SchemaCreated { name: ct.name.clone() }).await;
    Ok((StatusCode::CREATED, Json(ct)))
}

async fn patch_one(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<PatchContentType>,
) -> Result<Json<ContentType>, ApiError> {
    let ct = state.schemas.patch(&name, payload).await?;
    state.events.emit(Event::SchemaUpdated { name: ct.name.clone() }).await;
    Ok(Json(ct))
}

#[derive(Deserialize)]
struct DeleteQuery {
    confirm: Option<bool>,
}

async fn delete_one(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> Result<StatusCode, ApiError> {
    if q.confirm != Some(true) {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single("confirm_required: pass ?confirm=true"),
        )));
    }
    state.schemas.delete(&name).await?;
    state.events.emit(Event::SchemaDeleted { name }).await;
    Ok(StatusCode::NO_CONTENT)
}
