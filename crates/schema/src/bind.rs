//! Translate `Vec<BoundValue>` into a chained `sqlx::query::Query`.

use rustapi_core::BoundValue;
use sqlx::Postgres;

pub fn bind_all<'q>(
    mut q: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    values: &'q [BoundValue],
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
    for v in values {
        q = match v {
            BoundValue::Null => q.bind(Option::<String>::None),
            BoundValue::Str(s) => q.bind(s.as_str()),
            BoundValue::I64(i) => q.bind(*i),
            BoundValue::F64(f) => q.bind(*f),
            BoundValue::Bool(b) => q.bind(*b),
            BoundValue::DateTime(t) => q.bind(*t),
        };
    }
    q
}

pub fn bind_all_as<'q>(
    mut q: sqlx::query::QueryAs<'q, Postgres, (i64,), sqlx::postgres::PgArguments>,
    values: &'q [BoundValue],
) -> sqlx::query::QueryAs<'q, Postgres, (i64,), sqlx::postgres::PgArguments> {
    for v in values {
        q = match v {
            BoundValue::Null => q.bind(Option::<String>::None),
            BoundValue::Str(s) => q.bind(s.as_str()),
            BoundValue::I64(i) => q.bind(*i),
            BoundValue::F64(f) => q.bind(*f),
            BoundValue::Bool(b) => q.bind(*b),
            BoundValue::DateTime(t) => q.bind(*t),
        };
    }
    q
}
