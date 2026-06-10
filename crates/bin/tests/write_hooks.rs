mod common;

use async_trait::async_trait;
use common::TestApp;
use rustapi_core::{Error, ValidationErrors};
use rustapi_http::{WriteContext, WriteHook, WriteOp};
use serde_json::{json, Map, Value};
use std::sync::Arc;

/// Test hook driven entirely by request data so one type covers all cases:
/// - before_write injects `slug` from `title` when `slug` is absent
/// - before_write rejects when `title` equals "REJECT"
/// - before_write injects an UNKNOWN field when `title` equals "INJECTBAD"
///   (proves the framework re-validates the hook's output)
/// - after_write returns Err when `title` equals "POSTFAIL"
struct TestHook;

#[async_trait]
impl WriteHook for TestHook {
    async fn before_write(
        &self,
        _ctx: &WriteContext<'_>,
        mut body: Map<String, Value>,
    ) -> Result<Map<String, Value>, Error> {
        if body.get("title").and_then(|v| v.as_str()) == Some("REJECT") {
            return Err(Error::Validation(ValidationErrors::single(
                "title not allowed".to_string(),
            )));
        }
        if body.get("title").and_then(|v| v.as_str()) == Some("INJECTBAD") {
            body.insert("not_a_field".to_string(), Value::Bool(true));
            return Ok(body);
        }
        if !body.contains_key("slug") {
            if let Some(title) = body.get("title").and_then(|v| v.as_str()) {
                let slug = title.to_lowercase().replace(' ', "-");
                body.insert("slug".to_string(), Value::String(slug));
            }
        }
        Ok(body)
    }

    async fn after_write(&self, _ctx: &WriteContext<'_>, record: &Value) -> Result<(), Error> {
        if record.get("title").and_then(|v| v.as_str()) == Some("POSTFAIL") {
            return Err(Error::Internal(anyhow::anyhow!("after_write failed")));
        }
        Ok(())
    }
}

async fn make_post_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true, "max_length": 64},
                {"name": "slug", "kind": "string", "max_length": 80}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn before_write_transforms_body() {
    let app = TestApp::spawn_with_hook(Arc::new(TestHook)).await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "Hello World" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let entry: Value = resp.json().await.unwrap();
    assert_eq!(entry["slug"], "hello-world", "hook should derive slug");
}

#[tokio::test]
async fn before_write_rejects_request() {
    let app = TestApp::spawn_with_hook(Arc::new(TestHook)).await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "REJECT" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422, "rejection should be a validation error");

    let resp = app
        .admin(app.client.get(app.url("/api/post")))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["meta"]["total"], 0,
        "rejected request must not persist"
    );
}

#[tokio::test]
async fn before_write_output_is_revalidated() {
    let app = TestApp::spawn_with_hook(Arc::new(TestHook)).await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "INJECTBAD" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        422,
        "injected unknown field must be rejected"
    );

    let resp = app
        .admin(app.client.get(app.url("/api/post")))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["meta"]["total"], 0,
        "invalid injected field must not persist"
    );
}

#[tokio::test]
async fn after_write_failure_returns_500_but_write_is_durable() {
    let app = TestApp::spawn_with_hook(Arc::new(TestHook)).await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "POSTFAIL" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 500, "after_write Err surfaces as 5xx");

    // The write committed before after_write ran, so the row is durable.
    // Use status=all so draft-publish types also surface the persisted draft.
    let resp = app
        .admin(app.client.get(app.url("/api/post?status=all")))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["meta"]["total"], 1,
        "write must persist despite hook error"
    );
    assert_eq!(body["data"][0]["title"], "POSTFAIL");
}

use std::sync::Mutex;

/// Records the operation seen by before_write so the test can assert
/// Create on POST and Update on PUT.
struct RecordingHook {
    ops: Arc<Mutex<Vec<WriteOp>>>,
}

#[async_trait]
impl WriteHook for RecordingHook {
    async fn before_write(
        &self,
        ctx: &WriteContext<'_>,
        body: Map<String, Value>,
    ) -> Result<Map<String, Value>, Error> {
        self.ops.lock().unwrap().push(ctx.operation);
        Ok(body)
    }
}

#[tokio::test]
async fn write_context_reports_create_then_update() {
    let ops = Arc::new(Mutex::new(Vec::new()));
    let app = TestApp::spawn_with_hook(Arc::new(RecordingHook { ops: ops.clone() })).await;
    make_post_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({ "title": "First" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let entry: Value = resp.json().await.unwrap();
    let id = entry["id"].as_str().unwrap().to_string();

    let resp = app
        .admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({ "title": "Second" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let seen = ops.lock().unwrap().clone();
    assert_eq!(seen, vec![WriteOp::Create, WriteOp::Update]);
}
