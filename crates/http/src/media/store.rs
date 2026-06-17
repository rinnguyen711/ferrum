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
    FolderRow {
        id: t.0,
        parent_id: t.1,
        name: t.2,
        created_at: t.3,
        updated_at: t.4,
    }
}

const FOLDER_COLS: &str = "id, parent_id, name, created_at, updated_at";

/// List folders under `parent_id` (None = root level), name-sorted.
pub async fn list_folders(
    pool: &PgPool,
    parent_id: Option<Uuid>,
) -> Result<Vec<FolderRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, FolderTuple>(&format!(
        "SELECT {FOLDER_COLS} FROM _media_folders \
         WHERE parent_id IS NOT DISTINCT FROM $1 ORDER BY name"
    ))
    .bind(parent_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(folder_from).collect())
}

/// Every folder, name-sorted, ignoring hierarchy. Backs the UI tree builder.
pub async fn list_all_folders(pool: &PgPool) -> Result<Vec<FolderRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, FolderTuple>(&format!(
        "SELECT {FOLDER_COLS} FROM _media_folders ORDER BY name"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(folder_from).collect())
}

pub async fn create_folder(
    pool: &PgPool,
    parent_id: Option<Uuid>,
    name: &str,
) -> Result<FolderRow, sqlx::Error> {
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

#[derive(Debug, Clone)]
pub struct AssetRow {
    pub id: Uuid,
    pub folder_id: Option<Uuid>,
    pub provider: String,
    pub storage_key: String,
    pub file_name: String,
    pub alt_text: Option<String>,
    pub caption: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub original_filename: String,
    pub checksum: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

type AssetTuple = (
    Uuid,
    Option<Uuid>,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    Option<i32>,
    Option<i32>,
    String,
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
);

fn asset_from(t: AssetTuple) -> AssetRow {
    AssetRow {
        id: t.0,
        folder_id: t.1,
        provider: t.2,
        storage_key: t.3,
        file_name: t.4,
        alt_text: t.5,
        caption: t.6,
        mime_type: t.7,
        size_bytes: t.8,
        width: t.9,
        height: t.10,
        original_filename: t.11,
        checksum: t.12,
        created_at: t.13,
        updated_at: t.14,
    }
}

const ASSET_COLS: &str = "id, folder_id, provider, storage_key, file_name, alt_text, caption, \
    mime_type, size_bytes, width, height, original_filename, checksum, created_at, updated_at";

pub async fn list_assets(
    pool: &PgPool,
    folder_id: Option<Uuid>,
) -> Result<Vec<AssetRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AssetTuple>(&format!(
        "SELECT {ASSET_COLS} FROM _media_assets \
         WHERE folder_id IS NOT DISTINCT FROM $1 ORDER BY created_at DESC"
    ))
    .bind(folder_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(asset_from).collect())
}

/// Count of assets directly in each folder. Root-level assets (NULL folder_id)
/// are excluded — only folders that hold at least one asset appear.
pub async fn folder_asset_counts(pool: &PgPool) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT folder_id, COUNT(*) FROM _media_assets \
         WHERE folder_id IS NOT NULL GROUP BY folder_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_asset(pool: &PgPool, id: Uuid) -> Result<Option<AssetRow>, sqlx::Error> {
    let t = sqlx::query_as::<_, AssetTuple>(&format!(
        "SELECT {ASSET_COLS} FROM _media_assets WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(t.map(asset_from))
}

pub async fn get_assets_by_ids(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<AssetRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AssetTuple>(&format!(
        "SELECT {ASSET_COLS} FROM _media_assets WHERE id = ANY($1)"
    ))
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(asset_from).collect())
}

/// Parameters for inserting a freshly uploaded asset.
pub struct NewAsset<'a> {
    pub folder_id: Option<Uuid>,
    pub provider: &'a str,
    pub storage_key: &'a str,
    pub file_name: &'a str,
    pub mime_type: &'a str,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub original_filename: &'a str,
    pub checksum: Option<&'a str>,
}

pub async fn create_asset(pool: &PgPool, a: NewAsset<'_>) -> Result<AssetRow, sqlx::Error> {
    let t = sqlx::query_as::<_, AssetTuple>(&format!(
        "INSERT INTO _media_assets \
            (folder_id, provider, storage_key, file_name, mime_type, size_bytes, \
             width, height, original_filename, checksum) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING {ASSET_COLS}"
    ))
    .bind(a.folder_id)
    .bind(a.provider)
    .bind(a.storage_key)
    .bind(a.file_name)
    .bind(a.mime_type)
    .bind(a.size_bytes)
    .bind(a.width)
    .bind(a.height)
    .bind(a.original_filename)
    .bind(a.checksum)
    .fetch_one(pool)
    .await?;
    Ok(asset_from(t))
}

/// Update editable metadata + optional move. `None` leaves a field unchanged.
pub async fn update_asset(
    pool: &PgPool,
    id: Uuid,
    file_name: Option<&str>,
    alt_text: Option<&str>,
    caption: Option<&str>,
    folder_id: Option<Option<Uuid>>,
) -> Result<Option<AssetRow>, sqlx::Error> {
    let t = sqlx::query_as::<_, AssetTuple>(&format!(
        "UPDATE _media_assets SET \
            file_name = COALESCE($2, file_name), \
            alt_text  = COALESCE($3, alt_text), \
            caption   = COALESCE($4, caption), \
            folder_id = CASE WHEN $5 THEN $6 ELSE folder_id END, \
            updated_at = now() \
         WHERE id = $1 RETURNING {ASSET_COLS}"
    ))
    .bind(id)
    .bind(file_name)
    .bind(alt_text)
    .bind(caption)
    .bind(folder_id.is_some())
    .bind(folder_id.flatten())
    .fetch_optional(pool)
    .await?;
    Ok(t.map(asset_from))
}

/// Delete the row. Byte deletion happens in the handler via the provider.
pub async fn delete_asset(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM _media_assets WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

#[derive(Debug, Clone)]
pub struct SettingsRow {
    pub provider: String,
    pub config: serde_json::Value, // secret fields stored encrypted
}

/// Read the singleton settings row, if it exists.
pub async fn get_settings(pool: &PgPool) -> Result<Option<SettingsRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT provider, config FROM _media_settings WHERE id = TRUE",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(provider, config)| SettingsRow { provider, config }))
}

/// Upsert the singleton settings row.
pub async fn put_settings(
    pool: &PgPool,
    provider: &str,
    config: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO _media_settings (id, provider, config, updated_at) \
         VALUES (TRUE, $1, $2, now()) \
         ON CONFLICT (id) DO UPDATE SET provider = EXCLUDED.provider, \
            config = EXCLUDED.config, updated_at = now()",
    )
    .bind(provider)
    .bind(config)
    .execute(pool)
    .await?;
    Ok(())
}
