//! DML string + bind-plan builders. Most builders return `SqlAndBinds`
//! (`(String, Vec<BoundValue>)`) and the HTTP layer translates `BoundValue`
//! into sqlx binds via `bind_all`. The many-to-many link helpers
//! (`insert_links`/`delete_links`) instead return plain `(String, Uuid, …)`
//! because they bind a `uuid[]` array (`$2`) that `BoundValue` has no variant
//! for; their callers bind directly with `sqlx::query(...).bind(...)`.

use crate::filter::{Condition, Filter, FilterValue, Op};
use crate::ident::{join_table_name, quote_ident, table_name, IdentError};
use crate::sort::{Sort, SortDir};
use rustapi_core::{BoundValue, ContentType, FieldKind};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DmlError {
    #[error(transparent)]
    Ident(#[from] IdentError),
    #[error("unknown field `{0}` in payload")]
    UnknownField(String),
    #[error("invalid filter: {0}")]
    InvalidFilter(&'static str),
}

pub type SqlAndBinds = (String, Vec<BoundValue>);

/// Which publish state to return from a list query. `All` adds no clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishFilter {
    Published,
    Draft,
    All,
}

/// `INSERT INTO ct_<name> (cols...) VALUES ($1, $2, ...) RETURNING *`
pub fn insert(
    ct: &ContentType,
    values: &BTreeMap<String, BoundValue>,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(&ct.name)?;
    let by_name: std::collections::HashMap<&str, &rustapi_core::Field> =
        ct.fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut cols = vec![];
    let mut placeholders = vec![];
    let mut binds = vec![];
    for (i, (name, val)) in values.iter().enumerate() {
        let Some(f) = by_name.get(name.as_str()) else {
            return Err(DmlError::UnknownField(name.clone()));
        };
        cols.push(quote_ident(&f.physical_column())?);
        placeholders.push(format!("${}", i + 1));
        binds.push(val.clone());
    }
    let sql = if cols.is_empty() {
        format!("INSERT INTO {table} DEFAULT VALUES RETURNING *")
    } else {
        let cols_s = cols.join(", ");
        let ph_s = placeholders.join(", ");
        format!("INSERT INTO {table} ({cols_s}) VALUES ({ph_s}) RETURNING *")
    };
    Ok((sql, binds))
}

/// `UPDATE ct_<name> SET col=$1, ..., updated_at=now() WHERE id=$N RETURNING *`
pub fn update(
    ct: &ContentType,
    id: Uuid,
    values: &BTreeMap<String, BoundValue>,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(&ct.name)?;
    let by_name: std::collections::HashMap<&str, &rustapi_core::Field> =
        ct.fields.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut sets = vec![];
    let mut binds: Vec<BoundValue> = vec![];
    for (i, (name, val)) in values.iter().enumerate() {
        let Some(f) = by_name.get(name.as_str()) else {
            return Err(DmlError::UnknownField(name.clone()));
        };
        let col = quote_ident(&f.physical_column())?;
        let placeholder = i + 1;
        sets.push(format!("{col} = ${placeholder}"));
        binds.push(val.clone());
    }
    sets.push("\"updated_at\" = now()".into());
    let id_placeholder = binds.len() + 1;
    binds.push(BoundValue::Str(id.to_string()));
    let sets_s = sets.join(", ");
    let sql =
        format!("UPDATE {table} SET {sets_s} WHERE \"id\" = ${id_placeholder}::uuid RETURNING *");
    Ok((sql, binds))
}

/// `DELETE FROM ct_<name> WHERE id=$1`
pub fn delete(ct_name: &str, id: Uuid) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let sql = format!("DELETE FROM {table} WHERE \"id\" = $1::uuid");
    Ok((sql, vec![BoundValue::Str(id.to_string())]))
}

/// `UPDATE ct_<name> SET published_at = now(), updated_at = now() WHERE id=$1`
pub fn publish(ct_name: &str, id: Uuid) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let sql = format!(
        "UPDATE {table} SET \"published_at\" = now(), \"updated_at\" = now() WHERE \"id\" = $1::uuid RETURNING *"
    );
    Ok((sql, vec![BoundValue::Str(id.to_string())]))
}

/// `UPDATE ct_<name> SET published_at = NULL, updated_at = now() WHERE id=$1`
pub fn unpublish(ct_name: &str, id: Uuid) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let sql = format!(
        "UPDATE {table} SET \"published_at\" = NULL, \"updated_at\" = now() WHERE \"id\" = $1::uuid RETURNING *"
    );
    Ok((sql, vec![BoundValue::Str(id.to_string())]))
}

/// Build the multi-row link INSERT for a many-to-many field. The caller binds
/// `$1` = owner id (`Uuid`) and `$2` = target ids (`Vec<Uuid>`). `ON CONFLICT
/// DO NOTHING` makes re-inserting an existing link a no-op (PK guards dupes).
/// An empty `$2` array is a safe no-op (UNNEST yields zero rows), so callers
/// may skip the call for empty input but are not required to.
/// Does NOT return `SqlAndBinds`: `$2` is a `uuid[]` array bind with no
/// `BoundValue` variant, so the caller binds `$1`/`$2` directly.
pub fn insert_links(
    owner_type: &str,
    field: &str,
    target_type: &str,
    owner_id: Uuid,
) -> Result<(String, Uuid), DmlError> {
    let jt = join_table_name(owner_type, field)?;
    let owner_col = quote_ident(&format!("{owner_type}_id"))?;
    let target_col = quote_ident(&format!("{target_type}_id"))?;
    let sql = format!(
        "INSERT INTO {jt} ({owner_col}, {target_col}) \
SELECT $1::uuid, x FROM UNNEST($2::uuid[]) AS x ON CONFLICT DO NOTHING"
    );
    Ok((sql, owner_id))
}

/// `DELETE FROM <join> WHERE <owner>_id = $1::uuid` — clears all links for an
/// owner ahead of a replace-set re-insert. Caller binds `$1` = owner id.
/// Does NOT return `SqlAndBinds`: same direct-bind rationale as `insert_links`
/// (the pair are always called together; keeping the same return shape is
/// consistent and avoids a `BoundValue::Uuid` round-trip for the owner id).
pub fn delete_links(
    owner_type: &str,
    field: &str,
    owner_id: Uuid,
) -> Result<(String, Uuid), DmlError> {
    let jt = join_table_name(owner_type, field)?;
    let owner_col = quote_ident(&format!("{owner_type}_id"))?;
    let sql = format!("DELETE FROM {jt} WHERE {owner_col} = $1::uuid");
    Ok((sql, owner_id))
}

