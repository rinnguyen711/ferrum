//! /api/admin/tokens — CRUD for API tokens (admin-only).

use crate::error::ApiError;
use crate::routes::content::db;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{DateTime, Utc};
use rustapi_core::{Action, Error, Principal};
use rustapi_sql::{delete_token, insert_token, list_tokens};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/tokens", get(list).post(create))
        .route("/api/admin/tokens/:id", axum::routing::delete(revoke))
}

async fn ensure_admin(state: &AppState, principal: &Principal) -> Result<(), ApiError> {
    if !state.authz.can(principal, Action::UserWrite, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

#[derive(Serialize)]
struct TokenView {
    id: Uuid,
    name: String,
    scopes: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct CreateTokenResponse {
    token: String,
    #[serde(flatten)]
    meta: TokenView,
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    scopes: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
}

async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<TokenView>>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let rows = list_tokens(&state.pool).await.map_err(db)?;
    Ok(Json(rows.into_iter().map(|t| TokenView {
        id: t.id,
        name: t.name,
        scopes: t.scopes,
        expires_at: t.expires_at,
        last_used_at: t.last_used_at,
        created_at: t.created_at,
    }).collect()))
}

async fn create(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<CreateTokenResponse>), ApiError> {
    ensure_admin(&state, &principal).await?;

    if body.scopes.is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::field("scopes", "at least one scope is required"),
        )));
    }
    if body.name.trim().is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::field("name", "name is required"),
        )));
    }

    // Generate raw token: rat_ + 32 random bytes as hex.
    let raw = format!("rat_{}", hex::encode(generate_bytes()));

    let row = insert_token(&state.pool, &body.name, &raw, &body.scopes, body.expires_at)
        .await
        .map_err(db)?;

    Ok((StatusCode::CREATED, Json(CreateTokenResponse {
        token: raw,
        meta: TokenView {
            id: row.id,
            name: row.name,
            scopes: row.scopes,
            expires_at: row.expires_at,
            last_used_at: row.last_used_at,
            created_at: row.created_at,
        },
    })))
}

async fn revoke(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure_admin(&state, &principal).await?;
    let deleted = delete_token(&state.pool, id).await.map_err(db)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(Error::NotFound))
    }
}

fn generate_bytes() -> [u8; 32] {
    // uuid::Uuid::new_v4() uses getrandom internally — borrow that entropy.
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(a.as_bytes());
    out[16..].copy_from_slice(b.as_bytes());
    out
}
