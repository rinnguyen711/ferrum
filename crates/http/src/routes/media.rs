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

// Task 14 imports
use axum::body::Body;
use axum::extract::Multipart;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use sha2::{Digest, Sha256};

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
struct FolderQuery {
    parent_id: Option<Uuid>,
    scope: Option<String>,
}

async fn list_folders(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<FolderQuery>,
) -> Result<Json<Vec<FolderView>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let rows = if q.scope.as_deref() == Some("all") {
        store::list_all_folders(&state.pool).await.map_err(internal)?
    } else {
        store::list_folders(&state.pool, q.parent_id).await.map_err(internal)?
    };
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
    // PostgreSQL UNIQUE (parent_id, name) treats NULLs as distinct, so root-level
    // duplicates are not caught by the DB constraint. Check explicitly.
    if body.parent_id.is_none() {
        let siblings = store::list_folders(&state.pool, None).await.map_err(internal)?;
        if siblings.iter().any(|f| f.name == body.name.trim()) {
            return Err(ApiError(Error::Conflict("a folder with that name already exists here".into())));
        }
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

// ---- Task 14: asset handlers ----

#[derive(Serialize)]
pub(crate) struct AssetView {
    id: Uuid,
    folder_id: Option<Uuid>,
    file_name: String,
    alt_text: Option<String>,
    caption: Option<String>,
    mime_type: String,
    size_bytes: i64,
    width: Option<i32>,
    height: Option<i32>,
    original_filename: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl From<store::AssetRow> for AssetView {
    fn from(a: store::AssetRow) -> Self {
        AssetView {
            id: a.id, folder_id: a.folder_id, file_name: a.file_name, alt_text: a.alt_text,
            caption: a.caption, mime_type: a.mime_type, size_bytes: a.size_bytes,
            width: a.width, height: a.height, original_filename: a.original_filename,
            created_at: a.created_at, updated_at: a.updated_at,
        }
    }
}

#[derive(Deserialize)]
struct AssetQuery { folder_id: Option<Uuid> }

async fn list_assets(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<AssetQuery>,
) -> Result<Json<Vec<AssetView>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let rows = store::list_assets(&state.pool, q.folder_id).await.map_err(internal)?;
    Ok(Json(rows.into_iter().map(AssetView::from).collect()))
}

async fn upload_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<AssetView>), ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;

    let mut folder_id: Option<Uuid> = None;
    let mut file_bytes: Option<Bytes> = None;
    let mut original_filename = String::from("upload");

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        ApiError(Error::Unsupported(format!("bad multipart: {e}")))
    })? {
        match field.name() {
            Some("folder_id") => {
                let txt = field.text().await.map_err(|e| ApiError(Error::Unsupported(e.to_string())))?;
                if !txt.is_empty() {
                    folder_id = Some(Uuid::parse_str(&txt).map_err(|_| {
                        ApiError(Error::Validation(rustapi_core::ValidationErrors::field("folder_id", "invalid uuid")))
                    })?);
                }
            }
            Some("file") => {
                if let Some(fname) = field.file_name() { original_filename = fname.to_string(); }
                let data = field.bytes().await.map_err(|e| ApiError(Error::Unsupported(e.to_string())))?;
                file_bytes = Some(data);
            }
            _ => { let _ = field.bytes().await; }
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        ApiError(Error::Validation(rustapi_core::ValidationErrors::field("file", "required")))
    })?;

    let mime_type = infer::get(&bytes).map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let checksum = {
        let mut h = Sha256::new();
        h.update(&bytes);
        format!("{:x}", h.finalize())
    };
    let (width, height) = match imagesize::blob_size(&bytes) {
        Ok(d) => (Some(d.width as i32), Some(d.height as i32)),
        Err(_) => (None, None),
    };

    let id = Uuid::new_v4();
    let storage_key = format!("{id}/{original_filename}");

    let provider = state.storage.read().await.clone();
    provider.put(&storage_key, bytes.clone(), &mime_type).await
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!("storage put: {e}"))))?;

    let provider_id = current_provider_id(&state).await;

    let row = store::create_asset(&state.pool, store::NewAsset {
        folder_id,
        provider: &provider_id,
        storage_key: &storage_key,
        file_name: &original_filename,
        mime_type: &mime_type,
        size_bytes: bytes.len() as i64,
        width,
        height,
        original_filename: &original_filename,
        checksum: Some(&checksum),
    }).await.map_err(map_folder_err)?;

    Ok((StatusCode::CREATED, Json(row.into())))
}

/// Active provider id: env override → DB settings → "local" default.
async fn current_provider_id(state: &AppState) -> String {
    if let Ok(p) = std::env::var("RUSTAPI_MEDIA_PROVIDER") { return p; }
    if let Ok(Some(row)) = store::get_settings(&state.pool).await { return row.provider; }
    "local".to_string()
}

async fn get_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<AssetView>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let row = store::get_asset(&state.pool, id).await.map_err(internal)?
        .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(row.into()))
}

#[derive(Deserialize)]
struct UpdateAssetBody {
    file_name: Option<String>,
    alt_text: Option<String>,
    caption: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    folder_id: Option<Option<Uuid>>,
}

async fn update_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateAssetBody>,
) -> Result<Json<AssetView>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    let row = store::update_asset(
        &state.pool, id,
        body.file_name.as_deref(), body.alt_text.as_deref(),
        body.caption.as_deref(), body.folder_id,
    ).await.map_err(internal)?.ok_or(ApiError(Error::NotFound))?;
    Ok(Json(row.into()))
}

async fn delete_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::ContentWrite).await?;
    let row = store::get_asset(&state.pool, id).await.map_err(internal)?
        .ok_or(ApiError(Error::NotFound))?;
    let provider = state.storage.read().await.clone();
    let _ = provider.delete(&row.storage_key).await;
    store::delete_asset(&state.pool, id).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_asset_raw(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let row = store::get_asset(&state.pool, id).await.map_err(internal)?
        .ok_or(ApiError(Error::NotFound))?;
    let provider = state.storage.read().await.clone();
    let bytes = provider.get(&row.storage_key).await.map_err(|e| match e {
        rustapi_media::StorageError::NotFound => ApiError(Error::NotFound),
        other => ApiError(Error::Internal(anyhow::anyhow!("storage get: {other}"))),
    })?;
    Ok((
        [(header::CONTENT_TYPE, row.mime_type)],
        Body::from(bytes),
    ).into_response())
}
