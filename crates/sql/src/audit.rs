//! Audit-log persistence: insert one immutable row, query with filters,
//! aggregate stats, and prune old rows.

use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// A row read back from `_audit_log` for the admin list view.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditRow {
    pub id: Uuid,
    pub action: String,
    pub category: String,
    pub status: String,
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub actor_label: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub target_label: Option<String>,
    pub changes: Option<Value>,
    pub note: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Fields the caller wants written.
pub struct NewAudit {
    pub action: String,
    pub category: String,
    pub status: String,
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub actor_label: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub target_label: Option<String>,
    pub changes: Option<Value>,
    pub note: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
}

pub async fn insert_audit(pool: &PgPool, a: NewAudit) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO _audit_log
           (action, category, status, actor_type, actor_id, actor_label,
            target_type, target_id, target_label, changes, note, ip, user_agent, request_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(&a.action)
    .bind(&a.category)
    .bind(&a.status)
    .bind(&a.actor_type)
    .bind(a.actor_id)
    .bind(&a.actor_label)
    .bind(&a.target_type)
    .bind(&a.target_id)
    .bind(&a.target_label)
    .bind(&a.changes)
    .bind(&a.note)
    .bind(&a.ip)
    .bind(&a.user_agent)
    .bind(&a.request_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Filters for the list query. `None`/empty means "no filter on that field".
#[derive(Default)]
pub struct AuditQuery {
    pub category: Option<String>,
    pub status: Option<String>,
    pub actor_id: Option<Uuid>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub q: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

fn row_to_audit(r: &sqlx::postgres::PgRow) -> AuditRow {
    AuditRow {
        id: r.get("id"),
        action: r.get("action"),
        category: r.get("category"),
        status: r.get("status"),
        actor_type: r.get("actor_type"),
        actor_id: r.get("actor_id"),
        actor_label: r.get("actor_label"),
        target_type: r.get("target_type"),
        target_id: r.get("target_id"),
        target_label: r.get("target_label"),
        changes: r.get("changes"),
        note: r.get("note"),
        ip: r.get("ip"),
        user_agent: r.get("user_agent"),
        request_id: r.get("request_id"),
        created_at: r.get("created_at"),
    }
}

/// Returns `(rows, total_matching)`. `total` ignores limit/offset so the UI can
/// page. Built with positional binds in a fixed order to stay injection-safe.
pub async fn query_audit(pool: &PgPool, q: &AuditQuery) -> Result<(Vec<AuditRow>, i64), sqlx::Error> {
    let mut where_sql = String::from(" WHERE 1=1");
    let mut n = 0;
    if q.category.is_some() { n += 1; where_sql.push_str(&format!(" AND category = ${n}")); }
    if q.status.is_some()   { n += 1; where_sql.push_str(&format!(" AND status = ${n}")); }
    if q.actor_id.is_some() { n += 1; where_sql.push_str(&format!(" AND actor_id = ${n}")); }
    if q.target_type.is_some() { n += 1; where_sql.push_str(&format!(" AND target_type = ${n}")); }
    if q.target_id.is_some()   { n += 1; where_sql.push_str(&format!(" AND target_id = ${n}")); }
    if q.q.is_some() {
        n += 1;
        where_sql.push_str(&format!(
            " AND (actor_label ILIKE ${n} OR target_label ILIKE ${n} OR action ILIKE ${n})"
        ));
    }

    let count_sql = format!("SELECT count(*) AS c FROM _audit_log{where_sql}");
    let list_sql = format!(
        "SELECT * FROM _audit_log{where_sql} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
        n + 1, n + 2
    );

    macro_rules! bind_where {
        ($query:expr) => {{
            let mut query = $query;
            if let Some(v) = &q.category { query = query.bind(v); }
            if let Some(v) = &q.status { query = query.bind(v); }
            if let Some(v) = &q.actor_id { query = query.bind(v); }
            if let Some(v) = &q.target_type { query = query.bind(v); }
            if let Some(v) = &q.target_id { query = query.bind(v); }
            if let Some(v) = &q.q { query = query.bind(format!("%{v}%")); }
            query
        }};
    }

    let total: i64 = {
        let query = bind_where!(sqlx::query(&count_sql));
        query.fetch_one(pool).await?.get("c")
    };

    let rows = {
        let query = bind_where!(sqlx::query(&list_sql)).bind(q.limit).bind(q.offset);
        query.fetch_all(pool).await?.iter().map(row_to_audit).collect()
    };

    Ok((rows, total))
}

/// Per-category counts over the whole table (for tab badges).
pub async fn audit_category_counts(pool: &PgPool) -> Result<Vec<(String, i64)>, sqlx::Error> {
    let rows = sqlx::query("SELECT category, count(*) AS c FROM _audit_log GROUP BY category")
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(|r| (r.get::<String, _>("category"), r.get::<i64, _>("c"))).collect())
}

/// The four stat-card numbers, all over the last 90 days.
#[derive(Debug, serde::Serialize)]
pub struct AuditStats {
    pub events_logged: i64,
    pub sign_ins: i64,
    pub failed_attempts: i64,
    pub content_changes: i64,
    pub failed_actions: i64,
}

pub async fn audit_stats(pool: &PgPool) -> Result<AuditStats, sqlx::Error> {
    let r = sqlx::query(
        "SELECT
           count(*) AS events_logged,
           count(*) FILTER (WHERE action = 'auth.login') AS sign_ins,
           count(*) FILTER (WHERE action = 'auth.login_failed') AS failed_attempts,
           count(*) FILTER (WHERE category = 'content') AS content_changes,
           count(*) FILTER (WHERE status = 'failed') AS failed_actions
         FROM _audit_log
         WHERE created_at >= now() - INTERVAL '90 days'",
    )
    .fetch_one(pool)
    .await?;
    Ok(AuditStats {
        events_logged: r.get("events_logged"),
        sign_ins: r.get("sign_ins"),
        failed_attempts: r.get("failed_attempts"),
        content_changes: r.get("content_changes"),
        failed_actions: r.get("failed_actions"),
    })
}

/// Delete rows older than `days`. Returns the number removed.
pub async fn prune_audit(pool: &PgPool, days: i64) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(&format!(
        "DELETE FROM _audit_log WHERE created_at < now() - INTERVAL '{days} days'"
    ))
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}
