//! /api/:type/* handlers.

use crate::entry::{body_to_binds, row_to_csv_record, row_to_json, RelationCheck};
use crate::error::ApiError;
use crate::populate::{self, PopulateField};
use crate::query::{parse_list, ListParams};
use crate::state::{AppState, WriteContext, WriteOp};
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rustapi_core::{Action, ContentType, Error, Event, Principal, ValidationErrors};
use rustapi_schema::bind::{bind_all, bind_all_as};
use rustapi_sql::PublishFilter;
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
        .route(
            "/api/:type/:id",
            get(get_one).put(update).delete(delete_one),
        )
        .route("/api/:type/:id/publish", axum::routing::post(publish_entry))
        .route(
            "/api/:type/:id/unpublish",
            axum::routing::post(unpublish_entry),
        )
        .route(
            "/admin/content-types/:name/entries/export",
            get(export_entries),
        )
        .route(
            "/admin/content-types/:name/entries/import",
            axum::routing::post(import_entries),
        )
}

async fn ensure(
    state: &AppState,
    principal: &Principal,
    action: Action,
    ct: &str,
) -> Result<(), ApiError> {
    if !state.authz.can(principal, action, ct).await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared, non-axum entry CRUD core. The REST handlers below are thin wrappers
// over these; GraphQL resolvers (Task 3+) call the same functions. These
// return `rustapi_core::Error` directly so both surfaces map errors as they
// see fit. Behavior is identical to the previous inline handler bodies.
// ---------------------------------------------------------------------------

/// Authorize `action` on `ct_name`, fetch the content type, and reject single
/// types (which have their own /api/single-types route). Shared preamble for
/// every collection entry operation.
async fn authorize_collection(
    state: &AppState,
    principal: &Principal,
    action: Action,
    ct_name: &str,
) -> Result<ContentType, Error> {
    if !state.authz.can(principal, action, ct_name).await {
        return Err(Error::Forbidden);
    }
    let ct = state
        .schemas
        .registry()
        .get(ct_name)
        .await
        .ok_or(Error::NotFound)?;
    if ct.kind == rustapi_core::ContentTypeKind::Single {
        return Err(Error::Validation(rustapi_core::ValidationErrors::single(
            "use /api/single-types/:name for single types",
        )));
    }
    Ok(ct)
}

pub(crate) async fn list_entries(
    state: &AppState,
    principal: &Principal,
    ct_name: &str,
    params: ListParams,
    populate: Option<&str>,
    raw_query: &str,
) -> Result<Value, Error> {
    let ct = authorize_collection(state, principal, Action::ContentRead, ct_name).await?;
    let status = params.status.clone();
    let opts = parse_list(&ct, params, state.config.page_size_max)?;
    let offset: i64 = ((opts.page - 1) as i64) * (opts.page_size as i64);

    let filter = crate::filter::parse(raw_query, &ct)?;

    let publish = if ct.draft_publish() {
        match status.as_deref() {
            Some("draft") => PublishFilter::Draft,
            Some("all") => PublishFilter::All,
            _ => PublishFilter::Published,
        }
    } else {
        PublishFilter::All
    };

    // Resolve the sort column kind once (needed to decode the cursor and to
    // read the last row's sort value for nextCursor).
    let sort_kind = sort_column_kind(&ct, &opts.sort.column);

    // Decode cursor up-front if present (400 on bad/mismatched token).
    let after = match &opts.cursor {
        Some(tok) => {
            let (val, id) = crate::cursor::decode(tok, &opts.sort, sort_kind).map_err(|_| {
                Error::Validation(ValidationErrors::single(
                    "invalid cursor (use a nextCursor token from a previous response)",
                ))
            })?;
            Some((val, id))
        }
        None => None,
    };

    let keyset_mode = opts.cursor.is_some();

    let (list_sql, list_binds) = if keyset_mode {
        rustapi_sql::select_list_keyset_status(
            &ct.name,
            &filter,
            &opts.sort,
            after,
            opts.page_size as i64,
            publish,
        )
    } else {
        rustapi_sql::select_list_status(
            &ct.name,
            &filter,
            &opts.sort,
            opts.page_size as i64,
            offset,
            publish,
        )
    }
    .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;

    let q = bind_all(sqlx::query(&list_sql), &list_binds);
    let rows = q.fetch_all(&state.pool).await.map_err(|e| db(e).0)?;

    // Compute nextCursor whenever the page is full (short page = last page).
    // This applies in both offset-based first-page requests and subsequent
    // cursor-mode requests, so clients can always switch to keyset pagination
    // after the first page.
    let next_cursor = if rows.len() == opts.page_size as usize {
        rows.last()
            .map(|r| -> Result<String, Error> {
                let last_val = read_sort_value(r, &opts.sort.column, sort_kind)?;
                let last_id: uuid::Uuid = r.try_get("id").map_err(|e| db(e).0)?;
                Ok(crate::cursor::encode(&opts.sort, &last_val, last_id))
            })
            .transpose()?
    } else {
        None
    };

    let mut maps: Vec<Map<String, Value>> = Vec::with_capacity(rows.len());
    for r in &rows {
        match row_to_json(&ct, r)? {
            Value::Object(m) => maps.push(m),
            _ => unreachable!("row_to_json returns an object"),
        }
    }

    if let Some(raw) = populate {
        apply_populate(state, &ct, raw, &mut maps)
            .await
            .map_err(|e| e.0)?;
    }
    crate::media_embed::apply_media_embed(&state.pool, &ct, &mut maps).await?;

    let data: Vec<Value> = maps.into_iter().map(Value::Object).collect();

    let total: Option<i64> = if opts.with_count {
        let (count_sql, count_binds) = rustapi_sql::count_status(&ct.name, &filter, publish)
            .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
        let cq = bind_all_as(sqlx::query_as::<_, (i64,)>(&count_sql), &count_binds);
        Some(cq.fetch_one(&state.pool).await.map_err(|e| db(e).0)?.0)
    } else {
        None
    };

    let mut meta = serde_json::Map::new();
    meta.insert("page".into(), json!(opts.page));
    meta.insert("pageSize".into(), json!(opts.page_size));
    if let Some(t) = total {
        meta.insert("total".into(), json!(t));
    }
    if next_cursor.is_some() || keyset_mode {
        meta.insert("nextCursor".into(), json!(next_cursor));
    }

    Ok(json!({ "data": data, "meta": Value::Object(meta) }))
}

pub async fn get_entry(
    state: &AppState,
    principal: &Principal,
    ct_name: &str,
    id: Uuid,
    populate: Option<&str>,
) -> Result<Value, Error> {
    let ct = authorize_collection(state, principal, Action::ContentRead, ct_name).await?;
    let (sql, binds) = rustapi_sql::select_by_id(&ct.name, id)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_optional(&state.pool).await.map_err(|e| db(e).0)?;
    let row = row.ok_or(Error::NotFound)?;
    let mut map = match row_to_json(&ct, &row)? {
        Value::Object(m) => m,
        _ => unreachable!("row_to_json returns an object"),
    };
    if let Some(raw) = populate {
        // Reuse the list pipeline with a 1-row slice so forward/inverse
        // batched SELECTs apply identically to single-GET.
        let mut one = vec![std::mem::take(&mut map)];
        apply_populate(state, &ct, raw, &mut one)
            .await
            .map_err(|e| e.0)?;
        map = one.pop().unwrap_or_default();
    }
    {
        let mut one = vec![std::mem::take(&mut map)];
        crate::media_embed::apply_media_embed(&state.pool, &ct, &mut one).await?;
        map = one.pop().unwrap_or_default();
    }
    Ok(Value::Object(map))
}

pub async fn create_entry(
    state: &AppState,
    principal: &Principal,
    req_ctx: &rustapi_core::RequestContext,
    ct_name: &str,
    body: Map<String, Value>,
) -> Result<Value, Error> {
    let ct = authorize_collection(state, principal, Action::ContentWrite, ct_name).await?;

    let ctx = WriteContext {
        content_type: &ct.name,
        operation: WriteOp::Create,
        principal,
    };
    let body = state.hooks.before_write(&ctx, body).await?;
    validate_component_fields(state, &ct, &body)
        .await
        .map_err(|e| e.0)?;

    let (binds_map, checks, links, media_checks, media_links) = body_to_binds(&ct, body, true)?;
    verify_relation_targets_exist(state, &checks)
        .await
        .map_err(|e| e.0)?;
    verify_link_targets_exist(state, &links)
        .await
        .map_err(|e| e.0)?;
    verify_media_targets_exist(state, &media_checks)
        .await
        .map_err(|e| e.0)?;
    verify_media_link_targets_exist(state, &media_links)
        .await
        .map_err(|e| e.0)?;

    let (sql, binds) = rustapi_sql::insert(&ct, &binds_map)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;

    let mut tx = state.pool.begin().await.map_err(|e| db(e).0)?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| db_with_relation_context(e, &checks).0)?;
    let record = row_to_json(&ct, &row)?;
    let new_id = record
        .get("id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| Error::Internal(anyhow::anyhow!("insert returned no id")))?;

    write_links(&mut tx, &ct.name, &links, new_id)
        .await
        .map_err(|e| e.0)?;
    write_media_links(&mut tx, &ct.name, &media_links, new_id)
        .await
        .map_err(|e| e.0)?;
    tx.commit().await.map_err(|e| db(e).0)?;

    state.hooks.after_write(&ctx, &record).await?;
    state
        .events
        .emit(Event::EntryCreated {
            content_type: ct.name.clone(),
            id: new_id,
        })
        .await;
    let label = entry_label(&record, &new_id);
    audit_content(
        state,
        principal,
        req_ctx.clone(),
        "entry.create",
        &ct.name,
        &new_id,
        label,
        vec![],
    )
    .await;
    Ok(record)
}

pub async fn update_entry(
    state: &AppState,
    principal: &Principal,
    req_ctx: &rustapi_core::RequestContext,
    ct_name: &str,
    id: Uuid,
    body: Map<String, Value>,
) -> Result<Value, Error> {
    let ct = authorize_collection(state, principal, Action::ContentWrite, ct_name).await?;

    // Snapshot the prior record (best-effort) so the handler can build a
    // field-level audit diff. Populate is omitted to keep the diff scalar.
    let prior = get_entry(state, principal, ct_name, id, None).await.ok();

    let ctx = WriteContext {
        content_type: &ct.name,
        operation: WriteOp::Update,
        principal,
    };
    let body = state.hooks.before_write(&ctx, body).await?;
    validate_component_fields(state, &ct, &body)
        .await
        .map_err(|e| e.0)?;

    let (mut binds_map, checks, links, media_checks, media_links) = body_to_binds(&ct, body, true)?;
    verify_relation_targets_exist(state, &checks)
        .await
        .map_err(|e| e.0)?;
    verify_link_targets_exist(state, &links)
        .await
        .map_err(|e| e.0)?;
    verify_media_targets_exist(state, &media_checks)
        .await
        .map_err(|e| e.0)?;
    verify_media_link_targets_exist(state, &media_links)
        .await
        .map_err(|e| e.0)?;

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
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;

    let mut tx = state.pool.begin().await.map_err(|e| db(e).0)?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| db_with_relation_context(e, &checks).0)?;
    let row = match row {
        Some(r) => r,
        None => return Err(Error::NotFound), // tx rolls back on drop
    };
    write_links(&mut tx, &ct.name, &links, id)
        .await
        .map_err(|e| e.0)?;
    write_media_links(&mut tx, &ct.name, &media_links, id)
        .await
        .map_err(|e| e.0)?;
    tx.commit().await.map_err(|e| db(e).0)?;

    let record = row_to_json(&ct, &row)?;
    state.hooks.after_write(&ctx, &record).await?;
    state
        .events
        .emit(Event::EntryUpdated {
            content_type: ct.name.clone(),
            id,
        })
        .await;
    let changes = diff_records(prior.as_ref(), &record);
    let label = entry_label(&record, &id);
    audit_content(
        state,
        principal,
        req_ctx.clone(),
        "entry.update",
        &ct.name,
        &id,
        label,
        changes,
    )
    .await;
    Ok(record)
}

