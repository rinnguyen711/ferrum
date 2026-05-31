//! /api/:type/* handlers.

use crate::entry::{body_to_binds, row_to_json, RelationCheck};
use crate::error::ApiError;
use crate::query::{parse_list, ListParams};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rustapi_core::{Action, Error, Event, Principal, ValidationErrors};
use rustapi_schema::bind::{bind_all, bind_all_as};
use serde_json::{json, Map, Value};
use sqlx::Row;
use std::collections::HashMap;
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
    axum::extract::RawQuery(raw_query): axum::extract::RawQuery,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentRead, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let opts = parse_list(&ct, params, state.config.page_size_max)?;
    let offset: i64 = ((opts.page - 1) as i64) * (opts.page_size as i64);

    let filter = crate::filter::parse(raw_query.as_deref().unwrap_or(""), &ct)?;

    let (list_sql, list_binds) = rustapi_sql::select_list(
        &ct.name,
        &filter,
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

    let (count_sql, count_binds) = rustapi_sql::count(&ct.name, &filter)
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
    let (binds_map, checks) = body_to_binds(&ct, body, true)?;
    verify_relation_targets_exist(&state, &checks).await?;

    let (sql, binds) = rustapi_sql::insert(&ct, &binds_map)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_one(&state.pool).await.map_err(|e| db_with_relation_context(e, &checks))?;

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
    let (mut binds_map, checks) = body_to_binds(&ct, body, true)?;
    verify_relation_targets_exist(&state, &checks).await?;

    // PUT is full-replace per spec §6.2: fields absent from the body that are
    // not required get explicitly nulled. Required-with-no-default would have
    // been rejected by body_to_binds; required-with-default keeps its existing
    // value (we can't infer the default safely from the wire-encoded SQL
    // literal, so we omit and let the column keep its current value — same
    // shape as if the client had POSTed the entry without that field). For
    // relation fields the typed null is FieldKind::Uuid (matches the FK col).
    for f in &ct.fields {
        if !binds_map.contains_key(&f.name) && !f.required {
            let null_kind = if f.kind == rustapi_core::FieldKind::Relation {
                rustapi_core::FieldKind::Uuid
            } else {
                f.kind
            };
            binds_map.insert(f.name.clone(), rustapi_core::BoundValue::Null(null_kind));
        }
    }

    let (sql, binds) = rustapi_sql::update(&ct, id, &binds_map)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_optional(&state.pool).await.map_err(|e| db_with_relation_context(e, &checks))?;
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

/// Per relation target, batch-`SELECT id FROM "<target>" WHERE id = ANY($1)`,
/// then surface a 422 RelationTargetMissing for the **first** relation field
/// (in payload order) with any unresolved ids. Race-free for the same txn —
/// if a parallel delete removes a row between this check and the INSERT/
/// UPDATE, Postgres raises 23503 and `db_with_relation_context` re-maps it.
async fn verify_relation_targets_exist(
    state: &AppState,
    checks: &[RelationCheck],
) -> Result<(), ApiError> {
    if checks.is_empty() {
        return Ok(());
    }
    let mut by_target: HashMap<&str, Vec<Uuid>> = HashMap::new();
    for c in checks {
        by_target.entry(c.target.as_str()).or_default().push(c.id);
    }
    let mut found: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for (target, ids) in &by_target {
        let table = rustapi_sql::table_name(target)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        let sql = format!("SELECT id FROM {table} WHERE id = ANY($1)");
        let rows = sqlx::query(&sql)
            .bind(ids)
            .fetch_all(&state.pool)
            .await
            .map_err(db)?;
        for r in &rows {
            let id: Uuid = r.try_get("id").map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
            found.insert(id);
        }
    }
    // Walk in payload order so the first relation field with any miss wins,
    // matching the spec's "report the first failure" contract.
    let mut current_field: Option<&str> = None;
    let mut missing: Vec<String> = Vec::new();
    for c in checks {
        if !found.contains(&c.id) {
            match current_field {
                None => {
                    current_field = Some(&c.field);
                    missing.push(c.id.to_string());
                }
                Some(name) if name == c.field => missing.push(c.id.to_string()),
                Some(_) => break,
            }
        }
    }
    if let Some(field) = current_field {
        return Err(ApiError(Error::Validation(
            ValidationErrors::relation_target_missing(field, missing),
        )));
    }
    Ok(())
}

/// Wrap the standard `db` mapper to re-tag PG 23503 (FK violation) as a
/// RelationTargetMissing when a write race deleted the target row between
/// the pre-check and the INSERT/UPDATE. Best-effort: parses the FK
/// constraint name's `<table>_<col>_fkey` suffix to recover the field name.
/// If parsing fails the original FK-violation surfaces as 409 — better
/// surfaced than misleading.
fn db_with_relation_context(e: sqlx::Error, checks: &[RelationCheck]) -> ApiError {
    if let sqlx::Error::Database(d) = &e {
        if d.code().as_deref() == Some("23503") {
            if let Some(constraint) = d.constraint() {
                if let Some(field) = relation_field_from_constraint(constraint, checks) {
                    return ApiError(Error::Validation(
                        ValidationErrors::relation_target_missing(field, vec![]),
                    ));
                }
            }
        }
    }
    db(e)
}

/// Pg names FK constraints `<table>_<column>_fkey` by default. We compare
/// the column piece against `<field>_id` for each relation check.
fn relation_field_from_constraint(constraint: &str, checks: &[RelationCheck]) -> Option<String> {
    let trimmed = constraint.strip_suffix("_fkey").unwrap_or(constraint);
    for c in checks {
        let needle = format!("{}_id", c.field);
        if trimmed.ends_with(&needle) {
            return Some(c.field.clone());
        }
    }
    None
}
