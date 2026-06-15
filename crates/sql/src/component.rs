//! CRUD against the `_components` table.

use rustapi_core::Field;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub uid: String,
    pub display_name: String,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub managed: bool,
}

#[derive(Debug, Clone)]
pub struct ComponentStore {
    pool: PgPool,
}

impl ComponentStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Component>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RawComponent>(
            "SELECT uid, display_name, fields, managed FROM _components ORDER BY uid",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_component()).collect())
    }

    pub async fn get(&self, uid: &str) -> Result<Option<Component>, sqlx::Error> {
        let row = sqlx::query_as::<_, RawComponent>(
            "SELECT uid, display_name, fields, managed FROM _components WHERE uid = $1",
        )
        .bind(uid)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_component()))
    }

    pub async fn create(
        &self,
        uid: &str,
        display_name: &str,
        fields: &[Field],
        managed: bool,
    ) -> Result<Component, sqlx::Error> {
        sqlx::query(
            "INSERT INTO _components (uid, display_name, fields, managed) VALUES ($1, $2, $3, $4)",
        )
        .bind(uid)
        .bind(display_name)
        .bind(sqlx::types::Json(fields))
        .bind(managed)
        .execute(&self.pool)
        .await?;
        Ok(Component {
            uid: uid.to_string(),
            display_name: display_name.to_string(),
            fields: fields.to_vec(),
            managed,
        })
    }

    pub async fn update(
        &self,
        uid: &str,
        display_name: &str,
        fields: &[Field],
        managed: bool,
    ) -> Result<Option<Component>, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE _components SET display_name = $1, fields = $2, managed = $3 WHERE uid = $4",
        )
        .bind(display_name)
        .bind(sqlx::types::Json(fields))
        .bind(managed)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        Ok(Some(Component {
            uid: uid.to_string(),
            display_name: display_name.to_string(),
            fields: fields.to_vec(),
            managed,
        }))
    }

    pub async fn delete(&self, uid: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM _components WHERE uid = $1")
            .bind(uid)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct RawComponent {
    uid: String,
    display_name: String,
    fields: sqlx::types::Json<Vec<Field>>,
    managed: bool,
}

impl RawComponent {
    fn into_component(self) -> Component {
        Component {
            uid: self.uid,
            display_name: self.display_name,
            fields: self.fields.0,
            managed: self.managed,
        }
    }
}
