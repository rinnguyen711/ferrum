mod common;
use axum::{extract::State, routing::post, Extension, Json, Router};
use common::{wait_for_audit, TestApp};
use rustapi_core::{Principal, RequestContext};
use rustapi_http::{content_api, ApiError, AppState};
use serde_json::{json, Map, Value};
use sqlx::Row;

// Stand-in for a developer's custom business endpoint: calls the public content
// service to create a `widget` entry.
async fn make_widget(
    State(st): State<AppState>,
    Extension(principal): Extension<Principal>,
    Extension(req_ctx): Extension<RequestContext>,
) -> Result<Json<Value>, ApiError> {
    let mut body = Map::new();
    body.insert("title".into(), json!("from-custom-route"));
    let rec = content_api::create_entry(&st, &principal, &req_ctx, "widget", body).await?;
    Ok(Json(rec))
}

#[tokio::test]
async fn custom_router_create_via_content_api_persists_and_audits() {
    let custom = Router::new().route("/api/make-widget", post(make_widget));
    let app = TestApp::spawn_with_routers(vec![custom]).await;

    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "widget",
            "display_name": "Widget",
            "fields": [{"name": "title", "kind": "string"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.post(app.url("/api/make-widget")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let rec: Value = resp.json().await.unwrap();
    assert_eq!(rec["title"], json!("from-custom-route"));

    let row = wait_for_audit(&app.pool, "entry.create").await;
    let target_type: Option<String> = row.try_get("target_type").ok();
    assert_eq!(target_type.as_deref(), Some("widget"));
}
