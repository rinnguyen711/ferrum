//! Transactional schema mutations.

use crate::registry::SchemaRegistry;
use chrono::Utc;
use rustapi_core::{Cardinality, ContentType, Error, Field, NewContentType, PatchContentType, ValidationErrors};
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
        validate_relation_cross_refs(&self.registry, &payload.name, &payload.fields, false).await?;

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

        // Many-to-many fields need a join table each (created after the main
        // table so its FK to ct_<owner> resolves).
        for f in &ct.fields {
            if let Some(meta) = f.relation_meta() {
                if meta.cardinality == Cardinality::ManyToMany {
                    exec_create_join_table(&mut tx, &ct.name, &f.name, &meta.target).await?;
                }
            }
        }

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
        validate_relation_cross_refs(&self.registry, name, &payload.add_fields, true).await?;

        let mut new_fields = existing.fields.clone();
        new_fields.retain(|f| !payload.drop_fields.contains(&f.name));
        for f in &payload.add_fields {
            new_fields.push(f.clone());
        }

        let mut tx: Transaction<'_, Postgres> = self.pool.begin().await.map_err(internal)?;

        for drop_name in &payload.drop_fields {
            // Find the field being dropped on the existing type to learn its kind.
            let dropped = existing.fields.iter().find(|f| &f.name == drop_name);
            let is_m2m = dropped
                .and_then(|f| f.relation_meta())
                .map(|m| m.cardinality == Cardinality::ManyToMany)
                .unwrap_or(false);
            if is_m2m {
                let sql = rustapi_sql::drop_join_table(name, drop_name)
                    .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
            } else {
                let sql = rustapi_sql::drop_column(name, drop_name)
                    .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
            }
        }
        for f in &payload.add_fields {
            if let Some(meta) = f.relation_meta() {
                if meta.cardinality == Cardinality::ManyToMany {
                    exec_create_join_table(&mut tx, name, &f.name, &meta.target).await?;
                    continue;
                }
            }
            let sql = rustapi_sql::add_column(name, f)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }

        for ext in &payload.extend_enum_values {
            // Compute the full values list (existing + appended). new_fields is
            // the post-mutation field list; extend is on an existing field, so
            // look it up there.
            let target = new_fields
                .iter_mut()
                .find(|f| f.name == ext.field)
                .expect("validated to exist by PatchContentType::validate");
            let mut meta = target
                .enum_meta()
                .expect("validated to be enum by PatchContentType::validate");
            meta.values.extend(ext.append.iter().cloned());
            let sql = rustapi_sql::alter_enum_values(name, &ext.field, &meta.values)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            // alter_enum_values returns two statements joined by "; ". Split and
            // execute each on the same transaction.
            for stmt in sql.split("; ").filter(|s| !s.trim().is_empty()) {
                sqlx::query(stmt).execute(&mut *tx).await.map_err(map_db_err)?;
            }
            // Update the in-memory field's kind_meta so the UPDATE below persists
            // the new values list.
            target.kind_meta = serde_json::json!({"values": meta.values});
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
        let owned = self.registry.m2m_targets(name).await;
        let referencing = self.registry.m2m_referencing(name).await;
        let drop_sql = rustapi_sql::drop_table(name)
            .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;

        let mut tx = self.pool.begin().await.map_err(internal)?;
        // Drop dependent join tables first so the main DROP TABLE has no
        // lingering FK references.
        for (field, _target) in &owned {
            let sql = rustapi_sql::drop_join_table(name, field)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }
        for (owner, field) in &referencing {
            if owner == name {
                continue; // already handled in `owned`
            }
            let sql = rustapi_sql::drop_join_table(owner, field)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }
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

/// Cross-content-type validation for relation fields. `Field::validate()` only
/// covers local shape; this pass checks rules that need the whole registry:
///
/// * relation target must exist (self-reference allowed when `target == candidate_name`)
/// * `inverse` name must not collide with an existing field on the target type
/// * `inverse` name must not collide with another source's inverse on the same target
/// * PATCH-added relation fields cannot be `required: true` (no backfill in v1)
/// * applies to all cardinalities (ManyToOne, OneToOne, ManyToMany)
pub async fn validate_relation_cross_refs(
    registry: &SchemaRegistry,
    candidate_name: &str,
    candidate_fields: &[Field],
    is_patch_add: bool,
) -> Result<(), Error> {
    for f in candidate_fields {
        let Some(meta) = f.relation_meta() else { continue };

        if is_patch_add && f.required {
            return Err(Error::Validation(ValidationErrors::field(
                &f.name,
                "relation field cannot be added as required (no backfill in v1)",
            )));
        }

        if meta.cardinality == Cardinality::ManyToMany && meta.target == candidate_name {
            return Err(Error::Validation(ValidationErrors::field(
                &f.name,
                "self-referential many_to_many is not supported",
            )));
        }

        if meta.target != candidate_name && registry.get(&meta.target).await.is_none() {
            return Err(Error::Validation(ValidationErrors::field(
                &f.name,
                format!("unknown target content type `{}`", meta.target),
            )));
        }

        if let Some(inv) = &meta.inverse {
            let target_ct = if meta.target == candidate_name {
                None
            } else {
                registry.get(&meta.target).await
            };
            if let Some(target_ct) = target_ct {
                if target_ct.fields.iter().any(|x| x.name == *inv) {
                    return Err(Error::Validation(ValidationErrors::field(
                        &f.name,
                        format!(
                            "inverse `{}` collides with existing field on target `{}`",
                            inv, meta.target
                        ),
                    )));
                }
            }
            if let Some((src, _)) = registry.inverse_lookup(&meta.target, inv).await {
                if src != candidate_name {
                    return Err(Error::Validation(ValidationErrors::field(
                        &f.name,
                        format!(
                            "inverse `{}` already registered by source `{}` on target `{}`",
                            inv, src, meta.target
                        ),
                    )));
                }
            }
        }
    }
    Ok(())
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

/// Create a many-to-many join table (table + index) inside an existing
/// transaction. Used by both `create` and `patch` add-paths.
async fn exec_create_join_table(
    tx: &mut Transaction<'_, Postgres>,
    owner: &str,
    field: &str,
    target: &str,
) -> Result<(), Error> {
    let (jt, idx) = rustapi_sql::create_join_table(owner, field, target)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    sqlx::query(&jt).execute(&mut **tx).await.map_err(map_db_err)?;
    sqlx::query(&idx).execute(&mut **tx).await.map_err(map_db_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::{ContentType, Field, FieldKind};
    use serde_json::json;

    fn user_ct() -> ContentType {
        ContentType {
            id: Uuid::new_v4(),
            name: "user".into(),
            display_name: "User".into(),
            fields: vec![Field {
                name: "name".into(),
                kind: FieldKind::String,
                required: false,
                unique: false,
                default: json!(null),
                max_length: None,
                kind_meta: json!({}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn relation_field(name: &str, target: &str, inverse: Option<&str>) -> Field {
        let mut meta = serde_json::Map::new();
        meta.insert("target".into(), json!(target));
        meta.insert("cardinality".into(), json!("many_to_one"));
        if let Some(inv) = inverse {
            meta.insert("inverse".into(), json!(inv));
        }
        Field {
            name: name.into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: serde_json::Value::Object(meta),
        }
    }

    fn assert_validation_msg(err: &Error, needle: &str) {
        match err {
            Error::Validation(v) => {
                let combined = format!(
                    "{:?} {:?}",
                    v.message,
                    v.fields.iter().map(|f| &f.reason).collect::<Vec<_>>()
                );
                assert!(
                    combined.contains(needle),
                    "expected `{needle}` in validation error, got {combined}"
                );
            }
            other => panic!("expected Error::Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_rejects_unknown_target() {
        let reg = SchemaRegistry::new();
        let fields = vec![relation_field("author", "user", None)];
        let err = validate_relation_cross_refs(&reg, "post", &fields, false)
            .await
            .unwrap_err();
        assert_validation_msg(&err, "unknown target");
    }

    #[tokio::test]
    async fn create_allows_self_reference_target() {
        let reg = SchemaRegistry::new();
        let fields = vec![relation_field("parent", "node", None)];
        validate_relation_cross_refs(&reg, "node", &fields, false)
            .await
            .expect("self-reference allowed even when registry empty");
    }

    #[tokio::test]
    async fn create_rejects_inverse_collision_with_existing_field() {
        let reg = SchemaRegistry::new();
        let mut user = user_ct();
        user.fields.push(Field {
            name: "posts".into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        });
        reg.insert(user).await;

        let fields = vec![relation_field("author", "user", Some("posts"))];
        let err = validate_relation_cross_refs(&reg, "post", &fields, false)
            .await
            .unwrap_err();
        assert_validation_msg(&err, "collides with existing field");
    }

    #[tokio::test]
    async fn create_rejects_inverse_collision_with_other_inverse() {
        let reg = SchemaRegistry::new();
        reg.insert(user_ct()).await;
        reg.insert(ContentType {
            id: Uuid::new_v4(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![relation_field("author", "user", Some("posts"))],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await;

        let fields = vec![relation_field("writer", "user", Some("posts"))];
        let err = validate_relation_cross_refs(&reg, "comment", &fields, false)
            .await
            .unwrap_err();
        assert_validation_msg(&err, "already registered by source");
    }

    #[tokio::test]
    async fn create_accepts_valid_relation_with_inverse() {
        let reg = SchemaRegistry::new();
        reg.insert(user_ct()).await;
        let fields = vec![relation_field("author", "user", Some("posts"))];
        validate_relation_cross_refs(&reg, "post", &fields, false)
            .await
            .expect("inverse name should be free on target");
    }

    #[tokio::test]
    async fn patch_add_rejects_required_relation() {
        let reg = SchemaRegistry::new();
        reg.insert(user_ct()).await;
        let mut f = relation_field("author", "user", None);
        f.required = true;
        let err = validate_relation_cross_refs(&reg, "post", &[f], true)
            .await
            .unwrap_err();
        assert_validation_msg(&err, "cannot be added as required");
    }

    #[tokio::test]
    async fn patch_add_required_check_skipped_on_create() {
        let reg = SchemaRegistry::new();
        reg.insert(user_ct()).await;
        let mut f = relation_field("author", "user", None);
        f.required = true;
        // On create (is_patch_add=false) required is allowed because the table
        // is built fresh from scratch — Task 5 emits NOT NULL accordingly.
        validate_relation_cross_refs(&reg, "post", &[f], false)
            .await
            .expect("required relation allowed on create");
    }

    #[tokio::test]
    async fn primitive_fields_ignored() {
        let reg = SchemaRegistry::new();
        let f = Field {
            name: "title".into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        };
        validate_relation_cross_refs(&reg, "post", &[f], false)
            .await
            .expect("primitive fields bypass cross-CT relation validation");
    }

    #[tokio::test]
    async fn create_rejects_self_referential_m2m() {
        let reg = SchemaRegistry::new();
        let f = Field {
            name: "friends".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({"target":"person","cardinality":"many_to_many"}),
        };
        let err = validate_relation_cross_refs(&reg, "person", &[f], false)
            .await
            .unwrap_err();
        assert_validation_msg(&err, "self-referential many_to_many");
    }

    #[tokio::test]
    async fn create_allows_self_referential_many_to_one() {
        // many_to_one self-ref stays allowed (regression guard).
        let reg = SchemaRegistry::new();
        let f = relation_field("parent", "node", None); // many_to_one helper
        validate_relation_cross_refs(&reg, "node", &[f], false)
            .await
            .expect("many_to_one self-ref allowed");
    }
}
