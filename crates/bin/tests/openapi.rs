//! End-to-end tests for the runtime-generated OpenAPI docs.

mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn openapi_reflects_created_content_type() {
    let app = TestApp::spawn().await;

    // Create a content type via the admin API.
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "widget",
            "display_name": "Widget",
            "fields": [
                { "name": "label", "kind": "string", "required": true }
            ]
        }))
        .send()
        .await
        .expect("create content type");
    assert_eq!(resp.status(), 201, "content type should be created");

    // Fetch the spec (public, no auth).
    let doc: serde_json::Value = app
        .client
        .get(app.url("/openapi.json"))
        .send()
        .await
        .expect("openapi request")
        .json()
        .await
        .expect("openapi json");

    assert_eq!(doc["openapi"], "3.1.0");
    assert!(
        doc["paths"]["/api/widget"]["get"].is_object(),
        "dynamic path present"
    );
    assert!(
        doc["components"]["schemas"]["Widget"].is_object(),
        "response schema present"
    );
    assert!(
        doc["components"]["schemas"]["WidgetInput"]["properties"]["id"].is_null(),
        "request schema omits system id"
    );
    assert!(
        doc["paths"]["/auth/login"]["post"].is_object(),
        "static path present"
    );
}

#[tokio::test]
async fn docs_ui_served_as_html() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .get(app.url("/docs"))
        .send()
        .await
        .expect("docs request");
    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("text/html"), "got content-type {ct}");
    let body = resp.text().await.expect("body");
    assert!(body.contains("swagger-ui"), "page loads Swagger UI");
}

#[tokio::test]
async fn docs_disabled_returns_404() {
    let app = TestApp::spawn_with_docs(false).await;

    let spec = app
        .client
        .get(app.url("/openapi.json"))
        .send()
        .await
        .expect("spec request");
    assert_eq!(
        spec.status(),
        404,
        "/openapi.json must 404 when docs disabled"
    );

    let ui = app
        .client
        .get(app.url("/docs"))
        .send()
        .await
        .expect("docs request");
    assert_eq!(ui.status(), 404, "/docs must 404 when docs disabled");
}
