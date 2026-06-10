//! Bearer-JWT / API-token auth middleware.

use crate::auth::jwt;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;
use rustapi_core::{Error, Principal};
use rustapi_sql::lookup_by_hash;

pub async fn require_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(ApiError(Error::Unauthorized))?;

    // JWTs have exactly 3 segments separated by '.'.
    let principal = if bearer.chars().filter(|&c| c == '.').count() == 2 {
        // --- JWT path (unchanged) ---
        let claims = jwt::verify(state.config.jwt_secret.as_bytes(), bearer)
            .map_err(|_| ApiError(Error::Unauthorized))?;
        Principal::User {
            id: claims.sub,
            email: claims.email,
            roles: claims.roles,
        }
    } else {
        // --- API token path ---
        let token = lookup_by_hash(&state.pool, bearer)
            .await
            .map_err(|e| ApiError(Error::Internal(e.into())))?
            .ok_or(ApiError(Error::Unauthorized))?;

        // Check expiry.
        if let Some(exp) = token.expires_at {
            if exp < Utc::now() {
                return Err(ApiError(Error::Unauthorized));
            }
        }

        Principal::ApiToken {
            id: token.id,
            scopes: token.scopes,
        }
    };

    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}
