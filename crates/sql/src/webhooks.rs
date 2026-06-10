use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Webhook {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub event: String,
    pub status: String,
    pub attempt: i32,
    pub next_try_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Pending delivery row returned by the worker poll query.
#[derive(Debug, sqlx::FromRow)]
pub struct PendingDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub url: String,
    pub secret: Option<String>,
    pub event: String,
    pub payload: serde_json::Value,
    pub attempt: i32,
}

pub async fn list_webhooks(pool: &PgPool) -> Result<Vec<Webhook>, sqlx::Error> {
    sqlx::query_as::<_, Webhook>(
        "SELECT id, name, url, events, secret, enabled, created_at
         FROM _webhooks ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
}

pub async fn insert_webhook(
    pool: &PgPool,
    name: &str,
    url: &str,
    events: &[String],
    secret: Option<&str>,
) -> Result<Webhook, sqlx::Error> {
    sqlx::query_as::<_, Webhook>(
        "INSERT INTO _webhooks (name, url, events, secret)
         VALUES ($1, $2, $3, $4)
         RETURNING id, name, url, events, secret, enabled, created_at",
    )
    .bind(name)
    .bind(url)
    .bind(events)
    .bind(secret)
    .fetch_one(pool)
    .await
}

pub async fn update_webhook(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    url: &str,
    events: &[String],
    secret: Option<&str>,
    enabled: bool,
) -> Result<Option<Webhook>, sqlx::Error> {
    sqlx::query_as::<_, Webhook>(
        "UPDATE _webhooks SET name=$2, url=$3, events=$4, secret=$5, enabled=$6
         WHERE id=$1
         RETURNING id, name, url, events, secret, enabled, created_at",
    )
    .bind(id)
    .bind(name)
    .bind(url)
    .bind(events)
    .bind(secret)
    .bind(enabled)
    .fetch_optional(pool)
    .await
}

pub async fn delete_webhook(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("DELETE FROM _webhooks WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// Insert one delivery row per enabled webhook that subscribes to `event`.
pub async fn insert_deliveries(
    pool: &PgPool,
    event: &str,
    payload: &serde_json::Value,
) -> Result<u64, sqlx::Error> {
    let r = sqlx::query(
        "INSERT INTO _webhook_deliveries (webhook_id, event, payload)
         SELECT id, $1, $2
         FROM _webhooks
         WHERE enabled = true AND $1 = ANY(events)",
    )
    .bind(event)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// Fetch up to `limit` pending deliveries whose retry time has arrived.
/// Joins `_webhooks` to get URL and secret.
pub async fn poll_pending(pool: &PgPool, limit: i64) -> Result<Vec<PendingDelivery>, sqlx::Error> {
    sqlx::query_as::<_, PendingDelivery>(
        "SELECT d.id, d.webhook_id, w.url, w.secret, d.event, d.payload, d.attempt
         FROM _webhook_deliveries d
         JOIN _webhooks w ON w.id = d.webhook_id
         WHERE d.status = 'pending' AND d.next_try_at <= now()
         ORDER BY d.next_try_at
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn mark_delivery_success(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE _webhook_deliveries SET status='success' WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Increment attempt. If attempt >= 5, mark failed permanently; else reschedule
/// with exponential backoff: next_try_at = now() + (10 * 2^attempt) seconds.
pub async fn mark_delivery_failed(
    pool: &PgPool,
    id: Uuid,
    attempt: i32,
    error: &str,
) -> Result<(), sqlx::Error> {
    let new_attempt = attempt + 1;
    if new_attempt >= 5 {
        sqlx::query(
            "UPDATE _webhook_deliveries
             SET status='failed', attempt=$2, last_error=$3
             WHERE id=$1",
        )
        .bind(id)
        .bind(new_attempt)
        .bind(error)
        .execute(pool)
        .await?;
    } else {
        let backoff_secs = 10i64 * (1i64 << new_attempt);
        sqlx::query(
            "UPDATE _webhook_deliveries
             SET attempt=$2, last_error=$3,
                 next_try_at = now() + make_interval(secs => $4)
             WHERE id=$1",
        )
        .bind(id)
        .bind(new_attempt)
        .bind(error)
        .bind(backoff_secs)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn list_deliveries(
    pool: &PgPool,
    webhook_id: Uuid,
    limit: i64,
) -> Result<Vec<WebhookDelivery>, sqlx::Error> {
    sqlx::query_as::<_, WebhookDelivery>(
        "SELECT id, webhook_id, event, status, attempt, next_try_at, last_error, created_at
         FROM _webhook_deliveries
         WHERE webhook_id = $1
         ORDER BY created_at DESC
         LIMIT $2",
    )
    .bind(webhook_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}
