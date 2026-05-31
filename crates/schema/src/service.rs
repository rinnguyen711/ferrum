//! Transactional schema mutations.

use crate::registry::SchemaRegistry;
use chrono::Utc;
use rustapi_core::{ContentType, Error, NewContentType, PatchContentType};
use sqlx::{PgPool, Postgres, Transaction};
use tracing::instrument;
use uuid::Uuid;

#[derive(Clone)]
pub struct SchemaService {
    pool: PgPool,
    registry: SchemaRegistry,
}

impl SchemaService {
    pub fn new(pool: PgPool, registry: SchemaRegistry) -> Self {
        Self { pool, registry }
    }

    pub fn registry(&self) -> &SchemaRegistry {
        &self.registry
    }

    #[instrument(skip(self, payload), fields(name = %payload.name))]
    pub async fn create(&self, payload: NewContentType) -> Result<ContentType, Error> {
        payload.validate().map_err(Error::from)?;

        if self.registry.get(&payload.name).await.is_some() {
            return Err(Error::Conflict(format!(
                "content type `{}` already exists",
                payload.name
            )));
        }

        let id = Uuid::new_v4();
        let now = Utc::now();
        let ct = ContentType {
            id,
            name: payload.name.clone(),
            display_name: payload.display_name.clone(),
            fields: payload.fields.clone(),
            created_at: now,
            updated_at: now,
        };

        let create_table_sql = rustapi_sql::create_table(&ct)
            .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;

        let mut tx: Transaction<'_, Postgres> = self.pool.begin().await.map_err(internal)?;

        sqlx::query(
            "INSERT INTO _content_types (id, name, display_name, fields, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(ct.id)
        .bind(&ct.name)
        .bind(&ct.display_name)
        .bind(sqlx::types::Json(&ct.fields))
        .bind(ct.created_at)
        .bind(ct.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;

        sqlx::query(&create_table_sql)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;

        tx.commit().await.map_err(internal)?;

        self.registry.insert(ct.clone()).await;

        Ok(ct)
    }

    #[instrument(skip(self, payload), fields(name = %name))]
    pub async fn patch(
        &self,
        name: &str,
        payload: PatchContentType,
    ) -> Result<ContentType, Error> {
        let existing = self
            .registry
            .get(name)
            .await
            .ok_or(Error::NotFound)?;
        payload.validate(&existing).map_err(Error::from)?;

        let mut new_fields = existing.fields.clone();
        new_fields.retain(|f| !payload.drop_fields.contains(&f.name));
        for f in &payload.add_fields {
            new_fields.push(f.clone());
        }

        let mut tx: Transaction<'_, Postgres> = self.pool.begin().await.map_err(internal)?;

        for drop_name in &payload.drop_fields {
            let sql = rustapi_sql::drop_column(name, drop_name)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }
        for f in &payload.add_fields {
            let sql = rustapi_sql::add_column(name, f)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }

        let new_display = payload
            .display_name
            .clone()
            .unwrap_or_else(|| existing.display_name.clone());

        let now = Utc::now();
        sqlx::query(
            "UPDATE _content_types SET display_name = $1, fields = $2, updated_at = $3 WHERE name = $4",
        )
        .bind(&new_display)
        .bind(sqlx::types::Json(&new_fields))
        .bind(now)
        .bind(name)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;

        tx.commit().await.map_err(internal)?;

        let updated = ContentType {
            id: existing.id,
            name: existing.name.clone(),
            display_name: new_display,
            fields: new_fields,
            created_at: existing.created_at,
            updated_at: now,
        };
        self.registry.insert(updated.clone()).await;
        Ok(updated)
    }

    #[instrument(skip(self), fields(name = %name))]
    pub async fn delete(&self, name: &str) -> Result<(), Error> {
        if self.registry.get(name).await.is_none() {
            return Err(Error::NotFound);
        }
        let drop_sql = rustapi_sql::drop_table(name)
            .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;

        let mut tx = self.pool.begin().await.map_err(internal)?;
        sqlx::query(&drop_sql).execute(&mut *tx).await.map_err(map_db_err)?;
        sqlx::query("DELETE FROM _content_types WHERE name = $1")
            .bind(name)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
        tx.commit().await.map_err(internal)?;

        self.registry.remove(name).await;
        Ok(())
    }
}

fn internal(e: sqlx::Error) -> Error {
    Error::Internal(anyhow::anyhow!(e))
}

fn map_db_err(e: sqlx::Error) -> Error {
    if let sqlx::Error::Database(db) = &e {
        if let Some(code) = db.code() {
            // 23505 = unique_violation; 23514 = check_violation;
            // 23503 = fk_violation; 23502 = not_null_violation
            match code.as_ref() {
                "23505" => return Error::Conflict(db.message().to_string()),
                "23503" => {
                    // Phase 2.4: FK violations from this layer come from DELETE
                    // of a row referenced by relation FKs (children block the
                    // delete via ON DELETE RESTRICT). Write paths (entry
                    // handler) pre-check target existence and re-map any
                    // residual 23503 with field context — they bypass this
                    // mapper for the missing-target case.
                    return Error::RelationFkViolation {
                        constraint: db.constraint().map(|s| s.to_string()),
                    };
                }
                _ => {}
            }
        }
        // Per spec §5.6, surface other DB errors (DDL failures, constraint
        // violations) as 422 with the PG code + message under details.db.
        let code = db.code().map(|c| c.into_owned()).unwrap_or_default();
        return Error::Validation(rustapi_core::ValidationErrors::db(code, db.message()));
    }
    internal(e)
}
