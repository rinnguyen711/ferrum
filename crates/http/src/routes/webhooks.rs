//! /api/admin/webhooks — CRUD for webhooks (admin-only).

use crate::error::ApiError;
use crate::routes::content::db;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{DateTime, Utc};
use rustapi_core::{Action, Error, Principal};
use rustapi_sql::{
    delete_webhook, insert_webhook, list_deliveries, list_webhooks, update_webhook, Webhook,
    WebhookDelivery,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/webhooks", get(list).post(create))
        .route(
            "/api/admin/webhooks/:id",
            axum::routing::patch(update).delete(delete),
        )
        .route("/api/admin/webhooks/:id/deliveries", get(deliveries))
        .route(
            "/api/admin/webhooks/:id/test",
            axum::routing::post(test_ping),
        )
}

async fn ensure_admin(state: &AppState, principal: &Principal) -> Result<(), ApiError> {
    if !state.authz.can(principal, Action::UserWrite, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

#[derive(Serialize)]
struct WebhookView {
    id: Uuid,
    name: String,
    url: String,
    events: Vec<String>,
    enabled: bool,
    created_at: DateTime<Utc>,
}

impl From<Webhook> for WebhookView {
    fn from(w: Webhook) -> Self {
        Self {
            id: w.id,
            name: w.name,
            url: w.url,
            events: w.events,
            enabled: w.enabled,
            created_at: w.created_at,
        }
    }
}

#[derive(Serialize)]
struct DeliveryView {
    id: Uuid,
    webhook_id: Uuid,
    event: String,
    status: String,
    attempt: i32,
    last_error: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<WebhookDelivery> for DeliveryView {
    fn from(d: WebhookDelivery) -> Self {
        Self {
            id: d.id,
            webhook_id: d.webhook_id,
            event: d.event,
            status: d.status,
            attempt: d.attempt,
            last_error: d.last_error,
            created_at: d.created_at,
        }
    }
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    url: String,
    events: Vec<String>,
    #[serde(default)]
    secret: Option<String>,
}

#[derive(Deserialize)]
struct UpdateBody {
    name: String,
    url: String,
    events: Vec<String>,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

const VALID_EVENTS: &[&str] = &[
    "entry.created",
    "entry.updated",
    "entry.deleted",
    "entry.published",
    "entry.unpublished",
];

fn validate_events(events: &[String]) -> Result<(), ApiError> {
    if events.is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::field("events", "at least one event is required"),
        )));
    }
    for e in events {
        if !VALID_EVENTS.contains(&e.as_str()) {
            return Err(ApiError(Error::Validation(
                rustapi_core::ValidationErrors::field("events", "invalid event name"),
            )));
        }
    }
    Ok(())
}

fn validate_url(url: &str) -> Result<(), ApiError> {
    if url.trim().is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::field("url", "url is required"),
        )));
    }
    url::Url::parse(url).map_err(|_| {
        ApiError(Error::Validation(rustapi_core::ValidationErrors::field(
            "url",
            "url must be a valid URL",
        )))
    })?;
    Ok(())
}

async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<WebhookView>>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let rows = list_webhooks(&state.pool).await.map_err(db)?;
    Ok(Json(rows.into_iter().map(WebhookView::from).collect()))
}

async fn create(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<WebhookView>), ApiError> {
    ensure_admin(&state, &principal).await?;
    if body.name.trim().is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::field("name", "name is required"),
        )));
    }
    validate_url(&body.url)?;
    validate_events(&body.events)?;
    let row = insert_webhook(
        &state.pool,
        &body.name,
        &body.url,
        &body.events,
        body.secret.as_deref(),
    )
    .await
    .map_err(db)?;
    Ok((StatusCode::CREATED, Json(WebhookView::from(row))))
}

async fn update(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<WebhookView>, ApiError> {
    ensure_admin(&state, &principal).await?;
    if body.name.trim().is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::field("name", "name is required"),
        )));
    }
    validate_url(&body.url)?;
    validate_events(&body.events)?;
    let row = update_webhook(
        &state.pool,
        id,
        &body.name,
        &body.url,
        &body.events,
        body.secret.as_deref(),
        body.enabled,
    )
    .await
    .map_err(db)?
    .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(WebhookView::from(row)))
}

async fn delete(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure_admin(&state, &principal).await?;
    let deleted = delete_webhook(&state.pool, id).await.map_err(db)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(Error::NotFound))
    }
}

async fn deliveries(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DeliveryView>>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let rows = list_deliveries(&state.pool, id, 100).await.map_err(db)?;
    Ok(Json(rows.into_iter().map(DeliveryView::from).collect()))
}

async fn test_ping(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure_admin(&state, &principal).await?;
    let hooks = list_webhooks(&state.pool).await.map_err(db)?;
    let hook = hooks
        .into_iter()
        .find(|h| h.id == id)
        .ok_or(ApiError(Error::NotFound))?;
    if !hook.enabled {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single("webhook is disabled"),
        )));
    }
    let payload = serde_json::json!({
        "event": "ping",
        "createdAt": chrono::Utc::now(),
        "model": null,
        "entry": null,
    });
    sqlx::query(
        "INSERT INTO _webhook_deliveries (webhook_id, event, payload)
         VALUES ($1, 'ping', $2)",
    )
    .bind(id)
    .bind(&payload)
    .execute(&state.pool)
    .await
    .map_err(db)?;
    Ok(StatusCode::ACCEPTED)
}
