use crate::auth;
use crate::middleware::auth::require_auth;
use crate::openapi;
use crate::state::AppState;
use axum::routing::get;
use axum::Router;
use std::path::Path;

pub mod components;
pub mod content;
pub mod health;
pub mod media;
pub mod schema;
pub mod users;

pub fn build_router(state: AppState, extra: Vec<Router<AppState>>) -> Router {
    let mut public = Router::new()
        .route("/healthz", get(health::healthz))
        .merge(auth::public_router());

    if state.config.docs_enabled {
        public = public.merge(openapi::router());
    }

    let mut protected = Router::new()
        .merge(schema::router())
        .merge(content::router())
        .merge(users::router())
        .merge(media::router())
        .merge(components::router())
        .merge(auth::protected_router());

    // Custom routers from the bin, merged after built-ins. Behind the same
    // require_auth layer; axum panics on a duplicate exact path+method so
    // collisions surface at startup. Static segments take precedence over
    // `:param`, so e.g. `/api/feature` does not collide with `/api/:type`.
    for r in extra {
        protected = protected.merge(r);
    }

    let protected = protected.route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        require_auth,
    ));

    public.merge(protected).with_state(state)
}

/// Mount a built admin UI at `/studio`. Falls back to `index.html` for any
/// nested path so client-side routes (e.g. `/studio/content/article/13`)
/// resolve to the SPA instead of 404. Mounted as a sub-router (not
/// `nest_service`) so the inner ServeDir's 404 doesn't leak through the
/// outer status code — its own fallback rewrites the response.
pub fn mount_studio(router: Router, dir: impl AsRef<Path>) -> Router {
    use axum::extract::Path as AxumPath;
    use axum::http::{header, StatusCode};
    use axum::response::{IntoResponse, Response};
    use axum::routing::get;

    let dir = dir.as_ref().to_path_buf();

    // Two routes:
    //   GET /studio              → serve index.html
    //   GET /studio/*rest        → if file exists → serve it, else index.html
    // Avoids the tower-http ServeDir SPA-fallback footgun (status leaks 404).
    let dir_root = dir.clone();
    let root_handler = move || {
        let p = dir_root.join("index.html");
        async move { serve_file_or_404(p, "text/html; charset=utf-8").await }
    };

    let dir_path = dir.clone();
    let path_handler = move |AxumPath(rest): AxumPath<String>| {
        let dir = dir_path.clone();
        async move {
            let safe_rest = sanitize_relative_path(&rest);
            let candidate = dir.join(&safe_rest);
            if tokio::fs::try_exists(&candidate).await.unwrap_or(false)
                && candidate.is_file()
            {
                return serve_file_or_404(candidate, guess_mime(&safe_rest)).await;
            }
            serve_file_or_404(dir.join("index.html"), "text/html; charset=utf-8").await
        }
    };

    async fn serve_file_or_404(path: std::path::PathBuf, mime: &'static str) -> Response {
        match tokio::fs::read(&path).await {
            Ok(bytes) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime)],
                bytes,
            )
                .into_response(),
            Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
        }
    }

    fn sanitize_relative_path(s: &str) -> String {
        s.split('/')
            .filter(|seg| !seg.is_empty() && *seg != "." && *seg != "..")
            .collect::<Vec<_>>()
            .join("/")
    }

    fn guess_mime(path: &str) -> &'static str {
        match path.rsplit('.').next().unwrap_or("") {
            "js" | "mjs" => "text/javascript",
            "css" => "text/css",
            "html" => "text/html; charset=utf-8",
            "json" => "application/json",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "ico" => "image/x-icon",
            "woff2" => "font/woff2",
            "woff" => "font/woff",
            "map" => "application/json",
            _ => "application/octet-stream",
        }
    }

    router
        .route("/studio", get(root_handler.clone()))
        .route("/studio/", get(root_handler))
        .route("/studio/*rest", get(path_handler))
}
