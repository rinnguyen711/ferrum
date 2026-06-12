//! `_users` table access.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub roles: Vec<String>,
    pub confirmed: bool,
    pub blocked: bool,
    pub created_at: DateTime<Utc>,
}

/// Column list shared by every `SELECT`/`RETURNING` that maps to `UserRow`.
const COLS: &str = "id, email, password_hash, roles, confirmed, blocked, created_at";

/// True if any user exists. Backs the public setup-status endpoint.
pub async fn any_users(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let (exists,): (bool,) = sqlx::query_as("SELECT EXISTS (SELECT 1 FROM _users)")
        .fetch_one(pool)
        .await?;
    Ok(exists)
}

/// All users, newest first.
pub async fn list(pool: &PgPool) -> Result<Vec<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {COLS} FROM _users ORDER BY created_at DESC"
    ))
    .fetch_all(pool)
    .await
}

/// Insert a user (admin-created). Distinct from `insert_first_admin`, which is
/// guarded for the empty-table setup flow.
pub async fn create(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
    roles: &[String],
) -> Result<UserRow, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!(
        "INSERT INTO _users (email, password_hash, roles) VALUES ($1, $2, $3) RETURNING {COLS}"
    ))
    .bind(email)
    .bind(password_hash)
    .bind(roles)
    .fetch_one(pool)
    .await
}

/// Update selected fields. `None` arguments are left unchanged. Returns the
/// updated row, or `None` if no user has that id. `updated_at` bumped.
#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &PgPool,
    id: Uuid,
    email: Option<&str>,
    password_hash: Option<&str>,
    roles: Option<&[String]>,
    confirmed: Option<bool>,
    blocked: Option<bool>,
) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!(
        "UPDATE _users SET \
           email = COALESCE($2, email), \
           password_hash = COALESCE($3, password_hash), \
           roles = COALESCE($4, roles), \
           confirmed = COALESCE($5, confirmed), \
           blocked = COALESCE($6, blocked), \
           updated_at = now() \
         WHERE id = $1 \
         RETURNING {COLS}"
    ))
    .bind(id)
    .bind(email)
    .bind(password_hash)
    .bind(roles)
    .bind(confirmed)
    .bind(blocked)
    .fetch_optional(pool)
    .await
}

/// Delete by id. Returns true if a row was removed.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM _users WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
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
    sqlx::query_as::<_, UserRow>(&format!(
        "INSERT INTO _users (email, password_hash, roles) \
         SELECT $1, $2, $3 WHERE NOT EXISTS (SELECT 1 FROM _users) \
         RETURNING {COLS}"
    ))
    .bind(email)
    .bind(password_hash)
    .bind(roles)
    .fetch_optional(pool)
    .await
}

/// Look up by case-insensitive email. `None` if absent.
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {COLS} FROM _users WHERE lower(email) = lower($1)"
    ))
    .bind(email)
    .fetch_optional(pool)
    .await
}