pub async fn delete_entry(
    state: &AppState,
    principal: &Principal,
    req_ctx: &rustapi_core::RequestContext,
    ct_name: &str,
    id: Uuid,
) -> Result<(), Error> {
    authorize_collection(state, principal, Action::ContentDelete, ct_name).await?;
    let (sql, binds) = rustapi_sql::delete(ct_name, id)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let result = q.execute(&state.pool).await.map_err(|e| db(e).0)?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    state
        .events
        .emit(Event::EntryDeleted {
            content_type: ct_name.to_string(),
            id,
        })
        .await;
    audit_content(
        state,
        principal,
        req_ctx.clone(),
        "entry.delete",
        ct_name,
        &id,
        id.to_string(),
        vec![],
    )
    .await;
    Ok(())
}

// ---------------------------------------------------------------------------
// REST axum handlers — thin wrappers re-wrapping the shared core's Result.
// ---------------------------------------------------------------------------

async fn list(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    Query(params): Query<ListParams>,
    axum::extract::RawQuery(raw_query): axum::extract::RawQuery,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    let populate = params.populate.clone();
    let raw_query = raw_query.as_deref().unwrap_or("").to_string();
    Ok(Json(
        list_entries(
            &state,
            &principal,
            &ct_name,
            params,
            populate.as_deref(),
            &raw_query,
        )
        .await
        .map_err(ApiError)?,
    ))
}

