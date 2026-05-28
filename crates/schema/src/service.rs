//! Transactional schema mutations.

use crate::registry::SchemaRegistry;
use chrono::Utc;
use rustapi_core::{ContentType, Error, NewContentType};
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
                "23514" | "23503" | "23502" => {
                    return Error::Validation(rustapi_core::ValidationErrors::single(db.message()))
                }
                _ => {}
            }
        }
    }
    internal(e)
}
