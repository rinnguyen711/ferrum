//! /api/single-types/:name handlers.

use crate::entry::{body_to_binds, row_to_json};
use crate::error::ApiError;
use crate::state::{AppState, WriteContext, WriteOp};
use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use ferrum_core::{Action, ContentTypeKind, Error, Event, Principal, ValidationErrors};
use ferrum_schema::bind::bind_all;
use ferrum_sql::Sort;
use serde_json::{Map, Value};
use sqlx::Row;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/single-types/:name", get(get_single).put(put_single))
}

async fn get_single(
    State(state): State<AppState>,
    Path(name): Path<String>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .authz
        .can(&principal, Action::ContentRead, &name)
        .await
    {
        return Err(ApiError(Error::Forbidden));
    }
    let ct = state
        .schemas
        .registry()
        .get(&name)
        .await
        .ok_or(ApiError(Error::NotFound))?;
    if ct.kind != ContentTypeKind::Single {
        return Err(ApiError(Error::Validation(ValidationErrors::single(
            "use /api/:type for collection types",
        ))));
    }

    let (sql, binds) = ferrum_sql::select_list_status(
        &ct.name,
        &ferrum_sql::Filter::default(),
        &Sort::default_created_at(),
        1,
        0,
        ferrum_sql::PublishFilter::All,
    )
    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q
        .fetch_optional(&state.pool)
        .await
        .map_err(super::content::db)?;
    match row {
        None => Ok(Json(Value::Null)),
        Some(r) => Ok(Json(row_to_json(&ct, &r)?)),
    }
}

async fn put_single(
    State(state): State<AppState>,
    Path(name): Path<String>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    Json(body): Json<Map<String, Value>>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .authz
        .can(&principal, Action::ContentWrite, &name)
        .await
    {
        return Err(ApiError(Error::Forbidden));
    }
    let ct = state
        .schemas
        .registry()
        .get(&name)
        .await
        .ok_or(ApiError(Error::NotFound))?;
    if ct.kind != ContentTypeKind::Single {
        return Err(ApiError(Error::Validation(ValidationErrors::single(
            "use /api/:type for collection types",
        ))));
    }

    let ctx = WriteContext {
        content_type: &ct.name,
        operation: WriteOp::Update,
        principal: &principal,
    };
    let body = state
        .hooks
        .before_write(&ctx, body)
        .await
        .map_err(ApiError)?;

    let (binds_map, _checks, _links, _media_checks, _media_links) =
        body_to_binds(&ct, body, false)?;

    // Check if entry already exists
    let (sel_sql, sel_binds) = ferrum_sql::select_list_status(
        &ct.name,
        &ferrum_sql::Filter::default(),
        &Sort::default_created_at(),
        1,
        0,
        ferrum_sql::PublishFilter::All,
    )
    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let existing_row = bind_all(sqlx::query(&sel_sql), &sel_binds)
        .fetch_optional(&state.pool)
        .await
        .map_err(super::content::db)?;

    let record = if let Some(existing) = existing_row {
        let existing_id: Uuid = existing
            .try_get("id")
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
        let (sql, binds) = ferrum_sql::update(&ct, existing_id, &binds_map)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        let row = bind_all(sqlx::query(&sql), &binds)
            .fetch_one(&state.pool)
            .await
            .map_err(super::content::db)?;
        let r = row_to_json(&ct, &row)?;
        state
            .events
            .emit(Event::EntryUpdated {
                content_type: ct.name.clone(),
                id: existing_id,
            })
            .await;
        r
    } else {
        let (sql, binds) = ferrum_sql::insert(&ct, &binds_map)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        let row = bind_all(sqlx::query(&sql), &binds)
            .fetch_one(&state.pool)
            .await
            .map_err(super::content::db)?;
        let r = row_to_json(&ct, &row)?;
        let new_id = r
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| ApiError(Error::Internal(anyhow::anyhow!("insert returned no id"))))?;
        state
            .events
            .emit(Event::EntryCreated {
                content_type: ct.name.clone(),
                id: new_id,
            })
            .await;
        r
    };

    state
        .hooks
        .after_write(&ctx, &record)
        .await
        .map_err(ApiError)?;
    Ok(Json(record))
}
