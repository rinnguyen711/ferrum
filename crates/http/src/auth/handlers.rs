//! /auth handlers: setup, login, me.

use crate::auth::{jwt, password, users};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json};
use ferrum_core::{Error, Principal, ValidationErrors};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserView {
    pub id: uuid::Uuid,
    pub email: String,
    pub roles: Vec<String>,
}

#[derive(Serialize)]
pub struct SetupStatus {
    pub setup_required: bool,
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

/// POST /auth/setup — create the first admin. Self-closes once any user exists.
pub async fn setup(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<(StatusCode, Json<UserView>), ApiError> {
    validate_password(&body.password)?;
    let hash = password::hash(&body.password).map_err(anyhow_internal)?;
    let roles = vec!["admin".to_string()];
    let user = users::insert_first_admin(&state.pool, &body.email, &hash, &roles)
        .await
        .map_err(map_insert_err)?
        .ok_or_else(|| ApiError(Error::Conflict("setup already completed".into())))?;
    Ok((
        StatusCode::CREATED,
        Json(UserView {
            id: user.id,
            email: user.email,
            roles: user.roles,
        }),
    ))
}

/// POST /auth/login — verify creds, return a signed JWT.
pub async fn login(
    State(state): State<AppState>,
    Extension(ctx): Extension<ferrum_core::RequestContext>,
    Json(body): Json<Credentials>,
) -> Result<Json<Value>, ApiError> {
    let found = users::find_by_email(&state.pool, &body.email)
        .await
        .map_err(internal)?;

    // Always run a verify to keep timing roughly constant whether or not the
    // user exists (mitigates user enumeration).
    let ok = match &found {
        Some(u) => password::verify(&body.password, &u.password_hash),
        None => {
            let _ = password::verify(&body.password, DUMMY_HASH);
            false
        }
    };

    let user = match (ok, found) {
        (true, Some(u)) => u,
        _ => {
            let entry = ferrum_core::AuditEntry::new(
                "auth.login_failed",
                ferrum_core::Actor::system(body.email.clone()),
            )
            .target("session", body.email.clone(), body.email.clone())
            .failed()
            .note("Invalid credentials.")
            .ctx(ctx.clone());
            state.audit.record(entry).await;
            return Err(ApiError(Error::Unauthorized));
        }
    };

    let ttl = state.config.jwt_ttl_secs;
    let token = jwt::sign(
        state.config.jwt_secret.as_bytes(),
        user.id,
        &user.email,
        &user.roles,
        ttl,
    )
    .map_err(anyhow_internal)?;

    let success = ferrum_core::AuditEntry::new(
        "auth.login",
        ferrum_core::Actor {
            kind: ferrum_core::ActorKind::User,
            id: Some(user.id),
            label: user.email.clone(),
        },
    )
    .target("session", user.email.clone(), user.email.clone())
    .ctx(ctx.clone());
    state.audit.record(success).await;

    let expires_at = chrono::Utc::now().timestamp() + ttl;
    Ok(Json(json!({ "token": token, "expires_at": expires_at })))
}

/// GET /auth/me — echo the current principal.
pub async fn me(Extension(principal): Extension<Principal>) -> Json<UserView> {
    match principal {
        Principal::User { id, email, roles } => Json(UserView { id, email, roles }),
        Principal::ApiToken { id, .. } => Json(UserView {
            id,
            email: String::new(),
            roles: vec![],
        }),
    }
}

/// GET /auth/setup — public. Reports whether first-run setup is still open.
pub async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatus>, ApiError> {
    let exists = users::any_users(&state.pool).await.map_err(internal)?;
    Ok(Json(SetupStatus {
        setup_required: !exists,
    }))
}

/// A precomputed Argon2id hash used for constant-ish timing on the
/// missing-user login path (hash of "dummy-password-x").
const DUMMY_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$/ZzAQ62MXJ5N3mEUADu5Sg$1XlpdWXRE9nSLw15fTfCjN0T47dwGnct6jcE5tUN/oM";

fn internal(e: sqlx::Error) -> ApiError {
    ApiError(Error::Internal(e.into()))
}

fn anyhow_internal<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError(Error::Internal(anyhow::anyhow!("{e}")))
}

/// Map a unique-violation (duplicate email) to 409, else internal.
fn map_insert_err(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(db) = &e {
        if db.code().as_deref() == Some("23505") {
            return ApiError(Error::Conflict("email already exists".into()));
        }
    }
    ApiError(Error::Internal(e.into()))
}
