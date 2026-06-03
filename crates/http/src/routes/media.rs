//! /admin/media/* handlers. Authz reuses content actions: read → ContentRead,
//! write → ContentWrite.

use crate::error::ApiError;
use crate::media::store;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::{DateTime, Utc};
use rustapi_core::{Action, Error, Principal};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/media/providers", get(list_providers))
        .route("/admin/media/settings", get(get_settings).put(put_settings))
        .route("/admin/media/settings/test", post(test_settings))
        .route("/admin/media/folders", get(list_folders).post(create_folder))
        .route("/admin/media/folders/:id", axum::routing::patch(update_folder).delete(delete_folder))
        .route("/admin/media/assets", get(list_assets).post(upload_asset))
        .route("/admin/media/assets/:id", get(get_asset).patch(update_asset).delete(delete_asset))
        .route("/admin/media/assets/:id/raw", get(get_asset_raw))
}

async fn ensure(state: &AppState, principal: &Principal, action: Action) -> Result<(), ApiError> {
    if !state.authz.can(principal, action, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

fn internal<E: Into<anyhow::Error>>(e: E) -> ApiError {
    ApiError(Error::Internal(e.into()))
}

#[derive(Serialize)]
struct FolderView {
    id: Uuid,
    parent_id: Option<Uuid>,
    name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl From<store::FolderRow> for FolderView {
    fn from(f: store::FolderRow) -> Self {
        FolderView { id: f.id, parent_id: f.parent_id, name: f.name, created_at: f.created_at, updated_at: f.updated_at }
    }
}

#[derive(Deserialize)]
struct FolderQuery { parent_id: Option<Uuid> }

async fn list_folders(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<FolderQuery>,
) -> Result<Json<Vec<FolderView>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let rows = store::list_folders(&state.pool, q.parent_id).await.map_err(internal)?;
    Ok(Json(rows.into_iter().map(FolderView::from).collect()))
}

#[derive(Deserialize)]
struct CreateFolderBody { parent_id: Option<Uuid>, name: String }

async fn create_folder(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<CreateFolderBody>,
) -> Result<(StatusCode, Json<FolderView>), ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    if body.name.trim().is_empty() {
        return Err(ApiError(Error::Validation(rustapi_core::ValidationErrors::field("name", "required"))));
    }
    let row = store::create_folder(&state.pool, body.parent_id, body.name.trim())
        .await
        .map_err(map_folder_err)?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

#[derive(Deserialize)]
struct UpdateFolderBody {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    parent_id: Option<Option<Uuid>>,
}

async fn update_folder(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFolderBody>,
) -> Result<Json<FolderView>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    let row = store::update_folder(&state.pool, id, body.name.as_deref(), body.parent_id)
        .await
        .map_err(map_folder_err)?
        .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(row.into()))
}

async fn delete_folder(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    if store::folder_has_children(&state.pool, id).await.map_err(internal)? {
        return Err(ApiError(Error::Conflict("folder is not empty".into())));
    }
    if store::delete_folder(&state.pool, id).await.map_err(internal)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(Error::NotFound))
    }
}

fn map_folder_err(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(db) = &e {
        if db.code().as_deref() == Some("23505") {
            return ApiError(Error::Conflict("a folder with that name already exists here".into()));
        }
    }
    ApiError(Error::Internal(e.into()))
}

/// serde helper: `Option<Option<T>>` distinguishing absent vs explicit null.
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(de).map(Some)
}

// ---- STUBS replaced in tasks 13–14 ----
async fn list_providers() -> Result<Json<serde_json::Value>, ApiError> { Ok(Json(serde_json::json!([]))) }
async fn get_settings() -> Result<Json<serde_json::Value>, ApiError> { Ok(Json(serde_json::json!(null))) }
async fn put_settings() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn test_settings() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn list_assets() -> Result<Json<serde_json::Value>, ApiError> { Ok(Json(serde_json::json!([]))) }
async fn upload_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn get_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn update_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn delete_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn get_asset_raw() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
