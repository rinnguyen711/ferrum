//! /admin/users/* handlers (admin-only user management).

use crate::auth::{password, users};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use rustapi_core::{Action, Error, Principal, ValidationErrors};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list).post(create))
        .route("/admin/users/:id", axum::routing::patch(update).delete(remove))
}

#[derive(Serialize)]
struct UserView {
    id: Uuid,
    email: String,
    roles: Vec<String>,
}

impl From<users::UserRow> for UserView {
    fn from(u: users::UserRow) -> Self {
        UserView { id: u.id, email: u.email, roles: u.roles }
    }
}

#[derive(Deserialize)]
struct CreateBody {
    email: String,
    password: String,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Deserialize)]
struct UpdateBody {
    email: Option<String>,
    password: Option<String>,
    roles: Option<Vec<String>>,
}

/// Authz gate. Denial → 403.
async fn ensure(state: &AppState, principal: &Principal, action: Action) -> Result<(), ApiError> {
    if !state.authz.can(principal, action, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

fn principal_id(p: &Principal) -> Uuid {
    match p {
        Principal::User { id, .. } => *id,
        Principal::ApiToken { id, .. } => *id,
    }
}

fn validate_password(pw: &str) -> Result<(), ApiError> {
    if pw.len() < 8 {
        return Err(ApiError(Error::Validation(ValidationErrors::field(
            "password",
            "must be at least 8 characters",
        ))));
    }
    Ok(())
}

/// Map a unique-violation (duplicate email) to 409.
fn map_db_err(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(db) = &e {
        if db.code().as_deref() == Some("23505") {
            return ApiError(Error::Conflict("email already exists".into()));
        }
    }
    ApiError(Error::Internal(e.into()))
}

async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<UserView>>, ApiError> {
    ensure(&state, &principal, Action::UserRead).await?;
    let rows = users::list(&state.pool)
        .await
        .map_err(|e| ApiError(Error::Internal(e.into())))?;
    Ok(Json(rows.into_iter().map(UserView::from).collect()))
}

async fn create(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<UserView>), ApiError> {
    ensure(&state, &principal, Action::UserWrite).await?;
    validate_password(&body.password)?;
    let hash = password::hash(&body.password)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!("{e}"))))?;
    let row = users::create(&state.pool, &body.email, &hash, &body.roles)
        .await
        .map_err(map_db_err)?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

async fn update(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<UserView>, ApiError> {
    ensure(&state, &principal, Action::UserWrite).await?;

    // Lockout guard: cannot remove your own admin role.
    if id == principal_id(&principal) {
        if let Some(new_roles) = &body.roles {
            if !new_roles.iter().any(|r| r == "admin") {
                return Err(ApiError(Error::Conflict(
                    "cannot remove your own admin role".into(),
                )));
            }
        }
    }

    let hash = match &body.password {
        Some(pw) if !pw.is_empty() => {
            validate_password(pw)?;
            Some(
                password::hash(pw)
                    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!("{e}"))))?,
            )
        }
        _ => None,
    };

    let row = users::update(
        &state.pool,
        id,
        body.email.as_deref(),
        hash.as_deref(),
        body.roles.as_deref(),
    )
    .await
    .map_err(map_db_err)?
    .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(row.into()))
}

async fn remove(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    ensure(&state, &principal, Action::UserDelete).await?;

    // Lockout guard: cannot delete your own account.
    if id == principal_id(&principal) {
        return Err(ApiError(Error::Conflict(
            "cannot delete your own account".into(),
        )));
    }

    let removed = users::delete(&state.pool, id)
        .await
        .map_err(|e| ApiError(Error::Internal(e.into())))?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(Error::NotFound))
    }
}