/// `INSERT INTO j_media_<ct>_<field> (<ct>_id, asset_id, position)` — replace-set
/// insert of a gallery in array order. `position` comes from `WITH ORDINALITY`
/// (1-based). Caller binds `$1` = owner id, `$2` = `uuid[]` of asset ids in order.
/// No `ON CONFLICT` clause: callers always precede this with `delete_media_links` (replace-set pattern).
pub fn insert_media_links(
    ct: &str,
    field: &str,
    owner_id: Uuid,
) -> Result<(String, Uuid), DmlError> {
    let jt = crate::ident::media_join_table_name(ct, field)?;
    let owner_col = quote_ident(&format!("{ct}_id"))?;
    let sql = format!(
        "INSERT INTO {jt} ({owner_col}, \"asset_id\", \"position\") \
SELECT $1::uuid, x.asset, x.ord::int FROM UNNEST($2::uuid[]) WITH ORDINALITY AS x(asset, ord)"
    );
    Ok((sql, owner_id))
}

/// `DELETE FROM j_media_<ct>_<field> WHERE <ct>_id = $1::uuid` — clears a gallery
/// ahead of a replace-set re-insert. Caller binds `$1` = owner id.
pub fn delete_media_links(
    ct: &str,
    field: &str,
    owner_id: Uuid,
) -> Result<(String, Uuid), DmlError> {
    let jt = crate::ident::media_join_table_name(ct, field)?;
    let owner_col = quote_ident(&format!("{ct}_id"))?;
    let sql = format!("DELETE FROM {jt} WHERE {owner_col} = $1::uuid");
    Ok((sql, owner_id))
}

/// `SELECT * FROM ct_<name> WHERE id=$1`
pub fn select_by_id(ct_name: &str, id: Uuid) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let sql = format!("SELECT * FROM {table} WHERE \"id\" = $1::uuid");
    Ok((sql, vec![BoundValue::Str(id.to_string())]))
}

/// `SELECT * FROM ct_<name> WHERE id = ANY($1::uuid[])`
/// Caller binds a `Vec<Uuid>` as `$1`.
/// Returns only the SQL string (no BoundValue vec) because Vec<Uuid> has no
/// BoundValue variant — same pattern as `insert_links`.
pub fn select_by_ids_sql(ct_name: &str) -> Result<String, DmlError> {
    let table = table_name(ct_name)?;
    Ok(format!(
        "SELECT * FROM {table} WHERE \"id\" = ANY($1::uuid[])"
    ))
}

/// `SELECT * FROM ct_<name> [WHERE ...] ORDER BY <col> <dir> LIMIT $N OFFSET $N+1`
/// where N is `binds.len() + 1` after the filter has supplied its own placeholders.
pub fn select_list(
    ct_name: &str,
    filter: &Filter,
    sort: &Sort,
    limit: i64,
    offset: i64,
) -> Result<SqlAndBinds, DmlError> {
    select_list_status(ct_name, filter, sort, limit, offset, PublishFilter::All)
}

/// Like `select_list` but also filters by publish state.
pub fn select_list_status(
    ct_name: &str,
    filter: &Filter,
    sort: &Sort,
    limit: i64,
    offset: i64,
    publish: PublishFilter,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let col = quote_ident(&sort.column)?;
    let dir = sort.dir.as_sql();

    let (mut where_sql, mut binds) = render_where(filter, 1)?;
    let publish_pred = match publish {
        PublishFilter::Published => Some("\"published_at\" IS NOT NULL"),
        PublishFilter::Draft => Some("\"published_at\" IS NULL"),
        PublishFilter::All => None,
    };
    if let Some(pred) = publish_pred {
        if where_sql.is_empty() {
            where_sql = format!(" WHERE {pred}");
        } else {
            where_sql = format!("{where_sql} AND {pred}");
        }
    }

    let limit_ph = binds.len() + 1;
    let offset_ph = binds.len() + 2;
    binds.push(BoundValue::I64(limit));
    binds.push(BoundValue::I64(offset));

    let sql = format!(
        "SELECT * FROM {table}{where_sql} ORDER BY {col} {dir} LIMIT ${limit_ph} OFFSET ${offset_ph}"
    );
    Ok((sql, binds))
}

/// Keyset (seek) list query. Seeks past the `(sort_col, id)` cursor instead of
/// using OFFSET, so page depth no longer affects cost (needs an index on
/// `(sort_col, id)`). `after = None` returns the first page (no seek clause).
/// The `id` tiebreak makes ordering total so no rows are skipped/duplicated at
/// page seams. Comparison is `<` for DESC, `>` for ASC.
pub fn select_list_keyset(
    ct_name: &str,
    filter: &Filter,
    sort: &Sort,
    after: Option<(BoundValue, Uuid)>,
    limit: i64,
) -> Result<SqlAndBinds, DmlError> {
    select_list_keyset_status(ct_name, filter, sort, after, limit, PublishFilter::All)
}

/// Like `select_list_keyset` but also filters by publish state.
pub fn select_list_keyset_status(
    ct_name: &str,
    filter: &Filter,
    sort: &Sort,
    after: Option<(BoundValue, Uuid)>,
    limit: i64,
    publish: PublishFilter,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let col = quote_ident(&sort.column)?;
    let dir = sort.dir.as_sql();
    let cmp = match sort.dir {
        SortDir::Asc => ">",
        SortDir::Desc => "<",
    };

    let (mut where_sql, mut binds) = render_where(filter, 1)?;
    let publish_pred = match publish {
        PublishFilter::Published => Some("\"published_at\" IS NOT NULL"),
        PublishFilter::Draft => Some("\"published_at\" IS NULL"),
        PublishFilter::All => None,
    };
    if let Some(pred) = publish_pred {
        if where_sql.is_empty() {
            where_sql = format!(" WHERE {pred}");
        } else {
            where_sql = format!("{where_sql} AND {pred}");
        }
    }

    if let Some((val, id)) = after {
        let val_ph = binds.len() + 1;
        let id_ph = binds.len() + 2;
        let seek = format!("({col}, \"id\") {cmp} (${val_ph}, ${id_ph}::uuid)");
        if where_sql.is_empty() {
            where_sql = format!(" WHERE {seek}");
        } else {
            where_sql = format!("{where_sql} AND {seek}");
        }
        binds.push(val);
        binds.push(BoundValue::Uuid(id));
    }

    let limit_ph = binds.len() + 1;
    binds.push(BoundValue::I64(limit));

    let sql = format!(
        "SELECT * FROM {table}{where_sql} ORDER BY {col} {dir}, \"id\" {dir} LIMIT ${limit_ph}"
    );
    Ok((sql, binds))
}

