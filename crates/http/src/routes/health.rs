//! Liveness / readiness.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

pub async fn healthz(State(state): State<AppState>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    sqlx::query("SELECT 1").execute(&state.pool).await.map_err(|e| {
        ApiError(rustapi_core::Error::Internal(anyhow::anyhow!(e)))
    })?;
    Ok((StatusCode::OK, Json(json!({"status": "ok"}))))
}
