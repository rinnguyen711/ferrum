//! `_users` table access.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub roles: Vec<String>,
}

/// Count users. Used by the self-closing setup endpoint.
pub async fn count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT count(*) FROM _users")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Look up by case-insensitive email. `None` if absent.
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, String, String, Vec<String>)>(
        "SELECT id, email, password_hash, roles FROM _users WHERE lower(email) = lower($1)",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .map(|opt| {
        opt.map(|(id, email, password_hash, roles)| UserRow {
            id,
            email,
            password_hash,
            roles,
        })
    })
}

/// Insert a new user. Returns the created row. Caller pre-hashes the password.
pub async fn insert(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
    roles: &[String],
) -> Result<UserRow, sqlx::Error> {
    let (id, email, password_hash, roles) =
        sqlx::query_as::<_, (Uuid, String, String, Vec<String>)>(
            "INSERT INTO _users (email, password_hash, roles) VALUES ($1, $2, $3) \
             RETURNING id, email, password_hash, roles",
        )
        .bind(email)
        .bind(password_hash)
        .bind(roles)
        .fetch_one(pool)
        .await?;
    Ok(UserRow {
        id,
        email,
        password_hash,
        roles,
    })
}
