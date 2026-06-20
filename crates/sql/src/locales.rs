//! `_locales` table access. A `Locale` is a code + display name; exactly one
//! row is the default (enforced here on mutation, plus a partial unique index).

use rustapi_core::Error;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Locale {
    pub code: String,
    pub name: String,
    pub is_default: bool,
    pub position: i32,
}

/// All locales ordered by position then code.
pub async fn load_all(pool: &PgPool) -> Result<Vec<Locale>, Error> {
    sqlx::query_as::<_, Locale>(
        "SELECT code, name, is_default, position FROM \"_locales\" ORDER BY position, code",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Internal(anyhow::anyhow!(e)))
}

/// One locale by code.
pub async fn get(pool: &PgPool, code: &str) -> Result<Option<Locale>, Error> {
    sqlx::query_as::<_, Locale>(
        "SELECT code, name, is_default, position FROM \"_locales\" WHERE code = $1",
    )
    .bind(code)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Internal(anyhow::anyhow!(e)))
}

/// Insert or update a locale by code. `make_default = true` flips the default
/// to this locale (clearing the previous default) in one transaction.
pub async fn upsert(
    pool: &PgPool,
    code: &str,
    name: &str,
    position: i32,
    make_default: bool,
) -> Result<Locale, Error> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    if make_default {
        sqlx::query("UPDATE \"_locales\" SET is_default = false WHERE is_default")
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    }
    let loc = sqlx::query_as::<_, Locale>(
        "INSERT INTO \"_locales\" (code, name, is_default, position) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name, \
           is_default = (\"_locales\".is_default OR EXCLUDED.is_default), \
           position = EXCLUDED.position \
         RETURNING code, name, is_default, position",
    )
    .bind(code)
    .bind(name)
    .bind(make_default)
    .bind(position)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    tx.commit()
        .await
        .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    Ok(loc)
}

/// Delete a locale by code. Rejects deleting the default (caller maps to 422).
pub async fn delete(pool: &PgPool, code: &str) -> Result<bool, Error> {
    let row = get(pool, code).await?;
    match row {
        None => Ok(false),
        Some(l) if l.is_default => Err(Error::Validation(rustapi_core::ValidationErrors::single(
            "cannot delete the default locale",
        ))),
        Some(_) => {
            let res = sqlx::query("DELETE FROM \"_locales\" WHERE code = $1")
                .bind(code)
                .execute(pool)
                .await
                .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
            Ok(res.rows_affected() > 0)
        }
    }
}

#[cfg(test)]
mod tests {
    // CRUD fns hit Postgres; behavioral coverage lives in
    // crates/bin/tests/localization.rs (Task 12). This presence test just
    // confirms the module compiles.
    #[test]
    fn module_compiles() {
        assert_eq!(2 + 2, 4);
    }
}
