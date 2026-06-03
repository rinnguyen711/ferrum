//! Authentication: password hashing, JWT, user store, and /auth routes.

pub mod handlers;
pub mod jwt;
pub mod password;
pub mod users;

use crate::state::AppState;
use axum::routing::{get, post};
use axum::Router;

/// Unauthenticated auth routes (setup, login).
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/setup", post(handlers::setup))
        .route("/auth/login", post(handlers::login))
}

/// Authenticated auth routes (me).
pub fn protected_router() -> Router<AppState> {
    Router::new().route("/auth/me", get(handlers::me))
}