/// `SELECT count(*) FROM ct_<name> [WHERE ...]`
pub fn count(ct_name: &str, filter: &Filter) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let (where_sql, binds) = render_where(filter, 1)?;
    Ok((format!("SELECT count(*) FROM {table}{where_sql}"), binds))
}

/// `SELECT count(*) FROM ct_<name> [WHERE ...]` with publish-state predicate.
pub fn count_status(
    ct_name: &str,
    filter: &Filter,
    publish: PublishFilter,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let (mut where_sql, binds) = render_where(filter, 1)?;
    let publish_pred = match publish {
        PublishFilter::Published => Some("\"published_at\" IS NOT NULL"),
        PublishFilter::Draft => Some("\"published_at\" IS NULL"),
        PublishFilter::All => None,
    };
    if let Some(pred) = publish_pred {
        if where_sql.is_empty() {
            where_sql = format!(" WHERE {pred}");
        } else {
            where_sql = format!("{where_sql} AND {pred}");
        }
    }
    Ok((format!("SELECT count(*) FROM {table}{where_sql}"), binds))
}

/// Postgres type-cast string for a FieldKind. Used by row-decoding helpers
/// and by `render_where` to type placeholders in filter conditions.
fn order_symbol(op: Op) -> &'static str {
    match op {
        Op::Gt => ">",
        Op::Gte => ">=",
        Op::Lt => "<",
        Op::Lte => "<=",
        // Unreachable: caller filters by op group before calling.
        _ => "?",
    }
}

