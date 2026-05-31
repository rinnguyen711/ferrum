//! Translate `Vec<BoundValue>` into a chained `sqlx::query::Query`.

use rustapi_core::{BoundValue, FieldKind};
use sqlx::Postgres;

pub fn bind_all<'q>(
    mut q: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    values: &'q [BoundValue],
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
    for v in values {
        q = bind_one(q, v);
    }
    q
}

pub fn bind_all_as<'q>(
    mut q: sqlx::query::QueryAs<'q, Postgres, (i64,), sqlx::postgres::PgArguments>,
    values: &'q [BoundValue],
) -> sqlx::query::QueryAs<'q, Postgres, (i64,), sqlx::postgres::PgArguments> {
    for v in values {
        q = bind_one_as(q, v);
    }
    q
}

fn bind_one<'q>(
    q: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    v: &'q BoundValue,
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
    match v {
        BoundValue::Null(kind) => bind_typed_null(q, *kind),
        BoundValue::Str(s) => q.bind(s.as_str()),
        BoundValue::I64(i) => q.bind(*i),
        BoundValue::F64(f) => q.bind(*f),
        BoundValue::Bool(b) => q.bind(*b),
        BoundValue::DateTime(t) => q.bind(*t),
        BoundValue::Uuid(u) => q.bind(*u),
        BoundValue::Json(j) => q.bind(sqlx::types::Json(j)),
    }
}

fn bind_one_as<'q>(
    q: sqlx::query::QueryAs<'q, Postgres, (i64,), sqlx::postgres::PgArguments>,
    v: &'q BoundValue,
) -> sqlx::query::QueryAs<'q, Postgres, (i64,), sqlx::postgres::PgArguments> {
    match v {
        BoundValue::Null(kind) => bind_typed_null_as(q, *kind),
        BoundValue::Str(s) => q.bind(s.as_str()),
        BoundValue::I64(i) => q.bind(*i),
        BoundValue::F64(f) => q.bind(*f),
        BoundValue::Bool(b) => q.bind(*b),
        BoundValue::DateTime(t) => q.bind(*t),
        BoundValue::Uuid(u) => q.bind(*u),
        BoundValue::Json(j) => q.bind(sqlx::types::Json(j)),
    }
}

fn bind_typed_null<'q>(
    q: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    kind: FieldKind,
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
    match kind {
        FieldKind::String | FieldKind::Text => q.bind(Option::<String>::None),
        FieldKind::Integer => q.bind(Option::<i64>::None),
        FieldKind::Float => q.bind(Option::<f64>::None),
        FieldKind::Boolean => q.bind(Option::<bool>::None),
        FieldKind::Datetime => q.bind(Option::<chrono::DateTime<chrono::Utc>>::None),
        FieldKind::Uuid => q.bind(Option::<uuid::Uuid>::None),
        _ => q.bind(Option::<String>::None),
    }
}

fn bind_typed_null_as<'q>(
    q: sqlx::query::QueryAs<'q, Postgres, (i64,), sqlx::postgres::PgArguments>,
    kind: FieldKind,
) -> sqlx::query::QueryAs<'q, Postgres, (i64,), sqlx::postgres::PgArguments> {
    match kind {
        FieldKind::String | FieldKind::Text => q.bind(Option::<String>::None),
        FieldKind::Integer => q.bind(Option::<i64>::None),
        FieldKind::Float => q.bind(Option::<f64>::None),
        FieldKind::Boolean => q.bind(Option::<bool>::None),
        FieldKind::Datetime => q.bind(Option::<chrono::DateTime<chrono::Utc>>::None),
        FieldKind::Uuid => q.bind(Option::<uuid::Uuid>::None),
        _ => q.bind(Option::<String>::None),
    }
}
