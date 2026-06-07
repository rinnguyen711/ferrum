//! /api/:type/* handlers.

use crate::entry::{body_to_binds, row_to_json, RelationCheck};
use crate::error::ApiError;
use crate::populate::{self, PopulateField};
use crate::query::{parse_list, ListParams};
use crate::state::{AppState, WriteContext, WriteOp};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rustapi_core::{Action, ContentType, Error, Event, Principal, ValidationErrors};
use rustapi_sql::PublishFilter;
use rustapi_schema::bind::{bind_all, bind_all_as};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Deserialize, Default)]
struct GetParams {
    #[serde(default)]
    populate: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/:type", get(list).post(create))
        .route("/api/:type/:id", get(get_one).put(update).delete(delete_one))
        .route("/api/:type/:id/publish", axum::routing::post(publish_entry))
        .route("/api/:type/:id/unpublish", axum::routing::post(unpublish_entry))
}

async fn ensure(state: &AppState, principal: &Principal, action: Action, ct: &str) -> Result<(), ApiError> {
    if !state.authz.can(principal, action, ct).await {
        return Err(ApiError(Error::Forbidden));
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
    let populate_param = params.populate.clone();
    let status = params.status.clone();
    let opts = parse_list(&ct, params, state.config.page_size_max)?;
    let offset: i64 = ((opts.page - 1) as i64) * (opts.page_size as i64);

    let filter = crate::filter::parse(raw_query.as_deref().unwrap_or(""), &ct)?;

    let publish = if ct.draft_publish() {
        match status.as_deref() {
            Some("draft") => PublishFilter::Draft,
            Some("all") => PublishFilter::All,
            _ => PublishFilter::Published,
        }
    } else {
        PublishFilter::All
    };

    let (list_sql, list_binds) = rustapi_sql::select_list_status(
        &ct.name,
        &filter,
        &opts.sort,
        opts.page_size as i64,
        offset,
        publish,
    )
    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let q = bind_all(sqlx::query(&list_sql), &list_binds);
    let rows = q.fetch_all(&state.pool).await.map_err(db)?;

    let mut maps: Vec<Map<String, Value>> = Vec::with_capacity(rows.len());
    for r in &rows {
        match row_to_json(&ct, r)? {
            Value::Object(m) => maps.push(m),
            _ => unreachable!("row_to_json returns an object"),
        }
    }

    if let Some(raw) = populate_param.as_deref() {
        apply_populate(&state, &ct, raw, &mut maps).await?;
    }
    crate::media_embed::apply_media_embed(&state.pool, &ct, &mut maps).await.map_err(ApiError)?;

    let data: Vec<Value> = maps.into_iter().map(Value::Object).collect();

    let (count_sql, count_binds) = rustapi_sql::count_status(&ct.name, &filter, publish)
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

    let ctx = WriteContext {
        content_type: &ct.name,
        operation: WriteOp::Create,
        principal: &principal,
    };
    let body = state.hooks.before_write(&ctx, body).await.map_err(ApiError)?;

    let (binds_map, checks, links, media_checks, media_links) = body_to_binds(&ct, body, true)?;
    verify_relation_targets_exist(&state, &checks).await?;
    verify_link_targets_exist(&state, &links).await?;
    verify_media_targets_exist(&state, &media_checks).await?;
    verify_media_link_targets_exist(&state, &media_links).await?;

    let (sql, binds) = rustapi_sql::insert(&ct, &binds_map)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let mut tx = state.pool.begin().await.map_err(db)?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_one(&mut *tx).await.map_err(|e| db_with_relation_context(e, &checks))?;
    let record = row_to_json(&ct, &row)?;
    let new_id = record
        .get("id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ApiError(Error::Internal(anyhow::anyhow!("insert returned no id"))))?;

    write_links(&mut tx, &ct.name, &links, new_id).await?;
    write_media_links(&mut tx, &ct.name, &media_links, new_id).await?;
    tx.commit().await.map_err(db)?;

    state.hooks.after_write(&ctx, &record).await.map_err(ApiError)?;
    state.events.emit(Event::EntryCreated { content_type: ct.name.clone(), id: new_id }).await;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn get_one(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    Query(params): Query<GetParams>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentRead, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let (sql, binds) = rustapi_sql::select_by_id(&ct.name, id)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_optional(&state.pool).await.map_err(db)?;
    let row = row.ok_or(ApiError(Error::NotFound))?;
    let mut map = match row_to_json(&ct, &row)? {
        Value::Object(m) => m,
        _ => unreachable!("row_to_json returns an object"),
    };
    if let Some(raw) = params.populate.as_deref() {
        // Reuse the list pipeline with a 1-row slice so forward/inverse
        // batched SELECTs apply identically to single-GET.
        let mut one = vec![std::mem::take(&mut map)];
        apply_populate(&state, &ct, raw, &mut one).await?;
        map = one.pop().unwrap_or_default();
    }
    {
        let mut one = vec![std::mem::take(&mut map)];
        crate::media_embed::apply_media_embed(&state.pool, &ct, &mut one).await.map_err(ApiError)?;
        map = one.pop().unwrap_or_default();
    }
    Ok(Json(Value::Object(map)))
}

async fn update(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    Json(body): Json<Map<String, Value>>,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;

    let ctx = WriteContext {
        content_type: &ct.name,
        operation: WriteOp::Update,
        principal: &principal,
    };
    let body = state.hooks.before_write(&ctx, body).await.map_err(ApiError)?;

    let (mut binds_map, checks, links, media_checks, media_links) = body_to_binds(&ct, body, true)?;
    verify_relation_targets_exist(&state, &checks).await?;
    verify_link_targets_exist(&state, &links).await?;
    verify_media_targets_exist(&state, &media_checks).await?;
    verify_media_link_targets_exist(&state, &media_links).await?;

    // PUT is full-replace per spec §6.2: fields absent from the body that are
    // not required get explicitly nulled. Required-with-no-default would have
    // been rejected by body_to_binds; required-with-default keeps its existing
    // value (we can't infer the default safely from the wire-encoded SQL
    // literal, so we omit and let the column keep its current value — same
    // shape as if the client had POSTed the entry without that field). For
    // relation and media fields the typed null is FieldKind::Uuid (matches the FK col).
    for f in &ct.fields {
        // Many-to-many and multiple-media fields have no column on this table;
        // skip them here. Their links are handled by write_links/write_media_links below.
        if !f.is_stored_column() {
            continue;
        }
        if !binds_map.contains_key(&f.name) && !f.required {
            let null_kind = if f.kind == rustapi_core::FieldKind::Relation
                || f.kind == rustapi_core::FieldKind::Media
            {
                rustapi_core::FieldKind::Uuid
            } else {
                f.kind
            };
            binds_map.insert(f.name.clone(), rustapi_core::BoundValue::Null(null_kind));
        }
    }

    let (sql, binds) = rustapi_sql::update(&ct, id, &binds_map)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let mut tx = state.pool.begin().await.map_err(db)?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| db_with_relation_context(e, &checks))?;
    let row = match row {
        Some(r) => r,
        None => return Err(ApiError(Error::NotFound)), // tx rolls back on drop
    };
    write_links(&mut tx, &ct.name, &links, id).await?;
    write_media_links(&mut tx, &ct.name, &media_links, id).await?;
    tx.commit().await.map_err(db)?;

    let record = row_to_json(&ct, &row)?;
    state.hooks.after_write(&ctx, &record).await.map_err(ApiError)?;
    state.events.emit(Event::EntryUpdated { content_type: ct.name.clone(), id }).await;
    Ok(Json(record))
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
            match code.as_ref() {
                "23505" => return ApiError(Error::Conflict(d.message().to_string())),
                "23503" => {
                    return ApiError(Error::RelationFkViolation {
                        constraint: d.constraint().map(|s| s.to_string()),
                    });
                }
                _ => {}
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

/// Parse `?populate=` and apply each forward/inverse pass in payload order.
/// Both passes mutate `rows` in place; failure short-circuits and propagates
/// (parsing returns 4xx, hydration internal errors return 500 via the
/// `Internal` arm).
async fn apply_populate(
    state: &AppState,
    ct: &ContentType,
    raw: &str,
    rows: &mut [Map<String, Value>],
) -> Result<(), ApiError> {
    let registry = state.schemas.registry();
    let fields = populate::parse_populate(ct, registry, raw).await?;
    for f in fields {
        match f {
            PopulateField::Forward { field_name, target } => {
                populate::apply_forward(&state.pool, registry, rows, &field_name, &target).await?;
            }
            PopulateField::Inverse {
                field_name,
                source,
                fk_col,
            } => {
                populate::apply_inverse(
                    &state.pool,
                    registry,
                    rows,
                    &field_name,
                    &source,
                    &fk_col,
                )
                .await?;
            }
            PopulateField::InverseOne {
                field_name,
                source,
                fk_col,
            } => {
                populate::apply_inverse_one(
                    &state.pool,
                    registry,
                    rows,
                    &field_name,
                    &source,
                    &fk_col,
                )
                .await?;
            }
            PopulateField::Many {
                field_name,
                join_table,
                self_col,
                other_col,
                target,
            } => {
                populate::apply_many(
                    &state.pool,
                    registry,
                    rows,
                    &field_name,
                    &join_table,
                    &self_col,
                    &other_col,
                    &target,
                )
                .await?;
            }
        }
    }
    Ok(())
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

/// Pre-check that every many-to-many target id exists, per field. Mirrors
/// `verify_relation_targets_exist` but for `LinkPlan`s. Returns 422
/// RelationTargetMissing naming the first field with any unresolved id.
async fn verify_link_targets_exist(
    state: &AppState,
    links: &[crate::entry::LinkPlan],
) -> Result<(), ApiError> {
    for plan in links {
        if plan.ids.is_empty() {
            continue;
        }
        let table = rustapi_sql::table_name(&plan.target)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        let sql = format!("SELECT id FROM {table} WHERE id = ANY($1)");
        let rows = sqlx::query(&sql).bind(&plan.ids).fetch_all(&state.pool).await.map_err(db)?;
        let mut found = std::collections::HashSet::new();
        for r in &rows {
            let id: Uuid = r.try_get("id").map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
            found.insert(id);
        }
        let missing: Vec<String> = plan
            .ids
            .iter()
            .filter(|id| !found.contains(id))
            .map(|id| id.to_string())
            .collect();
        if !missing.is_empty() {
            return Err(ApiError(Error::Validation(
                ValidationErrors::relation_target_missing(&plan.field, missing),
            )));
        }
    }
    Ok(())
}

/// Apply each present LinkPlan as a replace-set inside the given transaction:
/// delete all existing links for the owner on that field, then insert the
/// supplied target ids. Absent fields are not in `links`, so their links are
/// untouched.
async fn write_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    owner_type: &str,
    links: &[crate::entry::LinkPlan],
    owner_id: Uuid,
) -> Result<(), ApiError> {
    for plan in links {
        if !plan.present {
            continue;
        }
        let (del_sql, _) = rustapi_sql::delete_links(owner_type, &plan.field, owner_id)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        sqlx::query(&del_sql).bind(owner_id).execute(&mut **tx).await.map_err(db)?;
        if plan.ids.is_empty() {
            continue;
        }
        let (ins_sql, _) =
            rustapi_sql::insert_links(owner_type, &plan.field, &plan.target, owner_id)
                .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        sqlx::query(&ins_sql)
            .bind(owner_id)
            .bind(&plan.ids)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
    }
    Ok(())
}

/// Existence pre-check for single-media ids. All target `_media_assets`, so one
/// batched SELECT covers every check. Returns 422 naming the first field with a
/// missing id (payload order).
async fn verify_media_targets_exist(state: &AppState, checks: &[crate::entry::MediaCheck]) -> Result<(), ApiError> {
    if checks.is_empty() { return Ok(()); }
    let ids: Vec<Uuid> = checks.iter().map(|c| c.id).collect();
    let rows = sqlx::query("SELECT id FROM \"_media_assets\" WHERE id = ANY($1)").bind(&ids).fetch_all(&state.pool).await.map_err(db)?;
    let mut found = std::collections::HashSet::new();
    for r in &rows {
        let id: Uuid = r.try_get("id").map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
        found.insert(id);
    }
    let mut current_field: Option<&str> = None;
    let mut missing: Vec<String> = Vec::new();
    for c in checks {
        if !found.contains(&c.id) {
            match current_field {
                None => { current_field = Some(&c.field); missing.push(c.id.to_string()); }
                Some(name) if name == c.field => missing.push(c.id.to_string()),
                Some(_) => break,
            }
        }
    }
    if let Some(field) = current_field {
        return Err(ApiError(Error::Validation(ValidationErrors::relation_target_missing(field, missing))));
    }
    Ok(())
}

/// Existence pre-check for multiple-media ids, per field.
async fn verify_media_link_targets_exist(state: &AppState, links: &[crate::entry::MediaLinkPlan]) -> Result<(), ApiError> {
    for plan in links {
        if plan.ids.is_empty() { continue; }
        let rows = sqlx::query("SELECT id FROM \"_media_assets\" WHERE id = ANY($1)").bind(&plan.ids).fetch_all(&state.pool).await.map_err(db)?;
        let mut found = std::collections::HashSet::new();
        for r in &rows {
            let id: Uuid = r.try_get("id").map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
            found.insert(id);
        }
        let missing: Vec<String> = plan.ids.iter().filter(|id| !found.contains(id)).map(|id| id.to_string()).collect();
        if !missing.is_empty() {
            return Err(ApiError(Error::Validation(ValidationErrors::relation_target_missing(&plan.field, missing))));
        }
    }
    Ok(())
}

async fn publish_entry(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    set_publish_state(state, ct_name, id, principal, true).await
}

async fn unpublish_entry(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    set_publish_state(state, ct_name, id, principal, false).await
}

async fn set_publish_state(
    state: AppState,
    ct_name: String,
    id: Uuid,
    principal: Principal,
    publish: bool,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    if !ct.draft_publish() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single(
                "Draft & Publish is not enabled for this content type",
            ),
        )));
    }
    let (sql, binds) = if publish {
        rustapi_sql::publish(&ct.name, id)
    } else {
        rustapi_sql::unpublish(&ct.name, id)
    }
    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_optional(&state.pool).await.map_err(db)?;
    let row = row.ok_or(ApiError(Error::NotFound))?;
    state.events.emit(Event::EntryUpdated { content_type: ct.name.clone(), id }).await;
    Ok(Json(row_to_json(&ct, &row)?))
}

/// Apply each multiple-media replace-set inside the txn: clear the gallery, then
/// insert the supplied asset ids in order (position via ORDINALITY).
async fn write_media_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    owner_type: &str,
    links: &[crate::entry::MediaLinkPlan],
    owner_id: Uuid,
) -> Result<(), ApiError> {
    for plan in links {
        if !plan.present { continue; }
        let (del_sql, _) = rustapi_sql::delete_media_links(owner_type, &plan.field, owner_id)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        sqlx::query(&del_sql).bind(owner_id).execute(&mut **tx).await.map_err(db)?;
        if plan.ids.is_empty() { continue; }
        let (ins_sql, _) = rustapi_sql::insert_media_links(owner_type, &plan.field, owner_id)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        sqlx::query(&ins_sql).bind(owner_id).bind(&plan.ids).execute(&mut **tx).await.map_err(db)?;
    }
    Ok(())
}