pub fn pg_cast(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::String | FieldKind::Text => "text",
        FieldKind::Integer => "int8",
        FieldKind::Float => "float8",
        FieldKind::Boolean => "bool",
        FieldKind::Datetime => "timestamptz",
        FieldKind::Uuid => "uuid",
        _ => "text",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sort::{Sort, SortDir};
    use chrono::Utc;
    use rustapi_core::{ContentType, Field};
    use serde_json::json;

    fn ct(fields: Vec<Field>) -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields,
            options: json!({}),
            kind: rustapi_core::ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn field(name: &str, kind: FieldKind) -> Field {
        Field {
            name: name.into(),
            kind,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        }
    }

    #[test]
    fn insert_basic() {
        let c = ct(vec![field("title", FieldKind::String)]);
        let mut vals = BTreeMap::new();
        vals.insert("title".into(), BoundValue::Str("Hi".into()));
        let (sql, binds) = insert(&c, &vals).unwrap();
        assert_eq!(
            sql,
            "INSERT INTO \"ct_post\" (\"title\") VALUES ($1) RETURNING *"
        );
        assert_eq!(binds, vec![BoundValue::Str("Hi".into())]);
    }

    #[test]
    fn insert_empty_uses_defaults() {
        let c = ct(vec![field("title", FieldKind::String)]);
        let (sql, binds) = insert(&c, &BTreeMap::new()).unwrap();
        assert_eq!(sql, "INSERT INTO \"ct_post\" DEFAULT VALUES RETURNING *");
        assert!(binds.is_empty());
    }

    #[test]
    fn insert_rejects_unknown_field() {
        let c = ct(vec![field("title", FieldKind::String)]);
        let mut vals = BTreeMap::new();
        vals.insert("nope".into(), BoundValue::Null(FieldKind::String));
        assert!(matches!(insert(&c, &vals), Err(DmlError::UnknownField(_))));
    }

    #[test]
    fn update_sets_updated_at_and_id_clause() {
        let c = ct(vec![field("title", FieldKind::String)]);
        let mut vals = BTreeMap::new();
        vals.insert("title".into(), BoundValue::Str("New".into()));
        let id = Uuid::new_v4();
        let (sql, binds) = update(&c, id, &vals).unwrap();
        assert!(sql.starts_with("UPDATE \"ct_post\" SET \"title\" = $1"));
        assert!(sql.contains("\"updated_at\" = now()"));
        assert!(sql.ends_with("WHERE \"id\" = $2::uuid RETURNING *"));
        assert_eq!(binds[0], BoundValue::Str("New".into()));
        assert_eq!(binds[1], BoundValue::Str(id.to_string()));
    }

    #[test]
    fn delete_works() {
        let id = Uuid::new_v4();
        let (sql, binds) = delete("post", id).unwrap();
        assert_eq!(sql, "DELETE FROM \"ct_post\" WHERE \"id\" = $1::uuid");
        assert_eq!(binds, vec![BoundValue::Str(id.to_string())]);
    }

    #[test]
    fn select_by_id_works() {
        let id = Uuid::new_v4();
        let (sql, _binds) = select_by_id("post", id).unwrap();
        assert_eq!(sql, "SELECT * FROM \"ct_post\" WHERE \"id\" = $1::uuid");
    }

    #[test]
    fn select_list_orders_and_paginates() {
        let s = Sort {
            column: "created_at".into(),
            dir: SortDir::Desc,
        };
        let (sql, binds) = select_list("post", &Filter::None, &s, 25, 50).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"ct_post\" ORDER BY \"created_at\" DESC LIMIT $1 OFFSET $2"
        );
        assert_eq!(binds, vec![BoundValue::I64(25), BoundValue::I64(50)]);
    }

    fn relation_field(name: &str, target: &str) -> Field {
        Field {
            name: name.into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({"target": target, "cardinality": "many_to_one"}),
        }
    }

    #[test]
    fn insert_relation_uses_physical_column() {
        let c = ct(vec![relation_field("author", "user")]);
        let mut vals = BTreeMap::new();
        let id = Uuid::new_v4();
        vals.insert("author".into(), BoundValue::Uuid(id));
        let (sql, binds) = insert(&c, &vals).unwrap();
        assert_eq!(
            sql,
            "INSERT INTO \"ct_post\" (\"author_id\") VALUES ($1) RETURNING *"
        );
        assert_eq!(binds, vec![BoundValue::Uuid(id)]);
    }

    #[test]
    fn update_relation_uses_physical_column() {
        let c = ct(vec![relation_field("author", "user")]);
        let mut vals = BTreeMap::new();
        let target_id = Uuid::new_v4();
        vals.insert("author".into(), BoundValue::Uuid(target_id));
        let row_id = Uuid::new_v4();
        let (sql, binds) = update(&c, row_id, &vals).unwrap();
        assert!(sql.starts_with("UPDATE \"ct_post\" SET \"author_id\" = $1"));
        assert_eq!(binds[0], BoundValue::Uuid(target_id));
    }

    #[test]
    fn insert_relation_null_writes_typed_null() {
        let c = ct(vec![relation_field("author", "user")]);
        let mut vals = BTreeMap::new();
        vals.insert("author".into(), BoundValue::Null(FieldKind::Uuid));
        let (sql, binds) = insert(&c, &vals).unwrap();
        assert_eq!(
            sql,
            "INSERT INTO \"ct_post\" (\"author_id\") VALUES ($1) RETURNING *"
        );
        assert_eq!(binds, vec![BoundValue::Null(FieldKind::Uuid)]);
    }

    #[test]
    fn count_basic() {
        let (sql, binds) = count("post", &Filter::None).unwrap();
        assert_eq!(sql, "SELECT count(*) FROM \"ct_post\"");
        assert!(binds.is_empty());
    }

    #[test]
    fn select_list_with_filter_shifts_pagination() {
        let s = Sort {
            column: "created_at".into(),
            dir: SortDir::Desc,
        };
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "title",
            FieldKind::String,
            Op::Eq,
            FilterValue::Bound(BoundValue::Str("hi".into())),
        ))]);
        let (sql, binds) = select_list("post", &f, &s, 25, 50).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"ct_post\" WHERE \"title\" = $1::text ORDER BY \"created_at\" DESC LIMIT $2 OFFSET $3"
        );
        assert_eq!(
            binds,
            vec![
                BoundValue::Str("hi".into()),
                BoundValue::I64(25),
                BoundValue::I64(50)
            ]
        );
    }

    #[test]
    fn select_list_empty_all_keeps_v1_placeholders() {
        // `Filter::All(vec![])` is equivalent to `Filter::None` at render_where;
        // confirm the dml layer keeps `LIMIT $1 OFFSET $2`.
        let s = Sort {
            column: "created_at".into(),
            dir: SortDir::Desc,
        };
        let (sql, binds) = select_list("post", &Filter::All(vec![]), &s, 25, 50).unwrap();
        assert!(sql.ends_with("LIMIT $1 OFFSET $2"));
        assert_eq!(binds, vec![BoundValue::I64(25), BoundValue::I64(50)]);
    }

    #[test]
    fn count_with_filter() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::Ne,
            FilterValue::Bound(BoundValue::I64(0)),
        ))]);
        let (sql, binds) = count("post", &f).unwrap();
        assert_eq!(
            sql,
            "SELECT count(*) FROM \"ct_post\" WHERE \"views\" <> $1::int8"
        );
        assert_eq!(binds, vec![BoundValue::I64(0)]);
    }

    #[test]
    fn insert_links_uses_unnest() {
        let owner = Uuid::nil();
        let (sql, owner_bind) = insert_links("post", "tags", "tag", owner).unwrap();
        assert_eq!(
            sql,
            "INSERT INTO \"j_post_tags\" (\"post_id\", \"tag_id\") \
SELECT $1::uuid, x FROM UNNEST($2::uuid[]) AS x ON CONFLICT DO NOTHING"
        );
        assert_eq!(owner_bind, owner);
    }

    #[test]
    fn delete_links_clears_owner() {
        let owner = Uuid::nil();
        let (sql, bind) = delete_links("post", "tags", owner).unwrap();
        assert_eq!(
            sql,
            "DELETE FROM \"j_post_tags\" WHERE \"post_id\" = $1::uuid"
        );
        assert_eq!(bind, owner);
    }

    #[test]
    fn insert_media_links_emits_positioned_unnest() {
        let id = Uuid::nil();
        let (sql, owner) = super::insert_media_links("post", "gallery", id).unwrap();
        assert_eq!(owner, id);
        assert_eq!(
            sql,
            "INSERT INTO \"j_media_post_gallery\" (\"post_id\", \"asset_id\", \"position\") \
SELECT $1::uuid, x.asset, x.ord::int FROM UNNEST($2::uuid[]) WITH ORDINALITY AS x(asset, ord)"
        );
    }

    #[test]
    fn delete_media_links_clears_owner() {
        let id = Uuid::nil();
        let (sql, owner) = super::delete_media_links("post", "gallery", id).unwrap();
        assert_eq!(owner, id);
        assert_eq!(
            sql,
            "DELETE FROM \"j_media_post_gallery\" WHERE \"post_id\" = $1::uuid"
        );
    }

    #[test]
    fn publish_sets_published_at_now() {
        let (sql, binds) = publish("post", Uuid::nil()).unwrap();
        assert_eq!(
            sql,
            "UPDATE \"ct_post\" SET \"published_at\" = now(), \"updated_at\" = now() WHERE \"id\" = $1::uuid RETURNING *"
        );
        assert_eq!(binds.len(), 1);
    }

    #[test]
    fn unpublish_nulls_published_at() {
        let (sql, _) = unpublish("post", Uuid::nil()).unwrap();
        assert_eq!(
            sql,
            "UPDATE \"ct_post\" SET \"published_at\" = NULL, \"updated_at\" = now() WHERE \"id\" = $1::uuid RETURNING *"
        );
    }

    #[test]
    fn select_list_published_filter_appends_clause() {
        let (sql, _) = select_list_status(
            "post",
            &Filter::None,
            &Sort::default_created_at(),
            10,
            0,
            PublishFilter::Published,
        )
        .unwrap();
        assert!(sql.contains("\"published_at\" IS NOT NULL"), "got: {sql}");
    }

    #[test]
    fn select_list_draft_filter_appends_clause() {
        let (sql, _) = select_list_status(
            "post",
            &Filter::None,
            &Sort::default_created_at(),
            10,
            0,
            PublishFilter::Draft,
        )
        .unwrap();
        assert!(sql.contains("\"published_at\" IS NULL"), "got: {sql}");
    }

    #[test]
    fn select_list_all_filter_no_publish_clause() {
        let (sql, _) = select_list_status(
            "post",
            &Filter::None,
            &Sort::default_created_at(),
            10,
            0,
            PublishFilter::All,
        )
        .unwrap();
        assert!(!sql.contains("published_at"), "got: {sql}");
    }

    #[test]
    fn test_select_by_ids_sql() {
        let sql = super::select_by_ids_sql("post").unwrap();
        assert!(sql.contains("ct_post"));
        assert!(sql.contains("ANY($1::uuid[])"));
        // should not contain LIMIT/OFFSET
        assert!(!sql.contains("LIMIT"));
    }

    #[test]
    fn select_list_keyset_first_page_no_cursor_desc() {
        let s = Sort { column: "created_at".into(), dir: SortDir::Desc };
        let (sql, binds) = super::select_list_keyset("post", &Filter::None, &s, None, 25).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"ct_post\" ORDER BY \"created_at\" DESC, \"id\" DESC LIMIT $1"
        );
        assert_eq!(binds, vec![BoundValue::I64(25)]);
    }

    #[test]
    fn select_list_keyset_with_cursor_desc() {
        let s = Sort { column: "created_at".into(), dir: SortDir::Desc };
        let id = Uuid::nil();
        let after = Some((BoundValue::Str("2024-01-01T00:00:00Z".into()), id));
        let (sql, binds) = super::select_list_keyset("post", &Filter::None, &s, after, 25).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"ct_post\" WHERE (\"created_at\", \"id\") < ($1, $2::uuid) \
             ORDER BY \"created_at\" DESC, \"id\" DESC LIMIT $3"
        );
        assert_eq!(
            binds,
            vec![BoundValue::Str("2024-01-01T00:00:00Z".into()), BoundValue::Uuid(id), BoundValue::I64(25)]
        );
    }

    #[test]
    fn select_list_keyset_with_cursor_asc_uses_gt() {
        let s = Sort { column: "title".into(), dir: SortDir::Asc };
        let id = Uuid::nil();
        let after = Some((BoundValue::Str("hello".into()), id));
        let (sql, _) = super::select_list_keyset("post", &Filter::None, &s, after, 10).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"ct_post\" WHERE (\"title\", \"id\") > ($1, $2::uuid) \
             ORDER BY \"title\" ASC, \"id\" ASC LIMIT $3"
        );
    }

    #[test]
    fn select_list_keyset_with_filter_shifts_placeholders() {
        let s = Sort { column: "created_at".into(), dir: SortDir::Desc };
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "title", FieldKind::String, Op::Eq,
            FilterValue::Bound(BoundValue::Str("hi".into())),
        ))]);
        let id = Uuid::nil();
        let after = Some((BoundValue::Str("2024".into()), id));
        let (sql, binds) = super::select_list_keyset("post", &f, &s, after, 25).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"ct_post\" WHERE \"title\" = $1::text AND (\"created_at\", \"id\") < ($2, $3::uuid) \
             ORDER BY \"created_at\" DESC, \"id\" DESC LIMIT $4"
        );
        assert_eq!(
            binds,
            vec![
                BoundValue::Str("hi".into()),
                BoundValue::Str("2024".into()),
                BoundValue::Uuid(id),
                BoundValue::I64(25),
            ]
        );
    }
}

