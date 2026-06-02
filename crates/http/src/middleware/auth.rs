//! Bearer-JWT auth middleware. Verifies an HS256 token and injects Principal::User.

use crate::auth::jwt;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use rustapi_core::{Error, Principal};

pub async fn require_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(ApiError(Error::Unauthorized))?;

    let claims = jwt::verify(state.config.jwt_secret.as_bytes(), token)
        .map_err(|_| ApiError(Error::Unauthorized))?;

    req.extensions_mut().insert(Principal::User {
        id: claims.sub,
        email: claims.email,
        roles: claims.roles,
    });
    Ok(next.run(req).await)
}
