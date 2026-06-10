//! DB operations for _api_tokens.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiToken {
    pub id: Uuid,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

pub async fn insert_token(
    pool: &PgPool,
    name: &str,
    raw_token: &str,
    scopes: &[String],
    expires_at: Option<DateTime<Utc>>,
) -> Result<ApiToken, sqlx::Error> {
    let hash = hash_token(raw_token);
    sqlx::query_as::<_, ApiToken>(
        r#"
        INSERT INTO _api_tokens (name, token_hash, scopes, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, scopes, expires_at, last_used_at, created_at
        "#,
    )
    .bind(name)
    .bind(hash)
    .bind(scopes)
    .bind(expires_at)
    .fetch_one(pool)
    .await
}

pub async fn list_tokens(pool: &PgPool) -> Result<Vec<ApiToken>, sqlx::Error> {
    sqlx::query_as::<_, ApiToken>(
        r#"
        SELECT id, name, scopes, expires_at, last_used_at, created_at
        FROM _api_tokens
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn delete_token(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM _api_tokens WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Look up a token by its SHA-256 hash. On hit, update `last_used_at` to now
/// and return the row. Returns `None` if no matching token exists.
pub async fn lookup_by_hash(
    pool: &PgPool,
    raw_token: &str,
) -> Result<Option<ApiToken>, sqlx::Error> {
    let hash = hash_token(raw_token);
    sqlx::query_as::<_, ApiToken>(
        r#"
        UPDATE _api_tokens
        SET last_used_at = now()
        WHERE token_hash = $1
        RETURNING id, name, scopes, expires_at, last_used_at, created_at
        "#,
    )
    .bind(hash)
    .fetch_optional(pool)
    .await
}