async fn create(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    axum::extract::Extension(ctx): axum::extract::Extension<rustapi_core::RequestContext>,
    Json(body): Json<Map<String, Value>>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let record = create_entry(&state, &principal, &ctx, &ct_name, body)
        .await
        .map_err(ApiError)?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn get_one(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    Query(params): Query<GetParams>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        get_entry(&state, &principal, &ct_name, id, params.populate.as_deref())
            .await
            .map_err(ApiError)?,
    ))
}

async fn update(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    axum::extract::Extension(ctx): axum::extract::Extension<rustapi_core::RequestContext>,
    Json(body): Json<Map<String, Value>>,
) -> Result<Json<Value>, ApiError> {
    let record = update_entry(&state, &principal, &ctx, &ct_name, id, body)
        .await
        .map_err(ApiError)?;
    Ok(Json(record))
}

async fn delete_one(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    axum::extract::Extension(ctx): axum::extract::Extension<rustapi_core::RequestContext>,
) -> Result<StatusCode, ApiError> {
    delete_entry(&state, &principal, &ctx, &ct_name, id)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn db(e: sqlx::Error) -> ApiError {
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

/// Best-effort human label for an entry, from its JSON body; falls back to id.
pub(crate) fn entry_label(record: &serde_json::Value, id: &uuid::Uuid) -> String {
    for key in ["title", "name", "label", "slug"] {
        if let Some(s) = record.get(key).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    id.to_string()
}

/// Field-level diff between two entry JSON objects: any top-level key whose
/// stringified value changed. Skips internal/system keys.
fn diff_records(
    before: Option<&serde_json::Value>,
    after: &serde_json::Value,
) -> Vec<rustapi_core::FieldChange> {
    use serde_json::Value;
    fn show(v: Option<&Value>) -> String {
        match v {
            None | Some(Value::Null) => "—".to_string(),
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
        }
    }
    let skip = ["id", "created_at", "updated_at", "published_at"];
    let after_obj = match after.as_object() {
        Some(o) => o,
        None => return Vec::new(),
    };
    let before_obj = before.and_then(|b| b.as_object());
    let mut out = Vec::new();
    for (k, av) in after_obj {
        if skip.contains(&k.as_str()) {
            continue;
        }
        let bv = before_obj.and_then(|o| o.get(k));
        if bv.map(|x| show(Some(x))) != Some(show(Some(av))) {
            out.push(rustapi_core::FieldChange {
                field: k.clone(),
                from: show(bv),
                to: show(Some(av)),
            });
        }
    }
    out
}

/// Fire an audit record for a content action (best-effort).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn audit_content(
    state: &AppState,
    principal: &Principal,
    ctx: rustapi_core::RequestContext,
    action: &str,
    content_type: &str,
    id: &uuid::Uuid,
    label: String,
    changes: Vec<rustapi_core::FieldChange>,
) {
    let actor = rustapi_core::Actor::from_principal(principal, None);
    let entry = rustapi_core::AuditEntry::new(action, actor)
        .target(content_type, id.to_string(), label)
        .changes(changes)
        .ctx(ctx);
    state.audit.record(entry).await;
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
                populate::apply_inverse(&state.pool, registry, rows, &field_name, &source, &fk_col)
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
            let id: Uuid = r
                .try_get("id")
                .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
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
        let rows = sqlx::query(&sql)
            .bind(&plan.ids)
            .fetch_all(&state.pool)
            .await
            .map_err(db)?;
        let mut found = std::collections::HashSet::new();
        for r in &rows {
            let id: Uuid = r
                .try_get("id")
                .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
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
        sqlx::query(&del_sql)
            .bind(owner_id)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
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
async fn verify_media_targets_exist(
    state: &AppState,
    checks: &[crate::entry::MediaCheck],
) -> Result<(), ApiError> {
    if checks.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = checks.iter().map(|c| c.id).collect();
    let rows = sqlx::query("SELECT id FROM \"_media_assets\" WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_all(&state.pool)
        .await
        .map_err(db)?;
    let mut found = std::collections::HashSet::new();
    for r in &rows {
        let id: Uuid = r
            .try_get("id")
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
        found.insert(id);
    }
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

/// Existence pre-check for multiple-media ids, per field.
async fn verify_media_link_targets_exist(
    state: &AppState,
    links: &[crate::entry::MediaLinkPlan],
) -> Result<(), ApiError> {
    for plan in links {
        if plan.ids.is_empty() {
            continue;
        }
        let rows = sqlx::query("SELECT id FROM \"_media_assets\" WHERE id = ANY($1)")
            .bind(&plan.ids)
            .fetch_all(&state.pool)
            .await
            .map_err(db)?;
        let mut found = std::collections::HashSet::new();
        for r in &rows {
            let id: Uuid = r
                .try_get("id")
                .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
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

async fn publish_entry(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    axum::extract::Extension(ctx): axum::extract::Extension<rustapi_core::RequestContext>,
) -> Result<Json<Value>, ApiError> {
    set_publish_state(state, ct_name, id, principal, ctx, true).await
}

async fn unpublish_entry(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    axum::extract::Extension(ctx): axum::extract::Extension<rustapi_core::RequestContext>,
) -> Result<Json<Value>, ApiError> {
    set_publish_state(state, ct_name, id, principal, ctx, false).await
}

async fn set_publish_state(
    state: AppState,
    ct_name: String,
    id: Uuid,
    principal: Principal,
    ctx: rustapi_core::RequestContext,
    publish: bool,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state
        .schemas
        .registry()
        .get(&ct_name)
        .await
        .ok_or(ApiError(Error::NotFound))?;
    if ct.kind == rustapi_core::ContentTypeKind::Single {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single("use /api/single-types/:name for single types"),
        )));
    }
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
    let ev = if publish {
        Event::EntryPublished {
            content_type: ct.name.clone(),
            id,
        }
    } else {
        Event::EntryUnpublished {
            content_type: ct.name.clone(),
            id,
        }
    };
    state.events.emit(ev).await;
    let record = row_to_json(&ct, &row)?;
    let label = entry_label(&record, &id);
    let (action, change) = if publish {
        (
            "entry.publish",
            rustapi_core::FieldChange {
                field: "status".into(),
                from: "—".into(),
                to: "Published".into(),
            },
        )
    } else {
        (
            "entry.unpublish",
            rustapi_core::FieldChange {
                field: "status".into(),
                from: "—".into(),
                to: "Draft".into(),
            },
        )
    };
    audit_content(
        &state,
        &principal,
        ctx,
        action,
        &ct.name,
        &id,
        label,
        vec![change],
    )
    .await;
    Ok(Json(record))
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
        if !plan.present {
            continue;
        }
        let (del_sql, _) = rustapi_sql::delete_media_links(owner_type, &plan.field, owner_id)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        sqlx::query(&del_sql)
            .bind(owner_id)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        if plan.ids.is_empty() {
            continue;
        }
        let (ins_sql, _) = rustapi_sql::insert_media_links(owner_type, &plan.field, owner_id)
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

/// Validate all component fields in the request body against their registered
/// schemas. Called for both create and update before `body_to_binds`.
async fn validate_component_fields(
    state: &AppState,
    ct: &rustapi_core::ContentType,
    body: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), ApiError> {
    for f in &ct.fields {
        let Some(meta) = f.component_meta() else {
            continue;
        };
        let component = state.components.get(&meta.component).await.ok_or_else(|| {
            ApiError(Error::Validation(rustapi_core::ValidationErrors::field(
                &f.name,
                format!("component `{}` not found in registry", meta.component),
            )))
        })?;

        let raw = body.get(&f.name);

        if f.required && (raw.is_none() || raw == Some(&serde_json::Value::Null)) {
            return Err(ApiError(Error::Validation(
                rustapi_core::ValidationErrors::field(&f.name, "field is required"),
            )));
        }

        let Some(raw) = raw else { continue };
        if raw.is_null() {
            continue;
        }

        if meta.multiple {
            let arr = raw.as_array().ok_or_else(|| {
                ApiError(Error::Validation(rustapi_core::ValidationErrors::field(
                    &f.name,
                    "repeatable component field must be an array",
                )))
            })?;
            for (i, item) in arr.iter().enumerate() {
                validate_component_instance(
                    item,
                    &component.fields,
                    &format!("{}[{}]", f.name, i),
                )?;
            }
        } else {
            validate_component_instance(raw, &component.fields, &f.name)?;
        }
    }
    Ok(())
}

fn validate_component_instance(
    value: &serde_json::Value,
    fields: &[rustapi_core::Field],
    path_prefix: &str,
) -> Result<(), ApiError> {
    let obj = value.as_object().ok_or_else(|| {
        ApiError(Error::Validation(rustapi_core::ValidationErrors::field(
            path_prefix,
            "component instance must be an object",
        )))
    })?;

    for f in fields {
        let field_path = format!("{}.{}", path_prefix, f.name);
        let v = obj.get(&f.name).unwrap_or(&serde_json::Value::Null);

        if f.required && v.is_null() {
            return Err(ApiError(Error::Validation(
                rustapi_core::ValidationErrors::field(&field_path, "field is required"),
            )));
        }
        // Media and Relation are stored as raw JSON inside component JSONB;
        // BoundValue::from_json always rejects them, so skip coercion here.
        if !v.is_null()
            && f.kind != rustapi_core::FieldKind::Media
            && f.kind != rustapi_core::FieldKind::Relation
        {
            rustapi_core::BoundValue::from_json(f.kind, v).map_err(|_| {
                ApiError(Error::Validation(rustapi_core::ValidationErrors::field(
                    &field_path,
                    format!("invalid value for kind {:?}", f.kind),
                )))
            })?;
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct ImportRowError {
    row: usize,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ExportQuery {
    ids: Option<String>,
}

async fn export_entries(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    Query(q): Query<ExportQuery>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Response, ApiError> {
    ensure(&state, &principal, Action::ContentRead, &ct_name).await?;

    let ct = state
        .schemas
        .registry()
        .get(&ct_name)
        .await
        .ok_or(ApiError(Error::NotFound))?;

    let raw_ids: Vec<String> = q
        .ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    if raw_ids.is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single("ids required"),
        )));
    }

    let ids: Vec<uuid::Uuid> = raw_ids
        .iter()
        .map(|s| {
            uuid::Uuid::parse_str(s).map_err(|_| {
                ApiError(Error::Validation(rustapi_core::ValidationErrors::single(
                    format!("invalid uuid: {s}"),
                )))
            })
        })
        .collect::<Result<_, _>>()?;

    let sql = rustapi_sql::select_by_ids_sql(&ct.name)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let rows = sqlx::query(&sql)
        .persistent(false)
        .bind(&ids)
        .fetch_all(&state.pool)
        .await
        .map_err(db)?;

    let mut headers_written = false;
    let mut wtr = csv::WriterBuilder::new().from_writer(vec![]);

    for row in &rows {
        let obj = row_to_json(&ct, row).map_err(ApiError)?;
        let (headers, record) = row_to_csv_record(&ct, &obj);
        if !headers_written {
            wtr.write_record(&headers)
                .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
            headers_written = true;
        }
        wtr.write_record(&record)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
    }
    // Write headers-only row if no rows matched (still valid CSV with header)
    if !headers_written {
        let dummy = serde_json::json!({});
        let (headers, _) = row_to_csv_record(&ct, &dummy);
        wtr.write_record(&headers)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
    }

    let csv_bytes = wtr
        .into_inner()
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;

    let filename = format!("{ct_name}.csv");
    Ok((
        [
            (
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/csv; charset=utf-8"),
            ),
            (
                header::CONTENT_DISPOSITION,
                header::HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
                    .unwrap_or_else(|_| header::HeaderValue::from_static("attachment")),
            ),
        ],
        Body::from(csv_bytes),
    )
        .into_response())
}

// Known limitations (MVP scope):
// - WriteHook callbacks and EventSink events are not fired for imported rows.
// - Component field shape validation is not performed (raw jsonb stored as-is).
// - Many-to-many and multi-media fields are rejected per-row, not silently dropped.
// - Each row is an independent upsert; partial failures leave prior rows committed.
async fn import_entries(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;

    let ct = state
        .schemas
        .registry()
        .get(&ct_name)
        .await
        .ok_or(ApiError(Error::NotFound))?;

    // Read the CSV file from multipart
    let csv_bytes = loop {
        match multipart.next_field().await.map_err(|e| {
            ApiError(Error::Validation(rustapi_core::ValidationErrors::single(
                format!("multipart error: {e}"),
            )))
        })? {
            None => {
                return Err(ApiError(Error::Validation(
                    rustapi_core::ValidationErrors::single("empty file"),
                )))
            }
            Some(field) if field.name() == Some("file") => {
                break field
                    .bytes()
                    .await
                    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
            }
            Some(_) => continue,
        }
    };

    if csv_bytes.is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single("empty file"),
        )));
    }

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_bytes.as_ref());

    let headers: Vec<String> = rdr
        .headers()
        .map_err(|_| {
            ApiError(Error::Validation(rustapi_core::ValidationErrors::single(
                "empty file",
            )))
        })?
        .iter()
        .map(|s| s.to_string())
        .collect();

    if headers.is_empty() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single("empty file"),
        )));
    }

    let records: Vec<csv::StringRecord> = rdr
        .records()
        .collect::<Result<_, _>>()
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;

    if records.len() > 1000 {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single("too many rows"),
        )));
    }

    let mut inserted = 0usize;
    let mut updated = 0usize;
    let mut errors: Vec<ImportRowError> = vec![];

    for (row_idx, record) in records.iter().enumerate() {
        let row_num = row_idx + 2; // header = row 1, first data row = row 2

        let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (h, v) in headers.iter().zip(record.iter()) {
            map.insert(h.clone(), v.to_string());
        }

        let body = crate::entry::csv_row_to_body(&map);

        // Extract id (may be absent/null → new insert with generated UUID)
        let explicit_id: Option<uuid::Uuid> = body
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok());

        // Strip system columns before body_to_binds
        let mut content_body = body.clone();
        content_body.remove("id");
        content_body.remove("created_at");
        content_body.remove("updated_at");
        content_body.remove("published_at");

        let (binds, relation_checks, link_plans, _media_checks, media_link_plans) =
            match crate::entry::body_to_binds(&ct, content_body, false) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(ImportRowError {
                        row: row_num,
                        message: e.to_string(),
                    });
                    continue;
                }
            };

        // Reject M2M and multi-media fields
        if !link_plans.is_empty() || !media_link_plans.is_empty() {
            errors.push(ImportRowError {
                row: row_num,
                message: "import does not support many-to-many relation or multiple-media fields"
                    .to_string(),
            });
            continue;
        }

        // Validate relation FKs
        let mut relation_error: Option<String> = None;
        {
            let mut by_target: std::collections::HashMap<String, Vec<uuid::Uuid>> =
                std::collections::HashMap::new();
            for rc in &relation_checks {
                by_target.entry(rc.target.clone()).or_default().push(rc.id);
            }
            for (target, ids) in by_target {
                let table = format!("ct_{target}");
                let found: Vec<(uuid::Uuid,)> = sqlx::query_as(&format!(
                    "SELECT id FROM \"{table}\" WHERE id = ANY($1::uuid[])"
                ))
                .bind(&ids)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
                if found.len() != ids.len() {
                    relation_error = Some(format!("relation target missing in `{target}`"));
                    break;
                }
            }
        }
        if let Some(msg) = relation_error {
            errors.push(ImportRowError {
                row: row_num,
                message: msg,
            });
            continue;
        }

        let row_id = explicit_id.unwrap_or_else(uuid::Uuid::new_v4);
        let table = format!("ct_{}", ct.name);

        let mut cols: Vec<String> = vec![];
        let mut placeholders: Vec<String> = vec![];
        let mut all_binds: Vec<rustapi_core::BoundValue> = vec![];

        all_binds.push(rustapi_core::BoundValue::Uuid(row_id));

        for (i, (col, val)) in binds.iter().enumerate() {
            let ph = i + 2;
            cols.push(format!("\"{col}\""));
            placeholders.push(format!("${ph}"));
            all_binds.push(val.clone());
        }

        let insert_sql = if cols.is_empty() {
            format!(
                "INSERT INTO \"{table}\" (\"id\") VALUES ($1::uuid) \
                 ON CONFLICT (\"id\") DO UPDATE SET \"updated_at\" = now() \
                 RETURNING (xmax = 0) AS is_insert"
            )
        } else {
            let cols_s = cols.join(", ");
            let ph_s = placeholders.join(", ");
            let sets: Vec<String> = cols
                .iter()
                .zip(placeholders.iter())
                .map(|(c, p)| format!("{c} = {p}"))
                .collect();
            let sets_s = sets.join(", ");
            format!(
                "INSERT INTO \"{table}\" (\"id\", {cols_s}) VALUES ($1::uuid, {ph_s}) \
                 ON CONFLICT (\"id\") DO UPDATE SET {sets_s}, \"updated_at\" = now() \
                 RETURNING (xmax = 0) AS is_insert"
            )
        };

        let result = {
            let mut q = sqlx::query(&insert_sql).persistent(false);
            for bv in &all_binds {
                q = rustapi_schema::bind::bind_one_for_import(q, bv);
            }
            q.fetch_one(&state.pool).await
        };

        match result {
            Ok(row) => {
                let is_insert: bool = row.try_get("is_insert").unwrap_or(true);
                if is_insert {
                    inserted += 1;
                } else {
                    updated += 1;
                }
            }
            Err(e) => {
                errors.push(ImportRowError {
                    row: row_num,
                    message: e.to_string(),
                });
            }
        }
    }

    Ok(Json(serde_json::json!({
        "inserted": inserted,
        "updated": updated,
        "errors": errors,
    })))
}

