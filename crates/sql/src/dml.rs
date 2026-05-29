//! DML string + bind-plan builders. Always returns `(String, Vec<BoundValue>)`.
//! HTTP layer translates `BoundValue` into sqlx binds.

use crate::filter::{Condition, Filter, FilterValue, Op};
use crate::ident::{quote_ident, table_name, IdentError};
use crate::sort::Sort;
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

/// `INSERT INTO ct_<name> (cols...) VALUES ($1, $2, ...) RETURNING *`
pub fn insert(ct: &ContentType, values: &BTreeMap<String, BoundValue>) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(&ct.name)?;
    let allowed: std::collections::HashSet<&str> = ct.fields.iter().map(|f| f.name.as_str()).collect();
    let mut cols = vec![];
    let mut placeholders = vec![];
    let mut binds = vec![];
    for (i, (name, val)) in values.iter().enumerate() {
        if !allowed.contains(name.as_str()) {
            return Err(DmlError::UnknownField(name.clone()));
        }
        cols.push(quote_ident(name)?);
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
    let allowed: std::collections::HashSet<&str> = ct.fields.iter().map(|f| f.name.as_str()).collect();
    let mut sets = vec![];
    let mut binds: Vec<BoundValue> = vec![];
    for (i, (name, val)) in values.iter().enumerate() {
        if !allowed.contains(name.as_str()) {
            return Err(DmlError::UnknownField(name.clone()));
        }
        let col = quote_ident(name)?;
        let placeholder = i + 1;
        sets.push(format!("{col} = ${placeholder}"));
        binds.push(val.clone());
    }
    sets.push("\"updated_at\" = now()".into());
    let id_placeholder = binds.len() + 1;
    binds.push(BoundValue::Str(id.to_string()));
    let sets_s = sets.join(", ");
    let sql = format!(
        "UPDATE {table} SET {sets_s} WHERE \"id\" = ${id_placeholder}::uuid RETURNING *"
    );
    Ok((sql, binds))
}

/// `DELETE FROM ct_<name> WHERE id=$1`
pub fn delete(ct_name: &str, id: Uuid) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let sql = format!("DELETE FROM {table} WHERE \"id\" = $1::uuid");
    Ok((sql, vec![BoundValue::Str(id.to_string())]))
}

/// `SELECT * FROM ct_<name> WHERE id=$1`
pub fn select_by_id(ct_name: &str, id: Uuid) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let sql = format!("SELECT * FROM {table} WHERE \"id\" = $1::uuid");
    Ok((sql, vec![BoundValue::Str(id.to_string())]))
}

/// `SELECT * FROM ct_<name> [WHERE ...] ORDER BY <col> <dir> LIMIT $1 OFFSET $2`
pub fn select_list(
    ct_name: &str,
    filter: &Filter,
    sort: &Sort,
    limit: i64,
    offset: i64,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let col = quote_ident(&sort.column)?;
    let dir = sort.dir.as_sql();

    let (where_sql, mut binds) = render_where(filter, 1)?;
    let limit_ph = binds.len() + 1;
    let offset_ph = binds.len() + 2;
    binds.push(BoundValue::I64(limit));
    binds.push(BoundValue::I64(offset));

    let sql = format!(
        "SELECT * FROM {table}{where_sql} ORDER BY {col} {dir} LIMIT ${limit_ph} OFFSET ${offset_ph}"
    );
    Ok((sql, binds))
}

/// `SELECT count(*) FROM ct_<name> [WHERE ...]`
pub fn count(ct_name: &str, filter: &Filter) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let (where_sql, binds) = render_where(filter, 1)?;
    Ok((format!("SELECT count(*) FROM {table}{where_sql}"), binds))
}

