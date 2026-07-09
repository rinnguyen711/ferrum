mod common;

use axum::extract::State;
use axum::routing::get;
use axum::Router;
use common::TestApp;
use ferrum_http::AppState;

/// A custom endpoint injected by the bin. Returns 200 with the configured API
/// version, proving the `State<AppState>` extractor resolves on an injected
/// route. Sits under `/api/_probe`, behind `require_auth`.
async fn probe(State(state): State<AppState>) -> String {
    state.config.api_version.clone()
}

fn extra() -> Vec<Router<AppState>> {
    vec![Router::new().route("/api/_probe", get(probe))]
}

#[tokio::test]
async fn injected_route_is_reachable_with_auth() {
    let app = TestApp::spawn_with_routers(extra()).await;

    let resp = app
        .admin(app.client.get(app.url("/api/_probe")))
        .send()
        .await
        .expect("probe request");

    assert_eq!(
        resp.status(),
        200,
        "authed probe should reach injected route"
    );
    assert_eq!(
        resp.text().await.expect("body"),
        "test",
        "probe returns api_version from AppState"
    );
}

#[tokio::test]
async fn injected_route_requires_auth() {
    let app = TestApp::spawn_with_routers(extra()).await;

    let resp = app
        .client
        .get(app.url("/api/_probe"))
        .send()
        .await
        .expect("probe request");

    assert_eq!(
        resp.status(),
        401,
        "unauthed probe should be rejected by require_auth"
    );
}
