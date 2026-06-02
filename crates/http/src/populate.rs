//! Phase 2.4 populate pass. Runs AFTER the main SELECT returns rows.
//!
//! Forward populate (many_to_one): one batched SELECT per target type
//! replaces the FK uuid string under the relation field's JSON key with
//! the full target object.
//!
//! Inverse populate (virtual reverse of a registered relation): lands in
//! Task 12 with a hard per-parent cap.

use rustapi_core::{ContentType, Error, FieldKind, ValidationErrors};
use rustapi_schema::SchemaRegistry;
use serde_json::{Map, Value};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const INVERSE_LIMIT_PER_PARENT: usize = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopulateField {
    Forward {
        field_name: String,
        target: String,
    },
    Inverse {
        field_name: String,
        source: String,
        fk_col: String,
    },
    /// one_to_one inverse: at most one child (FK is UNIQUE), returned as a
    /// single object or null rather than an array.
    InverseOne {
        field_name: String,
        source: String,
        fk_col: String,
    },
    /// many_to_many (forward or inverse). `self_col`/`other_col` are the join
    /// table's owner/target columns from the *current* type's perspective.
    Many {
        field_name: String,
        join_table: String,
        self_col: String,
        other_col: String,
        target: String,
    },
}

/// Parse `?populate=foo,bar` against a content type and registry. Returns
/// the resolved fields in payload order; duplicate names are silently
/// dropped after their first occurrence. Empty input or whitespace-only
/// entries surface as a 400-level validation error via the standard
/// `ValidationErrors` taxonomy (Task 7 deliberately skipped dedicated
/// EmptyPopulate / UnknownPopulateField variants).
pub async fn parse_populate(
    ct: &ContentType,
    registry: &SchemaRegistry,
    raw: &str,
) -> Result<Vec<PopulateField>, Error> {
    if raw.is_empty() {
        return Err(Error::Validation(ValidationErrors::single(
            "populate must not be empty",
        )));
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for piece in raw.split(',') {
        let name = piece.trim();
        if name.is_empty() {
            return Err(Error::Validation(ValidationErrors::single(
                "populate must not contain empty entries",
            )));
        }
        if !seen.insert(name.to_string()) {
            continue;
        }
        if let Some(f) = ct.fields.iter().find(|f| f.name == name) {
            if f.kind == FieldKind::Relation {
                let meta = f.relation_meta().ok_or_else(|| {
                    Error::Validation(ValidationErrors::single(format!(
                        "unknown populate field `{name}`"
                    )))
                })?;
                if meta.cardinality == rustapi_core::Cardinality::ManyToMany {
                    let join_table = rustapi_sql::join_table_name(&ct.name, &f.name)
                        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                    out.push(PopulateField::Many {
                        field_name: name.to_string(),
                        join_table,
                        self_col: format!("{}_id", ct.name),
                        other_col: format!("{}_id", meta.target),
                        target: meta.target,
                    });
                } else {
                    out.push(PopulateField::Forward {
                        field_name: name.to_string(),
                        target: meta.target,
                    });
                }
                continue;
            }
        }
        if let Some((source, fk_col)) = registry.inverse_lookup(&ct.name, name).await {
            // inverse_lookup is cardinality-agnostic; check whether the source
            // relation is M2M — if so skip this hit and fall through to
            // inverse_lookup_m2m below.
            let cardinality = registry
                .get(&source)
                .await
                .and_then(|src| {
                    src.fields.iter().find_map(|f| {
                        f.relation_meta().and_then(|m| {
                            if m.target == ct.name && m.inverse.as_deref() == Some(name) {
                                Some(m.cardinality)
                            } else {
                                None
                            }
                        })
                    })
                });
            match cardinality {
                Some(rustapi_core::Cardinality::ManyToMany) => {
                    // Fall through to inverse_lookup_m2m.
                }
                Some(rustapi_core::Cardinality::OneToOne) => {
                    out.push(PopulateField::InverseOne {
                        field_name: name.to_string(),
                        source,
                        fk_col,
                    });
                    continue;
                }
                _ => {
                    // many_to_one (or unknown — treat as array inverse).
                    out.push(PopulateField::Inverse {
                        field_name: name.to_string(),
                        source,
                        fk_col,
                    });
                    continue;
                }
            }
        }
        if let Some((owner, field)) = registry.inverse_lookup_m2m(&ct.name, name).await {
            let join_table = rustapi_sql::join_table_name(&owner, &field)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            // Current type is the *target* of the M:N; in the join table its own
            // id column is `<ct.name>_id`, children rows are `<owner>_id`.
            out.push(PopulateField::Many {
                field_name: name.to_string(),
                join_table,
                self_col: format!("{}_id", ct.name),
                other_col: format!("{owner}_id"),
                target: owner,
            });
            continue;
        }
        return Err(Error::Validation(ValidationErrors::single(format!(
            "unknown populate field `{name}`"
        ))));
    }
    Ok(out)
}

/// Hydrate one forward relation in-place on `rows`. The relation field on
/// each row carries a uuid string from the main SELECT; replace with the
/// full target object, or leave unchanged when the target id doesn't
/// resolve (deleted concurrently; pre-check is a different code path).
pub async fn apply_forward(
    pool: &PgPool,
    registry: &SchemaRegistry,
    rows: &mut [Map<String, Value>],
    field_name: &str,
    target: &str,
) -> Result<(), Error> {
    let target_ct = registry.get(target).await.ok_or_else(|| {
        Error::Internal(anyhow::anyhow!("populate target vanished: {target}"))
    })?;
    let mut ids: Vec<Uuid> = Vec::new();
    let mut seen: HashSet<Uuid> = HashSet::new();
    for r in rows.iter() {
        if let Some(Value::String(s)) = r.get(field_name) {
            if let Ok(u) = Uuid::parse_str(s) {
                if seen.insert(u) {
                    ids.push(u);
                }
            }
        }
    }
    if ids.is_empty() {
        return Ok(());
    }
    let table = rustapi_sql::table_name(target)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let sql = format!("SELECT * FROM {table} WHERE id = ANY($1)");
    let fetched = sqlx::query(&sql)
        .bind(&ids)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    let mut by_id: HashMap<Uuid, Value> = HashMap::with_capacity(fetched.len());
    for row in &fetched {
        let obj = crate::entry::row_to_json(&target_ct, row)?;
        let id_str = obj.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
        if let Some(s) = id_str {
            if let Ok(u) = Uuid::parse_str(&s) {
                by_id.insert(u, obj);
            }
        }
    }
    for r in rows.iter_mut() {
        let take = match r.get(field_name) {
            Some(Value::String(s)) => Uuid::parse_str(s).ok(),
            _ => None,
        };
        if let Some(u) = take {
            if let Some(obj) = by_id.get(&u).cloned() {
                r.insert(field_name.to_string(), obj);
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct InverseGroup {
    pub children: Vec<Value>,
    pub truncated: bool,
}

/// Bucket the SELECT'd children rows under their parent FK uuid and enforce
/// the per-parent cap. `parents` seeds the map so parents with zero matches
/// still get an empty group. Iteration order over `fetched` matters for
/// truncation: the first `cap` rows per parent stay, the rest set the flag.
pub fn group_inverse_children(
    parents: &[Uuid],
    fetched: Vec<(Uuid, Map<String, Value>)>,
    cap: usize,
) -> HashMap<Uuid, InverseGroup> {
    let mut out: HashMap<Uuid, InverseGroup> = HashMap::new();
    for p in parents {
        out.insert(
            *p,
            InverseGroup {
                children: Vec::new(),
                truncated: false,
            },
        );
    }
    for (p, row) in fetched {
        let g = out.entry(p).or_insert(InverseGroup {
            children: Vec::new(),
            truncated: false,
        });
        if g.children.len() < cap {
            g.children.push(Value::Object(row));
        } else {
            g.truncated = true;
        }
    }
    out
}

/// Hydrate an inverse relation in-place. Issues one batched SELECT against
/// `source` rows whose FK column matches any parent id; over-fetches by one
/// per parent (LIMIT `(cap+1) * N`) so the truncation flag is correct
/// whenever a parent crosses the cap. Parents with no children still
/// receive an empty array under `field_name` so the response shape is
/// stable.
pub async fn apply_inverse(
    pool: &PgPool,
    registry: &SchemaRegistry,
    rows: &mut [Map<String, Value>],
    field_name: &str,
    source_table: &str,
    fk_col: &str,
) -> Result<(), Error> {
    let source_ct = registry.get(source_table).await.ok_or_else(|| {
        Error::Internal(anyhow::anyhow!(
            "populate source vanished: {source_table}"
        ))
    })?;
    let mut parent_ids: Vec<Uuid> = Vec::with_capacity(rows.len());
    for r in rows.iter() {
        if let Some(Value::String(s)) = r.get("id") {
            if let Ok(u) = Uuid::parse_str(s) {
                parent_ids.push(u);
            }
        }
    }
    if parent_ids.is_empty() {
        for r in rows.iter_mut() {
            r.insert(field_name.into(), Value::Array(Vec::new()));
        }
        return Ok(());
    }
    let table = rustapi_sql::table_name(source_table)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let fk_quoted = rustapi_sql::quote_ident(fk_col)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    // Over-fetch by one per parent to detect truncation without a separate
    // COUNT roundtrip. Order by FK then id so per-parent slices stay stable.
    let limit = (INVERSE_LIMIT_PER_PARENT + 1) * parent_ids.len();
    let sql = format!(
        "SELECT * FROM {table} WHERE {fk_quoted} = ANY($1) \
         ORDER BY {fk_quoted}, id LIMIT {limit}"
    );
    let fetched_rows = sqlx::query(&sql)
        .bind(&parent_ids)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    let mut buckets: Vec<(Uuid, Map<String, Value>)> = Vec::with_capacity(fetched_rows.len());
    for row in &fetched_rows {
        let parent: Uuid = row
            .try_get(fk_col)
            .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
        let map = match crate::entry::row_to_json(&source_ct, row)? {
            Value::Object(m) => m,
            _ => unreachable!("row_to_json returns an object"),
        };
        buckets.push((parent, map));
    }
    let grouped = group_inverse_children(&parent_ids, buckets, INVERSE_LIMIT_PER_PARENT);
    for r in rows.iter_mut() {
        let pid = r
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        match pid.and_then(|p| grouped.get(&p)) {
            Some(g) => {
                r.insert(field_name.into(), Value::Array(g.children.clone()));
                if g.truncated {
                    r.insert(format!("{field_name}_truncated"), Value::Bool(true));
                }
            }
            None => {
                r.insert(field_name.into(), Value::Array(Vec::new()));
            }
        }
    }
    Ok(())
}

/// Hydrate a many-to-many field in-place. One batched SELECT joins the join
/// table to the target rows for all parents, then groups per parent with the
/// existing per-parent cap. Parents with no links get `[]`.
#[allow(clippy::too_many_arguments)]
pub async fn apply_many(
    pool: &PgPool,
    registry: &SchemaRegistry,
    rows: &mut [Map<String, Value>],
    field_name: &str,
    join_table: &str,
    self_col: &str,
    other_col: &str,
    target: &str,
) -> Result<(), Error> {
    let target_ct = registry.get(target).await.ok_or_else(|| {
        Error::Internal(anyhow::anyhow!("populate m2m target vanished: {target}"))
    })?;
    let mut parent_ids: Vec<Uuid> = Vec::with_capacity(rows.len());
    for r in rows.iter() {
        if let Some(Value::String(s)) = r.get("id") {
            if let Ok(u) = Uuid::parse_str(s) {
                parent_ids.push(u);
            }
        }
    }
    if parent_ids.is_empty() {
        for r in rows.iter_mut() {
            r.insert(field_name.into(), Value::Array(Vec::new()));
        }
        return Ok(());
    }
    let target_tbl = rustapi_sql::table_name(target)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let self_q = rustapi_sql::quote_ident(self_col)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let other_q = rustapi_sql::quote_ident(other_col)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    // join_table is already a quoted identifier from join_table_name.
    let limit = (INVERSE_LIMIT_PER_PARENT + 1) * parent_ids.len();
    let sql = format!(
        "SELECT j.{self_q} AS __parent, t.* \
         FROM {join_table} j JOIN {target_tbl} t ON t.\"id\" = j.{other_q} \
         WHERE j.{self_q} = ANY($1) \
         ORDER BY j.{self_q}, t.\"id\" LIMIT {limit}"
    );
    let fetched = sqlx::query(&sql)
        .bind(&parent_ids)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    let mut buckets: Vec<(Uuid, Map<String, Value>)> = Vec::with_capacity(fetched.len());
    for row in &fetched {
        let parent: Uuid = row
            .try_get("__parent")
            .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
        let map = match crate::entry::row_to_json(&target_ct, row)? {
            Value::Object(m) => m,
            _ => unreachable!("row_to_json returns an object"),
        };
        buckets.push((parent, map));
    }
    let grouped = group_inverse_children(&parent_ids, buckets, INVERSE_LIMIT_PER_PARENT);
    for r in rows.iter_mut() {
        let pid = r
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        match pid.and_then(|p| grouped.get(&p)) {
            Some(g) => {
                r.insert(field_name.into(), Value::Array(g.children.clone()));
                if g.truncated {
                    r.insert(format!("{field_name}_truncated"), Value::Bool(true));
                }
            }
            None => {
                r.insert(field_name.into(), Value::Array(Vec::new()));
            }
        }
    }
    Ok(())
}

/// Hydrate a one_to_one inverse: at most one child per parent (FK is UNIQUE).
/// Sets the field to the single child object, or `null` when none.
pub async fn apply_inverse_one(
    pool: &PgPool,
    registry: &SchemaRegistry,
    rows: &mut [Map<String, Value>],
    field_name: &str,
    source_table: &str,
    fk_col: &str,
) -> Result<(), Error> {
    let tmp_key = format!("__one_{field_name}");
    apply_inverse(pool, registry, rows, &tmp_key, source_table, fk_col).await?;
    for r in rows.iter_mut() {
        let collapsed = match r.remove(&tmp_key) {
            Some(Value::Array(mut xs)) if !xs.is_empty() => {
                if xs.len() > 1 {
                    tracing::warn!(
                        field = field_name,
                        source = source_table,
                        count = xs.len(),
                        "one_to_one inverse resolved more than one child; FK uniqueness may be violated — taking the first"
                    );
                }
                xs.remove(0)
            }
            _ => Value::Null,
        };
        r.remove(&format!("{tmp_key}_truncated"));
        r.insert(field_name.into(), collapsed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::Field;
    use serde_json::json;

    fn ct_with_relation() -> ContentType {
        ContentType {
            id: Uuid::new_v4(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![
                Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: false,
                    unique: false,
                    default: serde_json::Value::Null,
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "author".into(),
                    kind: FieldKind::Relation,
                    required: false,
                    unique: false,
                    default: serde_json::Value::Null,
                    max_length: None,
                    kind_meta: json!({
                        "target": "user",
                        "cardinality": "many_to_one",
                        "inverse": "posts"
                    }),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

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
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: json!({}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn parse_forward_field() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let out = parse_populate(&ct, &reg, "author").await.unwrap();
        assert_eq!(out.len(), 1);
        match &out[0] {
            PopulateField::Forward { field_name, target } => {
                assert_eq!(field_name, "author");
                assert_eq!(target, "user");
            }
            other => panic!("expected Forward, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_inverse_via_registry() {
        let reg = SchemaRegistry::new();
        reg.insert(user_ct()).await;
        reg.insert(ct_with_relation()).await;
        let user = reg.get("user").await.unwrap();
        let out = parse_populate(&user, &reg, "posts").await.unwrap();
        assert_eq!(out.len(), 1);
        match &out[0] {
            PopulateField::Inverse { field_name, source, fk_col } => {
                assert_eq!(field_name, "posts");
                assert_eq!(source, "post");
                assert_eq!(fk_col, "author_id");
            }
            other => panic!("expected Inverse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reject_unknown_field() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let err = parse_populate(&ct, &reg, "nope").await.unwrap_err();
        assert!(format!("{err:?}").contains("unknown populate field"));
    }

    #[tokio::test]
    async fn reject_empty() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let err = parse_populate(&ct, &reg, "").await.unwrap_err();
        assert!(format!("{err:?}").contains("must not be empty"));
    }

    #[tokio::test]
    async fn reject_blank_entry() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let err = parse_populate(&ct, &reg, "author, ,title").await.unwrap_err();
        assert!(format!("{err:?}").contains("empty entries"));
    }

    #[tokio::test]
    async fn dedupe_duplicates() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let out = parse_populate(&ct, &reg, "author,author").await.unwrap();
        assert_eq!(out.len(), 1);
    }

    #[tokio::test]
    async fn reject_primitive_field() {
        let ct = ct_with_relation();
        let reg = SchemaRegistry::new();
        let err = parse_populate(&ct, &reg, "title").await.unwrap_err();
        assert!(format!("{err:?}").contains("unknown populate field"));
    }

    #[tokio::test]
    async fn forward_and_inverse_preserve_order() {
        let reg = SchemaRegistry::new();
        reg.insert(user_ct()).await;
        reg.insert(ct_with_relation()).await;
        let post = reg.get("post").await.unwrap();
        // post has forward `author` but no inverse on itself; verify the
        // parse loop hits the forward arm + returns just one entry.
        let out = parse_populate(&post, &reg, "author").await.unwrap();
        assert_eq!(out.len(), 1);
    }

    fn child_row() -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("id".into(), Value::String(Uuid::new_v4().to_string()));
        m
    }

    #[test]
    fn inverse_grouping_caps_at_25_and_sets_flag() {
        let parents: Vec<Uuid> = vec![Uuid::new_v4(), Uuid::new_v4()];
        let mut children: Vec<(Uuid, Map<String, Value>)> = Vec::new();
        for _ in 0..26 {
            children.push((parents[0], child_row()));
        }
        for _ in 0..3 {
            children.push((parents[1], child_row()));
        }
        let grouped = group_inverse_children(&parents, children, INVERSE_LIMIT_PER_PARENT);
        let p0 = grouped.get(&parents[0]).unwrap();
        assert_eq!(p0.children.len(), 25);
        assert!(p0.truncated);
        let p1 = grouped.get(&parents[1]).unwrap();
        assert_eq!(p1.children.len(), 3);
        assert!(!p1.truncated);
    }

    #[test]
    fn inverse_grouping_seeds_empty_parents() {
        // Parents with zero children must still appear in the map so the
        // handler can write `[]` rather than skipping the JSON key.
        let parents: Vec<Uuid> = vec![Uuid::new_v4(), Uuid::new_v4()];
        let grouped = group_inverse_children(&parents, vec![], INVERSE_LIMIT_PER_PARENT);
        assert_eq!(grouped.len(), 2);
        for p in &parents {
            let g = grouped.get(p).unwrap();
            assert!(g.children.is_empty());
            assert!(!g.truncated);
        }
    }

    #[test]
    fn inverse_grouping_exact_cap_no_flag() {
        // 25 children → exactly at cap → no truncation.
        let parents = vec![Uuid::new_v4()];
        let children: Vec<(Uuid, Map<String, Value>)> =
            (0..25).map(|_| (parents[0], child_row())).collect();
        let grouped = group_inverse_children(&parents, children, INVERSE_LIMIT_PER_PARENT);
        let g = grouped.get(&parents[0]).unwrap();
        assert_eq!(g.children.len(), 25);
        assert!(!g.truncated);
    }

    fn ct_with_m2m() -> ContentType {
        ContentType {
            id: Uuid::new_v4(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "tags".into(),
                kind: FieldKind::Relation,
                required: false,
                unique: false,
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: json!({"target":"tag","cardinality":"many_to_many","inverse":"posts"}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn tag_ct() -> ContentType {
        ContentType {
            id: Uuid::new_v4(),
            name: "tag".into(),
            display_name: "Tag".into(),
            fields: vec![Field {
                name: "label".into(),
                kind: FieldKind::String,
                required: false,
                unique: false,
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: json!({}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn parse_m2m_forward() {
        let reg = SchemaRegistry::new();
        reg.insert(tag_ct()).await;
        reg.insert(ct_with_m2m()).await;
        let post = reg.get("post").await.unwrap();
        let out = parse_populate(&post, &reg, "tags").await.unwrap();
        match &out[0] {
            PopulateField::Many { field_name, join_table, self_col, other_col, target } => {
                assert_eq!(field_name, "tags");
                assert_eq!(join_table, "\"j_post_tags\"");
                assert_eq!(self_col, "post_id");
                assert_eq!(other_col, "tag_id");
                assert_eq!(target, "tag");
            }
            other => panic!("expected Many, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_m2m_inverse() {
        let reg = SchemaRegistry::new();
        reg.insert(tag_ct()).await;
        reg.insert(ct_with_m2m()).await;
        let tag = reg.get("tag").await.unwrap();
        let out = parse_populate(&tag, &reg, "posts").await.unwrap();
        match &out[0] {
            PopulateField::Many { field_name, self_col, other_col, target, .. } => {
                assert_eq!(field_name, "posts");
                assert_eq!(self_col, "tag_id");
                assert_eq!(other_col, "post_id");
                assert_eq!(target, "post");
            }
            other => panic!("expected Many (inverse), got {other:?}"),
        }
    }
}
