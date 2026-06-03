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

/// True if any user exists. Backs the public setup-status endpoint.
pub async fn any_users(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let (exists,): (bool,) = sqlx::query_as("SELECT EXISTS (SELECT 1 FROM _users)")
        .fetch_one(pool)
        .await?;
    Ok(exists)
}

/// Atomically create the first admin: inserts only when the table is empty.
/// Returns `Ok(None)` when a user already exists (setup is closed). The
/// `WHERE NOT EXISTS` guard runs in the same statement as the insert, so two
/// concurrent first-run requests cannot both succeed (the second sees the
/// first's row and inserts zero rows).
pub async fn insert_first_admin(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
    roles: &[String],
) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, (Uuid, String, String, Vec<String>)>(
        "INSERT INTO _users (email, password_hash, roles) \
         SELECT $1, $2, $3 WHERE NOT EXISTS (SELECT 1 FROM _users) \
         RETURNING id, email, password_hash, roles",
    )
    .bind(email)
    .bind(password_hash)
    .bind(roles)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, email, password_hash, roles)| UserRow {
        id,
        email,
        password_hash,
        roles,
    }))
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
