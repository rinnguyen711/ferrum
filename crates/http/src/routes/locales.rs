//! /admin/locales CRUD. Admin-gated. Reloads LocaleRegistry on every
//! mutation, mirroring routes/roles.rs.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Extension, Json, Router};
use ferrum_core::{Action, Error, Principal};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/locales", get(list).post(upsert))
        .route("/admin/locales/:code", axum::routing::delete(delete_one))
}

async fn ensure_admin(state: &AppState, principal: &Principal) -> Result<(), ApiError> {
    if !state.authz.can(principal, Action::UserWrite, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

async fn reload(state: &AppState) -> Result<(), ApiError> {
    let all = ferrum_sql::locales::load_all(&state.pool)
        .await
        .map_err(ApiError)?;
    state.locales.set(all).await;
    Ok(())
}

async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let all = ferrum_sql::locales::load_all(&state.pool)
        .await
        .map_err(ApiError)?;
    Ok(Json(json!({ "data": all })))
}

#[derive(Deserialize)]
struct UpsertBody {
    code: String,
    name: String,
    #[serde(default)]
    position: i32,
    #[serde(default)]
    is_default: bool,
}

async fn upsert(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<UpsertBody>,
) -> Result<Json<Value>, ApiError> {
    ensure_admin(&state, &principal).await?;
    if !ferrum_core::is_valid_locale_tag(&body.code) {
        return Err(ApiError(Error::Validation(
            ferrum_core::ValidationErrors::single("invalid locale code"),
        )));
    }
    let loc = ferrum_sql::locales::upsert(
        &state.pool,
        &body.code,
        &body.name,
        body.position,
        body.is_default,
    )
    .await
    .map_err(ApiError)?;
    reload(&state).await?;
    Ok(Json(json!(loc)))
}

async fn delete_one(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Extension(principal): Extension<Principal>,
) -> Result<axum::http::StatusCode, ApiError> {
    ensure_admin(&state, &principal).await?;
    let deleted = ferrum_sql::locales::delete(&state.pool, &code)
        .await
        .map_err(ApiError)?;
    if !deleted {
        return Err(ApiError(Error::NotFound));
    }
    reload(&state).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