/// Emit a `WHERE` fragment plus the binds it consumes, starting at the
/// caller-supplied placeholder index (1-based). Returns an empty string and
/// no binds when the filter is empty (`Filter::None` or top-level
/// `Filter::All(vec![])`).
///
/// Walks the `Filter` tree recursively via `render_node`. Top-level
/// single-child groups elide their parens so phase 2.1/2.2 single-condition
/// SQL output stays byte-identical. Multi-child groups wrap each child in
/// `(...)` and join with ` AND ` (for `All`) or ` OR ` (for `Any`); `Not`
/// always wraps its child in `NOT (...)`.
///
/// Callers that interleave their own placeholders with the WHERE binds must
/// pass `start_placeholder = own_binds.len() + 1` and then number their own
/// placeholders after the filter's binds. `select_list` and `count` both pass
/// `1` because WHERE binds come first in their argument vectors.
pub fn render_where(
    filter: &Filter,
    start_placeholder: usize,
) -> Result<(String, Vec<BoundValue>), DmlError> {
    if matches!(filter, Filter::None) {
        return Ok((String::new(), vec![]));
    }
    // Treat top-level `All(vec![])` as `None` — matches phase 2.1/2.2 behavior.
    if let Filter::All(xs) = filter {
        if xs.is_empty() {
            return Ok((String::new(), vec![]));
        }
    }
    let mut buf = String::from(" WHERE ");
    let mut binds: Vec<BoundValue> = Vec::new();
    let mut placeholder = start_placeholder;
    render_node(filter, &mut buf, &mut binds, &mut placeholder)?;
    Ok((buf, binds))
}

/// Dispatch a single `Filter` node: appends one fragment to `buf` without
/// any outer parens. Caller (top-level `render_where` or `render_joined`)
/// owns the surrounding `WHERE ` or `(...)` framing.
fn render_node(
    node: &Filter,
    buf: &mut String,
    binds: &mut Vec<BoundValue>,
    placeholder: &mut usize,
) -> Result<(), DmlError> {
    match node {
        Filter::None => Err(DmlError::InvalidFilter("Filter::None inside group")),
        Filter::Leaf(c) => render_leaf(c, buf, binds, placeholder),
        // Top-level empty `All` is short-circuited in `render_where` for
        // phase 2.1/2.2 back-compat. Reaching these arms inside the tree
        // means the parser produced an empty group — a bug.
        Filter::All(xs) if xs.is_empty() => {
            Err(DmlError::InvalidFilter("empty $and group reached emitter"))
        }
        Filter::Any(xs) if xs.is_empty() => {
            Err(DmlError::InvalidFilter("empty $or group reached emitter"))
        }
        Filter::All(xs) if xs.len() == 1 => render_node(&xs[0], buf, binds, placeholder),
        Filter::Any(xs) if xs.len() == 1 => render_node(&xs[0], buf, binds, placeholder),
        Filter::All(xs) => render_joined(xs, " AND ", buf, binds, placeholder),
        Filter::Any(xs) => render_joined(xs, " OR ", buf, binds, placeholder),
        Filter::Not(inner) => {
            buf.push_str("NOT (");
            render_node(inner, buf, binds, placeholder)?;
            buf.push(')');
            Ok(())
        }
    }
}