/// Postgres type-cast string for a FieldKind. Used by row-decoding helpers
/// and by `render_where` to type placeholders in filter conditions.
pub fn pg_cast(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::String | FieldKind::Text => "text",
        FieldKind::Integer => "int8",
        FieldKind::Float => "float8",
        FieldKind::Boolean => "bool",
        FieldKind::Datetime => "timestamptz",
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
        assert_eq!(sql, "INSERT INTO \"ct_post\" (\"title\") VALUES ($1) RETURNING *");
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
        let s = Sort { column: "created_at".into(), dir: SortDir::Desc };
        let (sql, binds) = select_list("post", &Filter::None, &s, 25, 50).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"ct_post\" ORDER BY \"created_at\" DESC LIMIT $1 OFFSET $2"
        );
        assert_eq!(binds, vec![BoundValue::I64(25), BoundValue::I64(50)]);
    }

    #[test]
    fn count_basic() {
        let (sql, binds) = count("post", &Filter::None).unwrap();
        assert_eq!(sql, "SELECT count(*) FROM \"ct_post\"");
        assert!(binds.is_empty());
    }

    #[test]
    fn select_list_with_filter_shifts_pagination() {
        let s = Sort { column: "created_at".into(), dir: SortDir::Desc };
        let f = Filter::All(vec![Condition::new(
            "title",
            Op::Eq,
            FilterValue::Bound(BoundValue::Str("hi".into())),
        )]);
        let (sql, binds) = select_list("post", &f, &s, 25, 50).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"ct_post\" WHERE \"title\" = $1::text ORDER BY \"created_at\" DESC LIMIT $2 OFFSET $3"
        );
        assert_eq!(
            binds,
            vec![BoundValue::Str("hi".into()), BoundValue::I64(25), BoundValue::I64(50)]
        );
    }

    #[test]
    fn count_with_filter() {
        let f = Filter::All(vec![Condition::new(
            "views",
            Op::Ne,
            FilterValue::Bound(BoundValue::I64(0)),
        )]);
        let (sql, binds) = count("post", &f).unwrap();
        assert_eq!(
            sql,
            "SELECT count(*) FROM \"ct_post\" WHERE \"views\" <> $1::int8"
        );
        assert_eq!(binds, vec![BoundValue::I64(0)]);
    }
}

