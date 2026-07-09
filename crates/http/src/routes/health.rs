//! Liveness / readiness.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use std::time::Instant;

pub async fn healthz(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let started = Instant::now();
    sqlx::query("SELECT 1")
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError(ferrum_core::Error::Internal(anyhow::anyhow!(e))))?;
    let db_ms = started.elapsed().as_millis() as i64;

    Ok((
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
            "db_ms": db_ms,
        })),
    ))
}
