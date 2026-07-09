//! Database-backed audit sink: writes one `_audit_log` row per recorded entry,
//! fire-and-forget. Plus a background worker that prunes old rows.

use ferrum_core::AuditEntry;
use ferrum_http::state::AuditSink;
use ferrum_sql::audit::{insert_audit, prune_audit, NewAudit};
use serde_json::json;
use sqlx::PgPool;
use std::time::Duration;

pub struct DbAuditSink {
    pool: PgPool,
}

impl DbAuditSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn changes_json(entry: &AuditEntry) -> Option<serde_json::Value> {
    if entry.changes.is_empty() {
        return None;
    }
    Some(serde_json::Value::Array(
        entry
            .changes
            .iter()
            .map(|c| json!({ "field": c.field, "from": c.from, "to": c.to }))
            .collect(),
    ))
}

#[async_trait::async_trait]
impl AuditSink for DbAuditSink {
    async fn record(&self, entry: AuditEntry) {
        let new = NewAudit {
            action: entry.action.clone(),
            category: entry.category.clone(),
            status: entry.status.clone(),
            actor_type: entry.actor.kind.as_str().to_string(),
            actor_id: entry.actor.id,
            actor_label: entry.actor.label.clone(),
            target_type: entry.target_type.clone(),
            target_id: entry.target_id.clone(),
            target_label: entry.target_label.clone(),
            changes: changes_json(&entry),
            note: entry.note.clone(),
            ip: entry.ctx.ip.clone(),
            user_agent: entry.ctx.user_agent.clone(),
            request_id: entry.ctx.request_id.clone(),
        };
        let pool = self.pool.clone();
        tokio::spawn(async move {
            if let Err(e) = insert_audit(&pool, new).await {
                tracing::error!(error = %e, "audit insert failed");
            }
        });
    }
}

/// Periodically delete audit rows older than `retention_days`.
pub fn spawn_prune_worker(pool: PgPool, retention_days: i64) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(6 * 60 * 60)); // every 6h
        loop {
            tick.tick().await;
            match prune_audit(&pool, retention_days).await {
                Ok(n) if n > 0 => tracing::info!(pruned = n, "audit log pruned"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "audit prune failed"),
            }
        }
    });
}
