//! /admin/roles — CRUD for roles and their permissions (admin-only).

use crate::error::ApiError;
use crate::routes::content::db;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use rustapi_core::{
    Action, Actor, AuditEntry, Error, Principal, RequestContext, ValidationErrors, PERM_VERBS,
};
use rustapi_sql::{
    delete_role, get_role, list_roles, set_permissions, upsert_role, RolePermission, RoleRecord,
};
use serde::{Deserialize, Serialize};

const PLUGIN_TYPES: &[&str] = &["plugin::users", "plugin::upload"];

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/roles", get(list).post(create))
        .route("/admin/roles/:key", get(get_one).put(update).delete(delete))
}

async fn ensure_admin(state: &AppState, principal: &Principal) -> Result<(), ApiError> {
    if !state.authz.can(principal, Action::UserWrite, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

#[derive(Serialize)]
struct PermissionView {
    content_type: String,
    action: String,
}

#[derive(Serialize)]
struct RoleView {
    key: String,
    name: String,
    description: String,
    color: String,
    is_system: bool,
    permissions: Vec<PermissionView>,
}

#[derive(Serialize)]
struct RoleSummary {
    key: String,
    name: String,
    description: String,
    color: String,
    is_system: bool,
    permission_count: i64,
}

#[derive(Deserialize)]
struct PermissionBody {
    content_type: String,
    action: String,
}

#[derive(Deserialize)]
struct CreateBody {
    key: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_color")]
    color: String,
    #[serde(default)]
    permissions: Vec<PermissionBody>,
}

#[derive(Deserialize)]
struct UpdateBody {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_color")]
    color: String,
    #[serde(default)]
    permissions: Vec<PermissionBody>,
}

fn default_color() -> String {
    "#52525B".to_string()
}

fn invalid(field: &str, msg: &str) -> ApiError {
    ApiError(Error::Validation(ValidationErrors::field(field, msg)))
}

fn validate_key(key: &str) -> Result<(), ApiError> {
    let ok = !key.is_empty()
        && key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !key.starts_with('-')
        && !key.ends_with('-');
    if !ok {
        return Err(invalid("key", "key must be kebab-case (a-z, 0-9, -)"));
    }
    Ok(())
}

async fn validate_permissions(
    state: &AppState,
    perms: &[PermissionBody],
) -> Result<Vec<RolePermission>, ApiError> {
    let mut out = Vec::with_capacity(perms.len());
    for p in perms {
        if !PERM_VERBS.contains(&p.action.as_str()) {
            return Err(invalid("permissions", "unknown action verb"));
        }
        let known_type = PLUGIN_TYPES.contains(&p.content_type.as_str())
            || state
                .schemas
                .registry()
                .get(&p.content_type)
                .await
                .is_some();
        if !known_type {
            return Err(invalid("permissions", "unknown content type"));
        }
        out.push(RolePermission {
            content_type: p.content_type.clone(),
            action: p.action.clone(),
        });
    }
    Ok(out)
}

fn to_view(role: RoleRecord, perms: Vec<RolePermission>) -> RoleView {
    RoleView {
        key: role.key,
        name: role.name,
        description: role.description,
        color: role.color,
        is_system: role.is_system,
        permissions: perms
            .into_iter()
            .map(|p| PermissionView {
                content_type: p.content_type,
                action: p.action,
            })
            .collect(),
    }
}

async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<RoleSummary>>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let roles = list_roles(&state.pool).await.map_err(db)?;
    let all = rustapi_sql::load_all(&state.pool).await.map_err(db)?;
    let out = roles
        .into_iter()
        .map(|r| {
            let permission_count = all.get(&r.key).map(|v| v.len()).unwrap_or(0) as i64;
            RoleSummary {
                key: r.key,
                name: r.name,
                description: r.description,
                color: r.color,
                is_system: r.is_system,
                permission_count,
            }
        })
        .collect();
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(key): Path<String>,
) -> Result<Json<RoleView>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let (role, perms) = get_role(&state.pool, &key)
        .await
        .map_err(db)?
        .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(to_view(role, perms)))
}

async fn create(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<RoleView>), ApiError> {
    ensure_admin(&state, &principal).await?;
    validate_key(&body.key)?;
    if body.name.trim().is_empty() {
        return Err(invalid("name", "name is required"));
    }
    if get_role(&state.pool, &body.key)
        .await
        .map_err(db)?
        .is_some()
    {
        return Err(invalid("key", "a role with this key already exists"));
    }
    let perms = validate_permissions(&state, &body.permissions).await?;
    let role = upsert_role(
        &state.pool,
        &body.key,
        body.name.trim(),
        &body.description,
        &body.color,
        false,
    )
    .await
    .map_err(db)?;
    set_permissions(&state.pool, &body.key, &perms)
        .await
        .map_err(db)?;
    state.roles.reload_from_db(&state.pool).await.map_err(db)?;
    let (role, perms) = get_role(&state.pool, &role.key)
        .await
        .map_err(db)?
        .ok_or(ApiError(Error::NotFound))?;
    Ok((StatusCode::CREATED, Json(to_view(role, perms))))
}

async fn update(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Extension(ctx): Extension<RequestContext>,
    Path(key): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<RoleView>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let (existing, _) = get_role(&state.pool, &key)
        .await
        .map_err(db)?
        .ok_or(ApiError(Error::NotFound))?;
    if existing.is_system {
        return Err(ApiError(Error::Forbidden));
    }
    if body.name.trim().is_empty() {
        return Err(invalid("name", "name is required"));
    }
    let perms = validate_permissions(&state, &body.permissions).await?;
    upsert_role(
        &state.pool,
        &key,
        body.name.trim(),
        &body.description,
        &body.color,
        false,
    )
    .await
    .map_err(db)?;
    set_permissions(&state.pool, &key, &perms)
        .await
        .map_err(db)?;
    state.roles.reload_from_db(&state.pool).await.map_err(db)?;
    let (role, perms) = get_role(&state.pool, &key)
        .await
        .map_err(db)?
        .ok_or(ApiError(Error::NotFound))?;
    state
        .audit
        .record(
            AuditEntry::new("role.change", Actor::from_principal(&principal, None))
                .target("role", role.key.clone(), role.name.clone())
                .ctx(ctx),
        )
        .await;
    Ok(Json(to_view(role, perms)))
}

async fn delete(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(key): Path<String>,
) -> Result<StatusCode, ApiError> {
    ensure_admin(&state, &principal).await?;
    let (existing, _) = get_role(&state.pool, &key)
        .await
        .map_err(db)?
        .ok_or(ApiError(Error::NotFound))?;
    if existing.is_system {
        return Err(ApiError(Error::Forbidden));
    }
    delete_role(&state.pool, &key).await.map_err(db)?;
    state.roles.reload_from_db(&state.pool).await.map_err(db)?;
    Ok(StatusCode::NO_CONTENT)
}