/// FieldKind of a sortable column: a user field's kind, or the kind of a known
/// system column. Defaults to Datetime for the timestamp system columns.
fn sort_column_kind(ct: &ContentType, col: &str) -> rustapi_core::FieldKind {
    use rustapi_core::FieldKind;
    if let Some(f) = ct.fields.iter().find(|f| f.name == col) {
        return f.kind;
    }
    match col {
        "id" => FieldKind::Uuid,
        "created_at" | "updated_at" | "published_at" => FieldKind::Datetime,
        other => {
            tracing::warn!(
                column = other,
                "unrecognized sort column; assuming Datetime kind for cursor"
            );
            FieldKind::Datetime
        }
    }
}

/// Read the sort column's value from a fetched row as a `BoundValue`, for
/// building the next cursor. Returns an error rather than fabricating a value
/// if the column read fails (e.g. a NULL sort value) — a wrong cursor would
/// silently corrupt pagination, so failing loudly is safer.
fn read_sort_value(
    row: &sqlx::postgres::PgRow,
    col: &str,
    kind: rustapi_core::FieldKind,
) -> Result<rustapi_core::BoundValue, Error> {
    use rustapi_core::{BoundValue, FieldKind};
    use sqlx::Row;
    let map_err = |e: sqlx::Error| {
        Error::Internal(anyhow::anyhow!(
            "cannot read sort column `{col}` for cursor: {e}"
        ))
    };
    let v = match kind {
        FieldKind::Integer => BoundValue::I64(row.try_get::<i64, _>(col).map_err(map_err)?),
        FieldKind::Float => BoundValue::F64(row.try_get::<f64, _>(col).map_err(map_err)?),
        FieldKind::Boolean => BoundValue::Bool(row.try_get::<bool, _>(col).map_err(map_err)?),
        FieldKind::Datetime => {
            let dt: chrono::DateTime<chrono::Utc> = row.try_get(col).map_err(map_err)?;
            BoundValue::Str(dt.to_rfc3339())
        }
        FieldKind::Uuid => {
            let u: uuid::Uuid = row.try_get(col).map_err(map_err)?;
            BoundValue::Str(u.to_string())
        }
        _ => BoundValue::Str(row.try_get::<String, _>(col).map_err(map_err)?),
    };
    Ok(v)
}
