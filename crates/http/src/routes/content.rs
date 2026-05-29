//! /api/:type/* handlers.

use crate::entry::{body_to_binds, row_to_json};
use crate::error::ApiError;
use crate::query::{parse_list, ListParams};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rustapi_core::{Action, Error, Event, Principal};
use rustapi_schema::bind::{bind_all, bind_all_as};
use serde_json::{json, Map, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/:type", get(list).post(create))
        .route("/api/:type/:id", get(get_one).put(update).delete(delete_one))
}

async fn ensure(state: &AppState, principal: &Principal, action: Action, ct: &str) -> Result<(), ApiError> {
    if !state.authz.can(principal, action, ct).await {
        return Err(ApiError(Error::Unauthorized));
    }
    Ok(())
}

async fn list(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    Query(params): Query<ListParams>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentRead, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let opts = parse_list(&ct, params, state.config.page_size_max)?;
    let offset: i64 = ((opts.page - 1) as i64) * (opts.page_size as i64);

    let (list_sql, list_binds) = rustapi_sql::select_list(
        &ct.name,
        &rustapi_sql::Filter::None,
        &opts.sort,
        opts.page_size as i64,
        offset,
    )
    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let q = bind_all(sqlx::query(&list_sql), &list_binds);
    let rows = q.fetch_all(&state.pool).await.map_err(db)?;

    let mut data = Vec::with_capacity(rows.len());
    for r in &rows {
        data.push(row_to_json(&ct, r)?);
    }

    let (count_sql, count_binds) =
        rustapi_sql::count(&ct.name, &rustapi_sql::Filter::None)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let cq = bind_all_as(sqlx::query_as::<_, (i64,)>(&count_sql), &count_binds);
    let total: i64 = cq.fetch_one(&state.pool).await.map_err(db)?.0;

    Ok(Json(json!({
        "data": data,
        "meta": {
            "page": opts.page,
            "pageSize": opts.page_size,
            "total": total
        }
    })))
}

async fn create(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    Json(body): Json<Map<String, Value>>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let binds_map = body_to_binds(&ct, body, true)?;

    let (sql, binds) = rustapi_sql::insert(&ct, &binds_map)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_one(&state.pool).await.map_err(db)?;

    let body = row_to_json(&ct, &row)?;
    let id = body.get("id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
    if let Some(id) = id {
        state.events.emit(Event::EntryCreated { content_type: ct.name.clone(), id }).await;
    }
    Ok((StatusCode::CREATED, Json(body)))
}

async fn get_one(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentRead, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let (sql, binds) = rustapi_sql::select_by_id(&ct.name, id)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_optional(&state.pool).await.map_err(db)?;
    let row = row.ok_or(ApiError(Error::NotFound))?;
    Ok(Json(row_to_json(&ct, &row)?))
}

async fn update(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    Json(body): Json<Map<String, Value>>,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let mut binds_map = body_to_binds(&ct, body, true)?;

    // PUT is full-replace per spec §6.2: fields absent from the body that are
    // not required get explicitly nulled. Required-with-no-default would have
    // been rejected by body_to_binds; required-with-default keeps its existing
    // value (we can't infer the default safely from the wire-encoded SQL
    // literal, so we omit and let the column keep its current value — same
    // shape as if the client had POSTed the entry without that field).
    for f in &ct.fields {
        if !binds_map.contains_key(&f.name) && !f.required {
            binds_map.insert(f.name.clone(), rustapi_core::BoundValue::Null(f.kind));
        }
    }

    let (sql, binds) = rustapi_sql::update(&ct, id, &binds_map)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_optional(&state.pool).await.map_err(db)?;
    let row = row.ok_or(ApiError(Error::NotFound))?;

    state.events.emit(Event::EntryUpdated { content_type: ct.name.clone(), id }).await;
    Ok(Json(row_to_json(&ct, &row)?))
}

async fn delete_one(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let _ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let (sql, binds) = rustapi_sql::delete(&ct_name, id)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let result = q.execute(&state.pool).await.map_err(db)?;
    if result.rows_affected() == 0 {
        return Err(ApiError(Error::NotFound));
    }
    state.events.emit(Event::EntryDeleted { content_type: ct_name.clone(), id }).await;
    Ok(StatusCode::NO_CONTENT)
}

fn db(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(d) = &e {
        if let Some(code) = d.code() {
            if code.as_ref() == "23505" {
                return ApiError(Error::Conflict(d.message().to_string()));
            }
        }
        let code = d.code().map(|c| c.into_owned()).unwrap_or_default();
        return ApiError(Error::Validation(rustapi_core::ValidationErrors::db(
            code,
            d.message(),
        )));
    }
    ApiError(Error::Internal(anyhow::anyhow!(e)))
}
