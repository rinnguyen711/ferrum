use crate::middleware::auth::require_admin_key;
use crate::state::AppState;
use axum::routing::get;
use axum::Router;

pub mod content;
pub mod health;
pub mod schema;

pub fn build_router(state: AppState) -> Router {
    let public = Router::new().route("/healthz", get(health::healthz));

    let admin = Router::new()
        .merge(schema::router())
        .merge(content::router())
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_admin_key,
        ));

    public.merge(admin).with_state(state)
}
