//! API key middleware. v1 produces only `Principal::Admin`.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use rustapi_core::{Error, Principal};

pub async fn require_admin_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError(Error::Unauthorized))?;

    if !constant_time_eq(key.as_bytes(), state.config.admin_key.as_bytes()) {
        return Err(ApiError(Error::Unauthorized));
    }

    // TEMP stub (replaced wholesale in Task 9 by require_auth). Lets the crate
    // compile after Principal::Admin was removed in Task 2.
    req.extensions_mut().insert(Principal::User {
        id: uuid::Uuid::nil(),
        email: String::new(),
        roles: vec!["admin".into()],
    });
    Ok(next.run(req).await)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
