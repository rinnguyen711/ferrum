//! Runtime-generated OpenAPI 3.1 spec + Swagger UI.

pub mod schema;
pub mod spec;
pub mod static_paths;

use crate::state::AppState;
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;

async fn openapi_json(State(state): State<AppState>) -> Json<Value> {
    Json(spec::build(&state).await)
}

async fn docs_ui() -> Html<&'static str> {
    Html(SWAGGER_UI_HTML)
}

/// Public (no-auth) routes for the spec JSON and the Swagger UI page.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/openapi.json", get(openapi_json))
        .route("/docs", get(docs_ui))
}

const SWAGGER_UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>rustapi — API docs</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.18.2/swagger-ui.css" />
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5.18.2/swagger-ui-bundle.js" crossorigin></script>
  <script>
    window.onload = () => {
      window.ui = SwaggerUIBundle({
        url: "/openapi.json",
        dom_id: "#swagger-ui",
      });
    };
  </script>
</body>
</html>"##;
