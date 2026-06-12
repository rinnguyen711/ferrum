//! Storage for custom roles and their per-content-type permissions.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoleRecord {
    pub key: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub is_system: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RolePermission {
    pub content_type: String,
    pub action: String,
}

pub async fn list_roles(pool: &PgPool) -> Result<Vec<RoleRecord>, sqlx::Error> {
    sqlx::query_as::<_, RoleRecord>(
        "SELECT key, name, description, color, is_system, created_at, updated_at
         FROM _roles ORDER BY is_system DESC, name",
    )
    .fetch_all(pool)
    .await
}

pub async fn get_role(
    pool: &PgPool,
    key: &str,
) -> Result<Option<(RoleRecord, Vec<RolePermission>)>, sqlx::Error> {
    let role = sqlx::query_as::<_, RoleRecord>(
        "SELECT key, name, description, color, is_system, created_at, updated_at
         FROM _roles WHERE key = $1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    let Some(role) = role else { return Ok(None) };
    let perms = sqlx::query_as::<_, RolePermission>(
        "SELECT content_type, action FROM _role_permissions WHERE role_key = $1
         ORDER BY content_type, action",
    )
    .bind(key)
    .fetch_all(pool)
    .await?;
    Ok(Some((role, perms)))
}

/// Inserts or updates the role row (metadata only). Bumps updated_at.
pub async fn upsert_role(
    pool: &PgPool,
    key: &str,
    name: &str,
    description: &str,
    color: &str,
    is_system: bool,
) -> Result<RoleRecord, sqlx::Error> {
    sqlx::query_as::<_, RoleRecord>(
        "INSERT INTO _roles (key, name, description, color, is_system)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (key) DO UPDATE
           SET name = EXCLUDED.name,
               description = EXCLUDED.description,
               color = EXCLUDED.color,
               updated_at = now()
         RETURNING key, name, description, color, is_system, created_at, updated_at",
    )
    .bind(key)
    .bind(name)
    .bind(description)
    .bind(color)
    .bind(is_system)
    .fetch_one(pool)
    .await
}

/// Replaces all permissions for a role in one transaction.
pub async fn set_permissions(
    pool: &PgPool,
    key: &str,
    perms: &[RolePermission],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM _role_permissions WHERE role_key = $1")
        .bind(key)
        .execute(&mut *tx)
        .await?;
    for p in perms {
        sqlx::query(
            "INSERT INTO _role_permissions (role_key, content_type, action)
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(key)
        .bind(&p.content_type)
        .bind(&p.action)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Deletes a role (and its permissions, via FK cascade). Returns true if a row
/// was removed.
pub async fn delete_role(pool: &PgPool, key: &str) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM _roles WHERE key = $1")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Loads every role's permissions, keyed by role key. Used to hydrate the cache.
pub async fn load_all(pool: &PgPool) -> Result<HashMap<String, Vec<RolePermission>>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT role_key, content_type, action FROM _role_permissions",
    )
    .fetch_all(pool)
    .await?;
    let mut out: HashMap<String, Vec<RolePermission>> = HashMap::new();
    for (key, content_type, action) in rows {
        out.entry(key).or_default().push(RolePermission { content_type, action });
    }
    Ok(out)
}
