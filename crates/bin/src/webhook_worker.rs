//! DbEventSink — durable webhook delivery via _webhook_deliveries queue.

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use rustapi_core::Event;
use rustapi_http::EventSink;
use rustapi_sql::{insert_deliveries, mark_delivery_failed, mark_delivery_success, poll_pending};
use sha2::Sha256;
use sqlx::PgPool;

type HmacSha256 = Hmac<Sha256>;

fn event_name(ev: &Event) -> Option<&'static str> {
    match ev {
        Event::EntryCreated { .. } => Some("entry.created"),
        Event::EntryUpdated { .. } => Some("entry.updated"),
        Event::EntryDeleted { .. } => Some("entry.deleted"),
        Event::EntryPublished { .. } => Some("entry.published"),
        Event::EntryUnpublished { .. } => Some("entry.unpublished"),
        Event::SchemaCreated { .. } | Event::SchemaUpdated { .. } | Event::SchemaDeleted { .. } => {
            None
        }
    }
}

async fn entry_payload(
    pool: &PgPool,
    content_type: &str,
    id: uuid::Uuid,
    ev_name: &str,
) -> serde_json::Value {
    if ev_name == "entry.deleted" {
        return serde_json::json!({ "id": id });
    }
    let table = match rustapi_sql::table_name(content_type) {
        Ok(t) => t,
        Err(_) => return serde_json::json!({ "id": id }),
    };
    let sql = format!("SELECT row_to_json(t) AS data FROM {table} t WHERE id = $1");
    match sqlx::query(&sql).bind(id).fetch_optional(pool).await {
        Ok(Some(row)) => {
            use sqlx::Row;
            row.try_get::<serde_json::Value, _>("data")
                .unwrap_or(serde_json::json!({ "id": id }))
        }
        _ => serde_json::json!({ "id": id }),
    }
}

pub struct DbEventSink {
    pool: PgPool,
}

impl DbEventSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventSink for DbEventSink {
    async fn emit(&self, event: Event) {
        let Some(name) = event_name(&event) else {
            return;
        };

        let (content_type, id) = match &event {
            Event::EntryCreated { content_type, id }
            | Event::EntryUpdated { content_type, id }
            | Event::EntryDeleted { content_type, id }
            | Event::EntryPublished { content_type, id }
            | Event::EntryUnpublished { content_type, id } => (content_type.clone(), *id),
            _ => return,
        };

        let entry = entry_payload(&self.pool, &content_type, id, name).await;
        let payload = serde_json::json!({
            "event": name,
            "createdAt": chrono::Utc::now(),
            "model": content_type,
            "entry": entry,
        });

        match insert_deliveries(&self.pool, name, &payload).await {
            Ok(queued) => {
                if queued > 0 {
                    tracing::info!(
                        event = name,
                        model = %content_type,
                        entry_id = %id,
                        queued,
                        "queued webhook deliveries"
                    );
                } else {
                    tracing::debug!(event = name, "no enabled webhooks subscribe to event");
                }
            }
            Err(e) => tracing::warn!(error = %e, event = name, "failed to queue webhook deliveries"),
        }
    }
}

pub fn spawn_worker(pool: PgPool) {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        loop {
            match poll_pending(&pool, 20).await {
                Ok(rows) => {
                    if !rows.is_empty() {
                        tracing::debug!(batch = rows.len(), "delivering webhook batch");
                    }
                    for row in rows {
                        let pool2 = pool.clone();
                        let client2 = client.clone();
                        tokio::task::spawn(async move {
                            let started = std::time::Instant::now();
                            let result = deliver(&client2, &row).await;
                            let latency_ms = started.elapsed().as_millis();
                            match result {
                                Ok(status) => {
                                    tracing::info!(
                                        delivery_id = %row.id,
                                        webhook_id = %row.webhook_id,
                                        event = %row.event,
                                        url = %row.url,
                                        status,
                                        latency_ms,
                                        attempt = row.attempt,
                                        "webhook delivered"
                                    );
                                    if let Err(e) = mark_delivery_success(&pool2, row.id).await {
                                        tracing::warn!(error = %e, delivery_id = %row.id, "mark_delivery_success failed");
                                    }
                                }
                                Err(msg) => {
                                    let new_attempt = row.attempt + 1;
                                    let exhausted = new_attempt >= 5;
                                    if exhausted {
                                        tracing::error!(
                                            delivery_id = %row.id,
                                            webhook_id = %row.webhook_id,
                                            event = %row.event,
                                            url = %row.url,
                                            latency_ms,
                                            attempt = row.attempt,
                                            error = %msg,
                                            "webhook delivery failed permanently (max attempts reached)"
                                        );
                                    } else {
                                        let backoff_secs = 10i64 * (1i64 << new_attempt);
                                        tracing::warn!(
                                            delivery_id = %row.id,
                                            webhook_id = %row.webhook_id,
                                            event = %row.event,
                                            url = %row.url,
                                            latency_ms,
                                            attempt = row.attempt,
                                            retry_in_secs = backoff_secs,
                                            error = %msg,
                                            "webhook delivery failed, will retry"
                                        );
                                    }
                                    if let Err(e) =
                                        mark_delivery_failed(&pool2, row.id, row.attempt, &msg)
                                            .await
                                    {
                                        tracing::warn!(error = %e, delivery_id = %row.id, "mark_delivery_failed failed");
                                    }
                                }
                            }
                        });
                    }
                }
                Err(e) => tracing::warn!(error = %e, "webhook poll_pending failed"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

/// Deliver one webhook. Returns the HTTP status code on a 2xx response;
/// otherwise an error string (non-2xx status or transport error) suitable
/// for logging and persisting as `last_error`.
async fn deliver(
    client: &reqwest::Client,
    row: &rustapi_sql::PendingDelivery,
) -> Result<u16, String> {
    let body = serde_json::to_vec(&row.payload).map_err(|e| e.to_string())?;

    let mut req = client
        .post(&row.url)
        .header("content-type", "application/json")
        .body(body.clone());

    if let Some(secret) = &row.secret {
        let sig = hmac_signature(secret, &body);
        req = req.header("x-rustapi-signature", format!("sha256={sig}"));
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if status.is_success() {
        Ok(status.as_u16())
    } else {
        Err(format!("HTTP {status}"))
    }
}

fn hmac_signature(secret: &str, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}
