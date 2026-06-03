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

// Task 13 imports
use rustapi_media::ProviderDescriptor;

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

// ---- Task 13: settings + providers ----

async fn list_providers(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<ProviderDescriptor>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    Ok(Json(rustapi_media::descriptors()))
}

const MASK: &str = "••••";

#[derive(Serialize)]
struct SettingsView { provider: String, config: serde_json::Value }

async fn get_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Option<SettingsView>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let row = store::get_settings(&state.pool).await.map_err(internal)?;
    let view = row.map(|r| {
        let mut cfg = r.config.clone();
        if let Some(obj) = cfg.as_object_mut() {
            for name in rustapi_media::secret_fields(&r.provider) {
                if obj.contains_key(name) {
                    obj.insert(name.to_string(), serde_json::Value::String(MASK.into()));
                }
            }
        }
        SettingsView { provider: r.provider, config: cfg }
    });
    Ok(Json(view))
}

#[derive(Deserialize)]
struct SettingsBody { provider: String, config: serde_json::Value }

/// Encrypt secret fields. If a secret equals the mask (or absent), reuse the
/// previously-stored encrypted value instead of re-encrypting.
fn prepare_config_for_save(
    state: &AppState,
    provider: &str,
    mut config: serde_json::Value,
    previous: Option<&store::SettingsRow>,
) -> Result<serde_json::Value, ApiError> {
    let secrets = rustapi_media::secret_fields(provider);
    if secrets.is_empty() {
        return Ok(config);
    }
    let key = state.secret_key.ok_or_else(|| {
        ApiError(Error::Conflict("RUSTAPI_SECRET_KEY not set; cannot store provider secrets".into()))
    })?;
    if let Some(obj) = config.as_object_mut() {
        for name in secrets {
            match obj.get(name).and_then(|v| v.as_str()) {
                Some(MASK) | None => {
                    if let Some(prev) = previous.and_then(|p| {
                        if p.provider == provider { p.config.get(name).cloned() } else { None }
                    }) {
                        obj.insert(name.to_string(), prev);
                    }
                }
                Some(plain) => {
                    let enc = rustapi_media::secret::encrypt(&key, plain)
                        .map_err(|_| internal(anyhow::anyhow!("encrypt failed")))?;
                    obj.insert(name.to_string(), serde_json::Value::String(enc));
                }
            }
        }
    }
    Ok(config)
}

async fn put_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<SettingsBody>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    rustapi_media::validate(&body.provider, &body.config)
        .map_err(|e| ApiError(Error::Validation(rustapi_core::ValidationErrors::field("config", e.to_string()))))?;

    let previous = store::get_settings(&state.pool).await.map_err(internal)?;
    let to_store = prepare_config_for_save(&state, &body.provider, body.config.clone(), previous.as_ref())?;
    store::put_settings(&state.pool, &body.provider, &to_store).await.map_err(internal)?;

    let mut live_cfg = to_store.clone();
    if let Some(key) = &state.secret_key {
        crate::media::boot::decrypt_secrets(&body.provider, &mut live_cfg, key);
    }
    let provider = rustapi_media::build(&body.provider, &live_cfg)
        .map_err(|e| ApiError(Error::Unsupported(e.to_string())))?;
    *state.storage.write().await = std::sync::Arc::from(provider);
    Ok(StatusCode::NO_CONTENT)
}

async fn test_settings(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<SettingsBody>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    rustapi_media::validate(&body.provider, &body.config)
        .map_err(|e| ApiError(Error::Validation(rustapi_core::ValidationErrors::field("config", e.to_string()))))?;
    let mut cfg = body.config.clone();
    if let Some(obj) = cfg.as_object_mut() {
        let prev = store::get_settings(&state.pool).await.map_err(internal)?;
        for name in rustapi_media::secret_fields(&body.provider) {
            if obj.get(name).and_then(|v| v.as_str()) == Some(MASK) {
                if let (Some(prev), Some(key)) = (&prev, &state.secret_key) {
                    if prev.provider == body.provider {
                        if let Some(serde_json::Value::String(enc)) = prev.config.get(name) {
                            if let Ok(plain) = rustapi_media::secret::decrypt(key, enc) {
                                obj.insert(name.to_string(), serde_json::Value::String(plain));
                            }
                        }
                    }
                }
            }
        }
    }
    let provider = rustapi_media::build(&body.provider, &cfg)
        .map_err(|e| ApiError(Error::Unsupported(e.to_string())))?;
    provider.test().await
        .map_err(|e| ApiError(Error::Conflict(format!("connection test failed: {e}"))))?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- STUBS replaced in task 14 ----
async fn list_assets() -> Result<Json<serde_json::Value>, ApiError> { Ok(Json(serde_json::json!([]))) }
async fn upload_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn get_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn update_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn delete_asset() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
async fn get_asset_raw() -> Result<StatusCode, ApiError> { Ok(StatusCode::NOT_IMPLEMENTED) }
