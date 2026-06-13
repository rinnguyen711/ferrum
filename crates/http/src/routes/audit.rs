//! /api/admin/audit — read-only audit log surface (admin-only).

use crate::error::ApiError;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    routing::get,
    Extension, Json, Router,
};
use rustapi_core::{Action, Error, Principal};
use rustapi_sql::audit::{audit_category_counts, audit_stats, query_audit, AuditQuery};
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/audit", get(list))
        .route("/api/admin/audit/stats", get(stats))
        .route("/api/admin/audit/export", get(export))
}

async fn ensure_admin(state: &AppState, principal: &Principal) -> Result<(), ApiError> {
    if !state.authz.can(principal, Action::UserWrite, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError(Error::Internal(anyhow::anyhow!(e.to_string())))
}

#[derive(Deserialize)]
struct ListParams {
    category: Option<String>,
    status: Option<String>,
    actor_id: Option<Uuid>,
    target_type: Option<String>,
    target_id: Option<String>,
    q: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

impl ListParams {
    fn to_query(&self, limit: i64, offset: i64) -> AuditQuery {
        AuditQuery {
            category: self.category.clone(),
            status: self.status.clone(),
            actor_id: self.actor_id,
            target_type: self.target_type.clone(),
            target_id: self.target_id.clone(),
            q: self.q.clone(),
            limit,
            offset,
        }
    }
}

async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(p): Query<ListParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let per_page = p.per_page.unwrap_or(25).clamp(1, 100);
    let page = p.page.unwrap_or(1).max(1);
    let q = p.to_query(per_page, (page - 1) * per_page);
    let (rows, total) = query_audit(&state.pool, &q).await.map_err(internal)?;
    let counts: HashMap<String, i64> = audit_category_counts(&state.pool)
        .await
        .map_err(internal)?
        .into_iter()
        .collect();
    Ok(Json(serde_json::json!({
        "rows": rows,
        "total": total,
        "page": page,
        "per_page": per_page,
        "category_counts": counts,
    })))
}

async fn stats(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<serde_json::Value>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let s = audit_stats(&state.pool).await.map_err(internal)?;
    Ok(Json(serde_json::to_value(s).map_err(internal)?))
}

async fn export(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(p): Query<ListParams>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    ensure_admin(&state, &principal).await?;
    let q = p.to_query(10_000, 0);
    let (rows, _) = query_audit(&state.pool, &q).await.map_err(internal)?;
    let mut csv = String::from("time,actor,action,target_type,target,status,ip\n");
    for r in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            r.created_at.to_rfc3339(),
            r.actor_label.replace(',', " "),
            r.action,
            r.target_type.unwrap_or_default(),
            r.target_label.unwrap_or_default().replace(',', " "),
            r.status,
            r.ip.unwrap_or_default(),
        ));
    }
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"audit-log.csv\"",
            ),
        ],
        csv,
    )
        .into_response())
}