/// Emit a `WHERE` fragment plus the binds it consumes, starting at the
/// caller-supplied placeholder index (1-based). Returns an empty string and
/// no binds when the filter is empty.
pub fn render_where(filter: &Filter, start_placeholder: usize) -> Result<(String, Vec<BoundValue>), DmlError> {
    let conds: &[Condition] = match filter {
        Filter::None => return Ok((String::new(), vec![])),
        Filter::All(c) if c.is_empty() => return Ok((String::new(), vec![])),
        Filter::All(c) => c,
    };

    let mut parts = Vec::with_capacity(conds.len());
    let mut binds = Vec::new();
    let mut placeholder = start_placeholder;

    for c in conds {
        let col = quote_ident(&c.column)?;
        let fragment = match (&c.op, &c.value) {
            (Op::Eq, FilterValue::Bound(BoundValue::Null(_))) => format!("{col} IS NULL"),
            (Op::Ne, FilterValue::Bound(BoundValue::Null(_))) => format!("{col} IS NOT NULL"),
            (Op::Eq, FilterValue::Bound(v)) => {
                let cast = pg_cast(kind_of(v));
                binds.push(v.clone());
                let p = placeholder;
                placeholder += 1;
                format!("{col} = ${p}::{cast}")
            }
            (Op::Ne, FilterValue::Bound(v)) => {
                let cast = pg_cast(kind_of(v));
                binds.push(v.clone());
                let p = placeholder;
                placeholder += 1;
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
        };
        parts.push(fragment);
    }

    Ok((format!(" WHERE {}", parts.join(" AND ")), binds))
}

fn kind_of(v: &BoundValue) -> FieldKind {
    match v {
        BoundValue::Null(k) => *k,
        BoundValue::Str(_) => FieldKind::Text,
        BoundValue::I64(_) => FieldKind::Integer,
        BoundValue::F64(_) => FieldKind::Float,
        BoundValue::Bool(_) => FieldKind::Boolean,
        BoundValue::DateTime(_) => FieldKind::Datetime,
    }
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
        let f = Filter::All(vec![Condition::new(
            "title",
            Op::Eq,
            FilterValue::Bound(BoundValue::Str("hi".into())),
        )]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"title\" = $1::text");
        assert_eq!(binds, vec![BoundValue::Str("hi".into())]);
    }

    #[test]
    fn single_ne_integer() {
        let f = Filter::All(vec![Condition::new(
            "views",
            Op::Ne,
            FilterValue::Bound(BoundValue::I64(0)),
        )]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"views\" <> $1::int8");
        assert_eq!(binds, vec![BoundValue::I64(0)]);
    }

    #[test]
    fn null_true() {
        let f = Filter::All(vec![Condition::new("x", Op::IsNull, FilterValue::Null(true))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"x\" IS NULL");
        assert!(binds.is_empty());
    }

    #[test]
    fn null_false() {
        let f = Filter::All(vec![Condition::new("x", Op::IsNull, FilterValue::Null(false))]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"x\" IS NOT NULL");
        assert!(binds.is_empty());
    }

    #[test]
    fn eq_with_typed_null_rewrites_is_null() {
        let f = Filter::All(vec![Condition::new(
            "x",
            Op::Eq,
            FilterValue::Bound(BoundValue::Null(FieldKind::Integer)),
        )]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"x\" IS NULL");
        assert!(binds.is_empty());
    }

    #[test]
    fn ne_with_typed_null_rewrites_is_not_null() {
        let f = Filter::All(vec![Condition::new(
            "x",
            Op::Ne,
            FilterValue::Bound(BoundValue::Null(FieldKind::Integer)),
        )]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"x\" IS NOT NULL");
        assert!(binds.is_empty());
    }

    #[test]
    fn combined_and() {
        let f = Filter::All(vec![
            Condition::new("a", Op::Eq, FilterValue::Bound(BoundValue::I64(7))),
            Condition::new("b", Op::Ne, FilterValue::Bound(BoundValue::Str("x".into()))),
            Condition::new("c", Op::IsNull, FilterValue::Null(true)),
        ]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE \"a\" = $1::int8 AND \"b\" <> $2::text AND \"c\" IS NULL"
        );
        assert_eq!(binds, vec![BoundValue::I64(7), BoundValue::Str("x".into())]);
    }

    #[test]
    fn is_null_between_eq_skips_placeholder_correctly() {
        // Locks the invariant that `placeholder` increments only when a bind
        // is pushed: IsNull in the middle must not skip a `$N` number for the
        // following Eq, and total binds must match the placeholder count.
        let f = Filter::All(vec![
            Condition::new("a", Op::Eq, FilterValue::Bound(BoundValue::I64(1))),
            Condition::new("b", Op::IsNull, FilterValue::Null(true)),
            Condition::new("c", Op::Eq, FilterValue::Bound(BoundValue::I64(2))),
        ]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE \"a\" = $1::int8 AND \"b\" IS NULL AND \"c\" = $2::int8"
        );
        assert_eq!(binds, vec![BoundValue::I64(1), BoundValue::I64(2)]);
    }

    #[test]
    fn placeholder_offset_respected() {
        let f = Filter::All(vec![Condition::new(
            "a",
            Op::Eq,
            FilterValue::Bound(BoundValue::I64(1)),
        )]);
        let (sql, _binds) = render_where(&f, 5).unwrap();
        assert_eq!(sql, " WHERE \"a\" = $5::int8");
    }

    #[test]
    fn bad_identifier_rejected() {
        let f = Filter::All(vec![Condition::new(
            "Bad Name",
            Op::IsNull,
            FilterValue::Null(true),
        )]);
        assert!(matches!(render_where(&f, 1), Err(DmlError::Ident(_))));
    }

    #[test]
    fn is_null_with_bound_value_rejected() {
        let f = Filter::All(vec![Condition::new(
            "a",
            Op::IsNull,
            FilterValue::Bound(BoundValue::I64(1)),
        )]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn eq_with_null_filter_value_rejected() {
        let f = Filter::All(vec![Condition::new("a", Op::Eq, FilterValue::Null(true))]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }
}