/// Render a group's children: wraps each child in `(...)` and interleaves
/// `sep` between them. Caller has already verified `xs.len() >= 2`.
fn render_joined(
    xs: &[Filter],
    sep: &str,
    buf: &mut String,
    binds: &mut Vec<BoundValue>,
    placeholder: &mut usize,
) -> Result<(), DmlError> {
    for (i, child) in xs.iter().enumerate() {
        if i > 0 {
            buf.push_str(sep);
        }
        buf.push('(');
        render_node(child, buf, binds, placeholder)?;
        buf.push(')');
    }
    Ok(())
}

/// Emit one `Condition` as a SQL fragment. Pushes the per-leaf bind values
/// (if any) onto `binds` and advances `placeholder` by the number of binds
/// emitted. The fragment itself is appended to `buf` without surrounding
/// parens — group framing happens in `render_joined`.
fn render_leaf(
    c: &Condition,
    buf: &mut String,
    binds: &mut Vec<BoundValue>,
    placeholder: &mut usize,
) -> Result<(), DmlError> {
    let col = quote_ident(&c.column)?;
    let fragment = match (&c.op, &c.value) {
        (Op::Eq, FilterValue::Bound(BoundValue::Null(_))) => format!("{col} IS NULL"),
        (Op::Ne, FilterValue::Bound(BoundValue::Null(_))) => format!("{col} IS NOT NULL"),
        (Op::Eq, FilterValue::Bound(v)) => {
            let cast = pg_cast(c.kind);
            binds.push(v.clone());
            let p = *placeholder;
            *placeholder += 1;
            format!("{col} = ${p}::{cast}")
        }
        (Op::Ne, FilterValue::Bound(v)) => {
            let cast = pg_cast(c.kind);
            binds.push(v.clone());
            let p = *placeholder;
            *placeholder += 1;
            format!("{col} <> ${p}::{cast}")
        }
        (Op::IsNull, FilterValue::Null(true)) => format!("{col} IS NULL"),
        (Op::IsNull, FilterValue::Null(false)) => format!("{col} IS NOT NULL"),
        (Op::IsNull, FilterValue::Bound(_)) => {
            return Err(DmlError::InvalidFilter("IsNull requires Null(bool)"));
        }
        (Op::Eq | Op::Ne, FilterValue::Null(_)) => {
            return Err(DmlError::InvalidFilter("Eq/Ne require Bound value"));
        }
        (Op::Gt | Op::Gte | Op::Lt | Op::Lte, FilterValue::Bound(BoundValue::Null(_))) => {
            return Err(DmlError::InvalidFilter(
                "order op cannot compare against NULL",
            ));
        }
        (Op::Gt | Op::Gte | Op::Lt | Op::Lte, FilterValue::Bound(v)) => {
            let cast = pg_cast(c.kind);
            binds.push(v.clone());
            let p = *placeholder;
            *placeholder += 1;
            let sym = order_symbol(c.op);
            format!("{col} {sym} ${p}::{cast}")
        }
        (Op::Gt | Op::Gte | Op::Lt | Op::Lte, _) => {
            return Err(DmlError::InvalidFilter("order op requires Bound value"));
        }
        (Op::In | Op::NotIn, FilterValue::List(vs)) if vs.is_empty() => {
            return Err(DmlError::InvalidFilter("set op requires non-empty List"));
        }
        (Op::In | Op::NotIn, FilterValue::List(vs)) => {
            let cast = pg_cast(c.kind);
            let mut placeholders = Vec::with_capacity(vs.len());
            for v in vs {
                binds.push(v.clone());
                let p = *placeholder;
                *placeholder += 1;
                placeholders.push(format!("${p}::{cast}"));
            }
            let list = placeholders.join(", ");
            let op_str = if matches!(c.op, Op::In) {
                "IN"
            } else {
                "NOT IN"
            };
            format!("{col} {op_str} ({list})")
        }
        (Op::In | Op::NotIn, _) => {
            return Err(DmlError::InvalidFilter("set op requires List value"));
        }
        (Op::Contains | Op::StartsWith | Op::EndsWith, FilterValue::Bound(BoundValue::Str(s))) => {
            binds.push(BoundValue::Str(s.clone()));
            let p = *placeholder;
            *placeholder += 1;
            format!("{col} LIKE ${p}::text ESCAPE '\\'")
        }
        (Op::ContainsI, FilterValue::Bound(BoundValue::Str(s))) => {
            binds.push(BoundValue::Str(s.clone()));
            let p = *placeholder;
            *placeholder += 1;
            format!("{col} ILIKE ${p}::text ESCAPE '\\'")
        }
        (Op::Contains | Op::StartsWith | Op::EndsWith | Op::ContainsI, _) => {
            return Err(DmlError::InvalidFilter("string op requires Bound(Str)"));
        }
        (Op::Eq | Op::Ne | Op::IsNull, FilterValue::List(_)) => {
            return Err(DmlError::InvalidFilter("phase-2.1 op cannot take List"));
        }
    };
    buf.push_str(&fragment);
    Ok(())
}

#[cfg(test)]
mod where_tests {
    use super::*;
    use crate::filter::{Condition, Filter, FilterValue, Op};

    #[test]
    fn none_emits_empty() {
        let (sql, binds) = render_where(&Filter::None, 1).unwrap();
        assert_eq!(sql, "");
        assert!(binds.is_empty());
    }

    #[test]
    fn empty_all_emits_empty() {
        let (sql, binds) = render_where(&Filter::All(vec![]), 1).unwrap();
        assert_eq!(sql, "");
        assert!(binds.is_empty());
    }

