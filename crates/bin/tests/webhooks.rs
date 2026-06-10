mod common;
use common::TestApp;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

struct MockServer {
    pub base_url: String,
    pub received: Arc<Mutex<Vec<Value>>>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl MockServer {
    async fn spawn() -> Self {
        let received: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(vec![]));
        let received2 = received.clone();
        let app = axum::Router::new().route(
            "/hook",
            axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
                let r = received2.clone();
                async move {
                    r.lock().await.push(body);
                    axum::http::StatusCode::OK
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            tokio::select! {
                _ = axum::serve(listener, app) => {}
                _ = rx => {}
            }
        });
        Self {
            base_url: format!("http://{addr}"),
            received,
            _shutdown: tx,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn wait_for_delivery(&self, timeout_ms: u64) -> Vec<Value> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            let got = self.received.lock().await.clone();
            if !got.is_empty() {
                return got;
            }
            if tokio::time::Instant::now() >= deadline {
                return vec![];
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

async fn create_webhook(app: &TestApp, hook_url: &str, events: &[&str]) -> Value {
    let resp = app
        .admin(app.client.post(app.url("/admin/webhooks")))
        .json(&json!({
            "name": "test-hook",
            "url": hook_url,
            "events": events,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    resp.json().await.unwrap()
}

#[tokio::test]
async fn webhook_crud_list_create_update_delete() {
    let app = TestApp::spawn().await;
    let mock = MockServer::spawn().await;

    let hook: Value = create_webhook(&app, &mock.url("/hook"), &["entry.created"]).await;
    assert_eq!(hook["name"], "test-hook");
    assert_eq!(hook["enabled"], true);

    let id = hook["id"].as_str().unwrap();

    let list: Vec<Value> = app
        .admin(app.client.get(app.url("/admin/webhooks")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list.len(), 1);

    let resp = app
        .admin(
            app.client
                .patch(app.url(&format!("/admin/webhooks/{id}"))),
        )
        .json(&json!({
            "name": "renamed",
            "url": mock.url("/hook"),
            "events": ["entry.created", "entry.updated"],
            "enabled": false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: Value = resp.json().await.unwrap();
    assert_eq!(updated["name"], "renamed");
    assert_eq!(updated["enabled"], false);

    let resp = app
        .admin(
            app.client
                .delete(app.url(&format!("/admin/webhooks/{id}"))),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let list: Vec<Value> = app
        .admin(app.client.get(app.url("/admin/webhooks")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn worker_delivers_pending_row_to_mock_server() {
    let mock = MockServer::spawn().await;
    let app = TestApp::spawn().await;

    create_webhook(&app, &mock.url("/hook"), &["entry.created"]).await;

    let hooks: Vec<Value> = app
        .admin(app.client.get(app.url("/admin/webhooks")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let hook_id = uuid::Uuid::parse_str(hooks[0]["id"].as_str().unwrap()).unwrap();

    let payload = serde_json::json!({
        "event": "entry.created",
        "createdAt": "2026-06-10T00:00:00Z",
        "model": "article",
        "entry": {"id": uuid::Uuid::new_v4(), "title": "Hello"},
    });
    sqlx::query(
        "INSERT INTO _webhook_deliveries (webhook_id, event, payload)
         VALUES ($1, 'entry.created', $2)",
    )
    .bind(hook_id)
    .bind(&payload)
    .execute(&app.pool)
    .await
    .unwrap();

    rustapi::webhook_worker::spawn_worker(app.pool.clone());

    let delivered = mock.wait_for_delivery(8000).await;
    assert!(!delivered.is_empty(), "worker should have delivered");
    assert_eq!(delivered[0]["event"], "entry.created");
    assert_eq!(delivered[0]["model"], "article");
}

#[tokio::test]
async fn delivery_marked_failed_after_max_attempts() {
    let app = TestApp::spawn().await;
    create_webhook(&app, "http://127.0.0.1:1/unreachable", &["entry.created"]).await;

    let hooks: Vec<Value> = app
        .admin(app.client.get(app.url("/admin/webhooks")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let hook_id = uuid::Uuid::parse_str(hooks[0]["id"].as_str().unwrap()).unwrap();

    let payload = serde_json::json!({"event": "entry.created"});
    // Insert already at attempt=4 so one more push → status=failed
    sqlx::query(
        "INSERT INTO _webhook_deliveries (webhook_id, event, payload, attempt, next_try_at)
         VALUES ($1, 'entry.created', $2, 4, now())",
    )
    .bind(hook_id)
    .bind(&payload)
    .execute(&app.pool)
    .await
    .unwrap();

    rustapi::webhook_worker::spawn_worker(app.pool.clone());
    tokio::time::sleep(std::time::Duration::from_secs(8)).await;

    let rows: Vec<(String, i32)> =
        sqlx::query_as("SELECT status, attempt FROM _webhook_deliveries WHERE webhook_id=$1")
            .bind(hook_id)
            .fetch_all(&app.pool)
            .await
            .unwrap();
    assert_eq!(rows[0].0, "failed");
    assert_eq!(rows[0].1, 5);
}

#[tokio::test]
async fn disabled_webhook_gets_no_delivery_row() {
    let app = TestApp::spawn().await;
    let mock = MockServer::spawn().await;

    let hook: Value = create_webhook(&app, &mock.url("/hook"), &["entry.created"]).await;
    let id = hook["id"].as_str().unwrap();

    app.admin(
        app.client
            .patch(app.url(&format!("/admin/webhooks/{id}"))),
    )
    .json(&json!({
        "name": "test-hook",
        "url": mock.url("/hook"),
        "events": ["entry.created"],
        "enabled": false,
    }))
    .send()
    .await
    .unwrap();

    // Simulate insert_deliveries for a disabled webhook — should insert 0 rows
    sqlx::query(
        "INSERT INTO _webhook_deliveries (webhook_id, event, payload)
         SELECT id, 'entry.created', '{}'::jsonb
         FROM _webhooks WHERE enabled=true AND 'entry.created' = ANY(events)",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _webhook_deliveries")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn hmac_signature_header_present_when_secret_set() {
    let received_headers: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let rh = received_headers.clone();

    let server_app = axum::Router::new().route(
        "/hook",
        axum::routing::post(
            move |headers: axum::http::HeaderMap, axum::Json(_body): axum::Json<Value>| {
                let rh = rh.clone();
                async move {
                    let sig = headers
                        .get("x-rustapi-signature")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    rh.lock().await.push(sig);
                    axum::http::StatusCode::OK
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        tokio::select! { _ = axum::serve(listener, server_app) => {} _ = rx => {} }
    });
    let hook_url = format!("http://{addr}/hook");

    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/webhooks")))
        .json(&json!({
            "name": "signed",
            "url": hook_url,
            "events": ["entry.created"],
            "secret": "my-secret",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let hook: Value = resp.json().await.unwrap();
    let hook_id = uuid::Uuid::parse_str(hook["id"].as_str().unwrap()).unwrap();

    let payload = serde_json::json!({"event": "entry.created", "model": "article", "entry": {}});
    sqlx::query(
        "INSERT INTO _webhook_deliveries (webhook_id, event, payload)
         VALUES ($1, 'entry.created', $2)",
    )
    .bind(hook_id)
    .bind(&payload)
    .execute(&app.pool)
    .await
    .unwrap();

    rustapi::webhook_worker::spawn_worker(app.pool.clone());
    tokio::time::sleep(std::time::Duration::from_secs(8)).await;

    let sigs = received_headers.lock().await.clone();
    assert!(!sigs.is_empty(), "should have received a request");
    assert!(
        sigs[0].starts_with("sha256="),
        "signature header should start with sha256="
    );
    assert_eq!(
        sigs[0].len(),
        "sha256=".len() + 64,
        "HMAC-SHA256 hex is 64 chars"
    );

    let _ = tx;
}

#[tokio::test]
async fn delete_webhook_cascades_deliveries() {
    let app = TestApp::spawn().await;
    let hook: Value = create_webhook(&app, "http://example.com/hook", &["entry.created"]).await;
    let id = uuid::Uuid::parse_str(hook["id"].as_str().unwrap()).unwrap();

    sqlx::query(
        "INSERT INTO _webhook_deliveries (webhook_id, event, payload)
         VALUES ($1, 'entry.created', '{}'::jsonb)",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .unwrap();

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM _webhook_deliveries WHERE webhook_id=$1")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(count.0, 1);

    app.admin(
        app.client
            .delete(app.url(&format!("/admin/webhooks/{id}"))),
    )
    .send()
    .await
    .unwrap();

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM _webhook_deliveries WHERE webhook_id=$1")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(count.0, 0, "deliveries should cascade-delete");
}
