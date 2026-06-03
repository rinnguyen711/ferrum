//! Raw sqlx access to the `_media_*` tables. Mirrors `auth/users.rs` style:
//! plain `query_as` returning typed rows, `Result<_, sqlx::Error>`.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct FolderRow {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

type FolderTuple = (Uuid, Option<Uuid>, String, DateTime<Utc>, DateTime<Utc>);

fn folder_from(t: FolderTuple) -> FolderRow {
    FolderRow { id: t.0, parent_id: t.1, name: t.2, created_at: t.3, updated_at: t.4 }
}

const FOLDER_COLS: &str = "id, parent_id, name, created_at, updated_at";

/// List folders under `parent_id` (None = root level), name-sorted.
pub async fn list_folders(pool: &PgPool, parent_id: Option<Uuid>) -> Result<Vec<FolderRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, FolderTuple>(&format!(
        "SELECT {FOLDER_COLS} FROM _media_folders \
         WHERE parent_id IS NOT DISTINCT FROM $1 ORDER BY name"
    ))
    .bind(parent_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(folder_from).collect())
}

pub async fn create_folder(pool: &PgPool, parent_id: Option<Uuid>, name: &str) -> Result<FolderRow, sqlx::Error> {
    let t = sqlx::query_as::<_, FolderTuple>(&format!(
        "INSERT INTO _media_folders (parent_id, name) VALUES ($1, $2) RETURNING {FOLDER_COLS}"
    ))
    .bind(parent_id)
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(folder_from(t))
}

pub async fn get_folder(pool: &PgPool, id: Uuid) -> Result<Option<FolderRow>, sqlx::Error> {
    let t = sqlx::query_as::<_, FolderTuple>(&format!(
        "SELECT {FOLDER_COLS} FROM _media_folders WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(t.map(folder_from))
}

/// Update name and/or parent. `None` leaves a field unchanged. `updated_at` bumped.
pub async fn update_folder(
    pool: &PgPool,
    id: Uuid,
    name: Option<&str>,
    parent_id: Option<Option<Uuid>>,
) -> Result<Option<FolderRow>, sqlx::Error> {
    let t = sqlx::query_as::<_, FolderTuple>(&format!(
        "UPDATE _media_folders SET \
            name = COALESCE($2, name), \
            parent_id = CASE WHEN $3 THEN $4 ELSE parent_id END, \
            updated_at = now() \
         WHERE id = $1 RETURNING {FOLDER_COLS}"
    ))
    .bind(id)
    .bind(name)
    .bind(parent_id.is_some())
    .bind(parent_id.flatten())
    .fetch_optional(pool)
    .await?;
    Ok(t.map(folder_from))
}

/// True if the folder has any child folder or asset.
pub async fn folder_has_children(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let (exists,): (bool,) = sqlx::query_as(
        "SELECT EXISTS ( \
            SELECT 1 FROM _media_folders WHERE parent_id = $1 \
            UNION ALL SELECT 1 FROM _media_assets WHERE folder_id = $1 \
         )",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Delete a folder. Caller must ensure it's empty. Returns true if a row was deleted.
pub async fn delete_folder(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM _media_folders WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