    #[test]
    fn single_eq_string() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "title",
            FieldKind::String,
            Op::Eq,
            FilterValue::Bound(BoundValue::Str("hi".into())),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"title\" = $1::text");
        assert_eq!(binds, vec![BoundValue::Str("hi".into())]);
    }

    #[test]
    fn single_ne_integer() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::Ne,
            FilterValue::Bound(BoundValue::I64(0)),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"views\" <> $1::int8");
        assert_eq!(binds, vec![BoundValue::I64(0)]);
    }

    #[test]
    fn null_true() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "x",
            FieldKind::Integer,
            Op::IsNull,
            FilterValue::Null(true),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"x\" IS NULL");
        assert!(binds.is_empty());
    }

    #[test]
    fn null_false() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "x",
            FieldKind::Integer,
            Op::IsNull,
            FilterValue::Null(false),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"x\" IS NOT NULL");
        assert!(binds.is_empty());
    }

    #[test]
    fn eq_with_typed_null_rewrites_is_null() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "x",
            FieldKind::Integer,
            Op::Eq,
            FilterValue::Bound(BoundValue::Null(FieldKind::Integer)),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"x\" IS NULL");
        assert!(binds.is_empty());
    }

    #[test]
    fn ne_with_typed_null_rewrites_is_not_null() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "x",
            FieldKind::Integer,
            Op::Ne,
            FilterValue::Bound(BoundValue::Null(FieldKind::Integer)),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"x\" IS NOT NULL");
        assert!(binds.is_empty());
    }

    #[test]
    fn combined_and() {
        let f = Filter::All(vec![
            Filter::Leaf(Condition::new(
                "a",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Bound(BoundValue::I64(7)),
            )),
            Filter::Leaf(Condition::new(
                "b",
                FieldKind::String,
                Op::Ne,
                FilterValue::Bound(BoundValue::Str("x".into())),
            )),
            Filter::Leaf(Condition::new(
                "c",
                FieldKind::Boolean,
                Op::IsNull,
                FilterValue::Null(true),
            )),
        ]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE (\"a\" = $1::int8) AND (\"b\" <> $2::text) AND (\"c\" IS NULL)"
        );
        assert_eq!(binds, vec![BoundValue::I64(7), BoundValue::Str("x".into())]);
    }

    #[test]
    fn is_null_between_eq_skips_placeholder_correctly() {
        // Locks the invariant that `placeholder` increments only when a bind
        // is pushed: IsNull in the middle must not skip a `$N` number for the
        // following Eq, and total binds must match the placeholder count.
        let f = Filter::All(vec![
            Filter::Leaf(Condition::new(
                "a",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Bound(BoundValue::I64(1)),
            )),
            Filter::Leaf(Condition::new(
                "b",
                FieldKind::Boolean,
                Op::IsNull,
                FilterValue::Null(true),
            )),
            Filter::Leaf(Condition::new(
                "c",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Bound(BoundValue::I64(2)),
            )),
        ]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE (\"a\" = $1::int8) AND (\"b\" IS NULL) AND (\"c\" = $2::int8)"
        );
        assert_eq!(binds, vec![BoundValue::I64(1), BoundValue::I64(2)]);
    }

    #[test]
    fn placeholder_offset_respected() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "a",
            FieldKind::Integer,
            Op::Eq,
            FilterValue::Bound(BoundValue::I64(1)),
        ))]);
        let (sql, _binds) = render_where(&f, 5).unwrap();
        assert_eq!(sql, " WHERE \"a\" = $5::int8");
    }

    #[test]
    fn bad_identifier_rejected() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "Bad Name",
            FieldKind::Integer,
            Op::IsNull,
            FilterValue::Null(true),
        ))]);
        assert!(matches!(render_where(&f, 1), Err(DmlError::Ident(_))));
    }

    #[test]
    fn is_null_with_bound_value_rejected() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "a",
            FieldKind::Integer,
            Op::IsNull,
            FilterValue::Bound(BoundValue::I64(1)),
        ))]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn eq_with_null_filter_value_rejected() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "a",
            FieldKind::Integer,
            Op::Eq,
            FilterValue::Null(true),
        ))]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn gt_integer() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::Gt,
            FilterValue::Bound(BoundValue::I64(5)),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"views\" > $1::int8");
        assert_eq!(binds, vec![BoundValue::I64(5)]);
    }

    #[test]
    fn gte_float() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "score",
            FieldKind::Float,
            Op::Gte,
            FilterValue::Bound(BoundValue::F64(0.5)),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"score\" >= $1::float8");
        assert_eq!(binds, vec![BoundValue::F64(0.5)]);
    }

    #[test]
    fn lt_datetime() {
        use chrono::{DateTime, Utc};
        let t: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "created_at",
            FieldKind::Datetime,
            Op::Lt,
            FilterValue::Bound(BoundValue::DateTime(t)),
        ))]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"created_at\" < $1::timestamptz");
    }

    #[test]
    fn lte_integer() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::Lte,
            FilterValue::Bound(BoundValue::I64(100)),
        ))]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"views\" <= $1::int8");
    }

    #[test]
    fn order_op_rejects_typed_null() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::Gt,
            FilterValue::Bound(BoundValue::Null(FieldKind::Integer)),
        ))]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn order_op_rejects_filter_value_null() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::Gt,
            FilterValue::Null(true),
        ))]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn in_list_emits_parens() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::In,
            FilterValue::List(vec![
                BoundValue::I64(1),
                BoundValue::I64(2),
                BoundValue::I64(3),
            ]),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"views\" IN ($1::int8, $2::int8, $3::int8)");
        assert_eq!(
            binds,
            vec![BoundValue::I64(1), BoundValue::I64(2), BoundValue::I64(3)]
        );
    }

    #[test]
    fn not_in_string() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "category",
            FieldKind::String,
            Op::NotIn,
            FilterValue::List(vec![
                BoundValue::Str("a".into()),
                BoundValue::Str("b".into()),
            ]),
        ))]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"category\" NOT IN ($1::text, $2::text)");
    }

    #[test]
    fn empty_in_list_rejected() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::In,
            FilterValue::List(vec![]),
        ))]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn in_with_non_list_rejected() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::In,
            FilterValue::Bound(BoundValue::I64(1)),
        ))]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn in_placeholders_continue_after_other_binds() {
        let f = Filter::All(vec![
            Filter::Leaf(Condition::new(
                "title",
                FieldKind::String,
                Op::Eq,
                FilterValue::Bound(BoundValue::Str("x".into())),
            )),
            Filter::Leaf(Condition::new(
                "views",
                FieldKind::Integer,
                Op::In,
                FilterValue::List(vec![BoundValue::I64(1), BoundValue::I64(2)]),
            )),
        ]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE (\"title\" = $1::text) AND (\"views\" IN ($2::int8, $3::int8))"
        );
    }

    #[test]
    fn contains_uses_like_escape() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "title",
            FieldKind::String,
            Op::Contains,
            FilterValue::Bound(BoundValue::Str("%foo%".into())),
        ))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"title\" LIKE $1::text ESCAPE '\\'");
        assert_eq!(binds, vec![BoundValue::Str("%foo%".into())]);
    }

    #[test]
    fn containsi_uses_ilike() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "title",
            FieldKind::String,
            Op::ContainsI,
            FilterValue::Bound(BoundValue::Str("%foo%".into())),
        ))]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"title\" ILIKE $1::text ESCAPE '\\'");
    }

    #[test]
    fn starts_with_emits_like() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "slug",
            FieldKind::Text,
            Op::StartsWith,
            FilterValue::Bound(BoundValue::Str("blog-%".into())),
        ))]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"slug\" LIKE $1::text ESCAPE '\\'");
    }

    #[test]
    fn ends_with_emits_like() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "slug",
            FieldKind::Text,
            Op::EndsWith,
            FilterValue::Bound(BoundValue::Str("%-2026".into())),
        ))]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"slug\" LIKE $1::text ESCAPE '\\'");
    }

    #[test]
    fn string_op_rejects_non_string_bound() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "views",
            FieldKind::Integer,
            Op::Contains,
            FilterValue::Bound(BoundValue::I64(7)),
        ))]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn string_op_rejects_null_filter_value() {
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "title",
            FieldKind::String,
            Op::Contains,
            FilterValue::Null(true),
        ))]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn any_two_leaves() {
        let f = Filter::Any(vec![
            Filter::Leaf(Condition::new(
                "a",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Bound(BoundValue::I64(1)),
            )),
            Filter::Leaf(Condition::new(
                "b",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Bound(BoundValue::I64(2)),
            )),
        ]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE (\"a\" = $1::int8) OR (\"b\" = $2::int8)");
        assert_eq!(binds, vec![BoundValue::I64(1), BoundValue::I64(2)]);
    }

    #[test]
    fn not_wraps_single_leaf() {
        let f = Filter::Not(Box::new(Filter::Leaf(Condition::new(
            "a",
            FieldKind::Integer,
            Op::Eq,
            FilterValue::Bound(BoundValue::I64(1)),
        ))));
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE NOT (\"a\" = $1::int8)");
        assert_eq!(binds, vec![BoundValue::I64(1)]);
    }

    #[test]
    fn single_child_all_elides_parens() {
        // Phase 2.1/2.2 back-compat: parser wraps top-level leaves in
        // `All(vec![Leaf(...)])`; emitter must elide the wrap.
        let f = Filter::All(vec![Filter::Leaf(Condition::new(
            "a",
            FieldKind::Integer,
            Op::Eq,
            FilterValue::Bound(BoundValue::I64(1)),
        ))]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"a\" = $1::int8");
    }

    #[test]
    fn single_child_any_elides_parens() {
        let f = Filter::Any(vec![Filter::Leaf(Condition::new(
            "a",
            FieldKind::Integer,
            Op::Eq,
            FilterValue::Bound(BoundValue::I64(1)),
        ))]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"a\" = $1::int8");
    }

    #[test]
    fn nested_any_inside_all() {
        let f = Filter::All(vec![
            Filter::Leaf(Condition::new(
                "a",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Bound(BoundValue::I64(1)),
            )),
            Filter::Any(vec![
                Filter::Leaf(Condition::new(
                    "b",
                    FieldKind::Integer,
                    Op::Eq,
                    FilterValue::Bound(BoundValue::I64(2)),
                )),
                Filter::Leaf(Condition::new(
                    "c",
                    FieldKind::Integer,
                    Op::Eq,
                    FilterValue::Bound(BoundValue::I64(3)),
                )),
            ]),
        ]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE (\"a\" = $1::int8) AND ((\"b\" = $2::int8) OR (\"c\" = $3::int8))"
        );
        assert_eq!(
            binds,
            vec![BoundValue::I64(1), BoundValue::I64(2), BoundValue::I64(3)]
        );
    }

    #[test]
    fn not_wraps_group() {
        let f = Filter::Not(Box::new(Filter::Any(vec![
            Filter::Leaf(Condition::new(
                "a",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Bound(BoundValue::I64(1)),
            )),
            Filter::Leaf(Condition::new(
                "b",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Bound(BoundValue::I64(2)),
            )),
        ])));
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE NOT ((\"a\" = $1::int8) OR (\"b\" = $2::int8))");
    }

    #[test]
    fn empty_any_emitter_invariant_guard() {
        let f = Filter::Any(vec![]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn bind_ordering_across_nested_groups() {
        let f = Filter::Any(vec![
            Filter::All(vec![
                Filter::Leaf(Condition::new(
                    "a",
                    FieldKind::Integer,
                    Op::Eq,
                    FilterValue::Bound(BoundValue::I64(10)),
                )),
                Filter::Leaf(Condition::new(
                    "b",
                    FieldKind::Integer,
                    Op::Eq,
                    FilterValue::Bound(BoundValue::I64(20)),
                )),
            ]),
            Filter::Not(Box::new(Filter::Leaf(Condition::new(
                "c",
                FieldKind::Integer,
                Op::Eq,
                FilterValue::Bound(BoundValue::I64(30)),
            )))),
        ]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE ((\"a\" = $1::int8) AND (\"b\" = $2::int8)) OR (NOT (\"c\" = $3::int8))"
        );
        assert_eq!(
            binds,
            vec![
                BoundValue::I64(10),
                BoundValue::I64(20),
                BoundValue::I64(30)
            ]
        );
    }
}
