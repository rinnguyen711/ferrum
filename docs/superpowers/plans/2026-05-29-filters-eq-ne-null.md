# Phase 2.1: Equality + Null Filters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `$eq`, `$ne`, `$null` filter operators to `GET /api/:type` using Strapi-style `?filters[col][$op]=value` syntax with implicit AND across multiple params.

**Architecture:** Extend the v1 `rustapi-sql::Filter` enum with an AND-of-conditions variant. Add a `render_where` SQL emitter shared by `select_list` and `count`. Add a parser in `rustapi-http::filter` that turns the raw query string into a `Filter`. Handler wires `RawQuery` → parser → SQL.

**Tech Stack:** Same as v1 — Rust 1.88, axum, sqlx, no new deps. `regex` (already a `rustapi-core` dep) is reused for the bracket parse.

**Prerequisites:** v1 implementation complete at HEAD c7f5094 or later.

**Spec:** `docs/superpowers/specs/2026-05-29-filters-eq-ne-null-design.md`

---

### Task 1: Extend `Filter` and add `Condition`/`Op`/`FilterValue` types

**Files:**
- Modify: `crates/sql/src/filter.rs`
- Test: inline

- [ ] **Step 1: Replace `crates/sql/src/filter.rs`**

```rust
//! Filter expressions. v1 shipped `None` only. Phase 2.1 adds equality + null
//! ops combined with implicit AND. Combinators (OR / NOT) land in phase 2.3.

use rustapi_core::BoundValue;

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Filter {
    #[default]
    None,
    /// Implicit AND across conditions. An empty vec behaves like `None`.
    All(Vec<Condition>),
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Condition {
    /// Already validated as an identifier by upstream callers. The SQL emitter
    /// re-validates via `quote_ident`.
    pub column: String,
    pub op: Op,
    pub value: FilterValue,
}

impl Condition {
    pub fn new(column: impl Into<String>, op: Op, value: FilterValue) -> Self {
        Self { column: column.into(), op, value }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
    Eq,
    Ne,
    IsNull,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum FilterValue {
    /// Used by `$eq` / `$ne`. When the inner `BoundValue` is `Null(kind)` the
    /// emitter rewrites to `IS NULL` / `IS NOT NULL`.
    Bound(BoundValue),
    /// Used by `$null`: true = IS NULL, false = IS NOT NULL.
    Null(bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_none() {
        assert!(matches!(Filter::default(), Filter::None));
    }

    #[test]
    fn condition_new_builds_struct() {
        let c = Condition::new("title", Op::Eq, FilterValue::Null(true));
        assert_eq!(c.column, "title");
        assert_eq!(c.op, Op::Eq);
    }
}
```

- [ ] **Step 2: Run unit tests**

Run: `cargo test -p rustapi-sql --lib filter`
Expected: PASS — 2 tests.

- [ ] **Step 3: Verify clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS, no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/sql/src/filter.rs
git commit -m "feat(sql): extend Filter with Condition/Op/FilterValue types"
```

---

### Task 2: Re-export new types + revive `pg_cast` callsite

**Files:**
- Modify: `crates/sql/src/lib.rs`

- [ ] **Step 1: Replace `crates/sql/src/lib.rs`**

```rust
#![forbid(unsafe_code)]

pub mod ddl;
pub mod dml;
pub mod filter;
pub mod ident;
pub mod sort;

pub use ddl::{add_column, create_table, drop_column, drop_table, DdlError};
pub use dml::{count, delete, insert, pg_cast, select_by_id, select_list, update, DmlError, SqlAndBinds};
pub use filter::{Condition, Filter, FilterValue, Op};
pub use ident::{quote_ident, table_name, IdentError};
pub use sort::{Sort, SortDir};
```

- [ ] **Step 2: Build workspace**

Run: `cargo build --workspace`
Expected: PASS.

- [ ] **Step 3: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/sql/src/lib.rs
git commit -m "feat(sql): re-export Filter, Condition, Op, FilterValue"
```

---

### Task 3: Add `DmlError::InvalidFilter` and `render_where` emitter

**Files:**
- Modify: `crates/sql/src/dml.rs`
- Test: inline (golden-string, no DB)

- [ ] **Step 1: Replace the `DmlError` enum** in `crates/sql/src/dml.rs`

Find:

```rust
#[derive(Debug, thiserror::Error)]
pub enum DmlError {
    #[error(transparent)]
    Ident(#[from] IdentError),
    #[error("unknown field `{0}` in payload")]
    UnknownField(String),
}
```

Replace with:

```rust
#[derive(Debug, thiserror::Error)]
pub enum DmlError {
    #[error(transparent)]
    Ident(#[from] IdentError),
    #[error("unknown field `{0}` in payload")]
    UnknownField(String),
    #[error("invalid filter: {0}")]
    InvalidFilter(&'static str),
}
```

- [ ] **Step 2: Add `render_where` and tests** — append to `crates/sql/src/dml.rs`:

```rust
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
```

- [ ] **Step 3: Ensure imports compile** — `crates/sql/src/dml.rs` already imports `BoundValue, ContentType, FieldKind`. No new imports needed.

- [ ] **Step 4: Run tests**

Run: `cargo test -p rustapi-sql --lib`
Expected: PASS — prior 21 + new 13 = 34 tests.

- [ ] **Step 5: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/sql/src/dml.rs
git commit -m "feat(sql): render_where emitter for filter conditions"
```

---

### Task 4: Wire filter into `select_list` and `count`

**Files:**
- Modify: `crates/sql/src/dml.rs`
- Test: extend existing `mod tests` for dml

- [ ] **Step 1: Replace `select_list`** in `crates/sql/src/dml.rs`

Find:

```rust
pub fn select_list(
    ct_name: &str,
    _filter: &Filter,
    sort: &Sort,
    limit: i64,
    offset: i64,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let col = quote_ident(&sort.column)?;
    let dir = sort.dir.as_sql();
    // Filter::None in v1 — no WHERE clause emitted.
    let sql = format!(
        "SELECT * FROM {table} ORDER BY {col} {dir} LIMIT $1 OFFSET $2"
    );
    Ok((sql, vec![BoundValue::I64(limit), BoundValue::I64(offset)]))
}
```

Replace with:

```rust
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
```

- [ ] **Step 2: Replace `count`** in `crates/sql/src/dml.rs`

Find:

```rust
pub fn count(ct_name: &str, _filter: &Filter) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    Ok((format!("SELECT count(*) FROM {table}"), vec![]))
}
```

Replace with:

```rust
pub fn count(ct_name: &str, filter: &Filter) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let (where_sql, binds) = render_where(filter, 1)?;
    Ok((format!("SELECT count(*) FROM {table}{where_sql}"), binds))
}
```

- [ ] **Step 3: Add new tests** — append inside the existing `mod tests` block in `crates/sql/src/dml.rs`:

```rust
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rustapi-sql --lib`
Expected: PASS — 36 tests.

- [ ] **Step 5: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS, no warnings (no breaking changes to call sites since signatures kept).

- [ ] **Step 6: Commit**

```bash
git add crates/sql/src/dml.rs
git commit -m "feat(sql): select_list and count emit WHERE from Filter"
```

---

### Task 5: Add filter parser module

**Files:**
- Create: `crates/http/src/filter.rs`
- Modify: `crates/http/src/lib.rs`

- [ ] **Step 1: Write `crates/http/src/filter.rs`**

```rust
//! Strapi-style `?filters[col][$op]=value` parser. Produces a `rustapi_sql::Filter`
//! ready for the SQL builder. v1 supports `$eq`, `$ne`, `$null` with implicit
//! AND across params.

use rustapi_core::{is_system_column, BoundValue, ContentType, Error, Field, FieldKind, ValidationErrors};
use rustapi_sql::{Condition, Filter, FilterValue, Op};
use std::collections::HashSet;
use std::sync::OnceLock;
use url::form_urlencoded;

/// Parse a raw query string into a `Filter`. Non-filter params are ignored.
/// Returns `Filter::None` if no filter params are present.
pub fn parse(raw_query: &str, ct: &ContentType) -> Result<Filter, Error> {
    let mut seen: HashSet<(String, Op)> = HashSet::new();
    let mut conds: Vec<Condition> = Vec::new();

    for (k, v) in form_urlencoded::parse(raw_query.as_bytes()) {
        if !k.starts_with("filters[") {
            continue;
        }
        let (col, op_str) = parse_key(&k)?;
        let op = match op_str.as_str() {
            "$eq" => Op::Eq,
            "$ne" => Op::Ne,
            "$null" => Op::IsNull,
            other => {
                return Err(Error::Validation(ValidationErrors::field(
                    col,
                    format!("unknown operator `{other}`"),
                )));
            }
        };

        let field = field_for(ct, &col)?;
        if !seen.insert((col.clone(), op)) {
            return Err(Error::Validation(ValidationErrors::field(
                col,
                "duplicate filter operator on column",
            )));
        }

        let value = coerce_value(field, op, &col, &v)?;
        conds.push(Condition::new(col, op, value));
    }

    if conds.is_empty() {
        Ok(Filter::None)
    } else {
        Ok(Filter::All(conds))
    }
}

fn parse_key(k: &str) -> Result<(String, String), Error> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"^filters\[(?P<col>[^\[\]]+)\]\[(?P<op>\$[a-z]+)\]$").unwrap()
    });
    let caps = re.captures(k).ok_or_else(|| {
        Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}`"
        )))
    })?;
    Ok((caps["col"].to_string(), caps["op"].to_string()))
}

fn field_for<'a>(ct: &'a ContentType, col: &str) -> Result<FieldOrSystem<'a>, Error> {
    if is_system_column(col) {
        return Ok(FieldOrSystem::System(system_kind(col)));
    }
    if let Some(f) = ct.fields.iter().find(|f| f.name == col) {
        return Ok(FieldOrSystem::User(f));
    }
    Err(Error::Validation(ValidationErrors::field(
        col,
        "unknown filter field",
    )))
}

enum FieldOrSystem<'a> {
    User(&'a Field),
    System(FieldKind),
}

impl FieldOrSystem<'_> {
    fn kind(&self) -> FieldKind {
        match self {
            FieldOrSystem::User(f) => f.kind,
            FieldOrSystem::System(k) => *k,
        }
    }
}

fn system_kind(col: &str) -> FieldKind {
    // `id` is a UUID exposed as a string; match it as Text for coercion.
    match col {
        "id" => FieldKind::Text,
        "created_at" | "updated_at" => FieldKind::Datetime,
        _ => FieldKind::Text,
    }
}

fn coerce_value(field: FieldOrSystem<'_>, op: Op, col: &str, raw: &str) -> Result<FilterValue, Error> {
    let kind = field.kind();
    match op {
        Op::IsNull => parse_bool(raw)
            .map(FilterValue::Null)
            .map_err(|reason| Error::Validation(ValidationErrors::field(col, reason))),
        Op::Eq | Op::Ne => {
            if raw.eq_ignore_ascii_case("null") {
                return Ok(FilterValue::Bound(BoundValue::Null(kind)));
            }
            coerce_bound(kind, col, raw).map(FilterValue::Bound)
        }
    }
}

fn coerce_bound(kind: FieldKind, col: &str, raw: &str) -> Result<BoundValue, Error> {
    let v = match kind {
        FieldKind::String | FieldKind::Text => BoundValue::Str(raw.to_string()),
        FieldKind::Integer => raw
            .parse::<i64>()
            .map(BoundValue::I64)
            .map_err(|_| field_err(col, "expected integer"))?,
        FieldKind::Float => raw
            .parse::<f64>()
            .map(BoundValue::F64)
            .map_err(|_| field_err(col, "expected number"))?,
        FieldKind::Boolean => parse_bool(raw)
            .map(BoundValue::Bool)
            .map_err(|reason| field_err(col, reason))?,
        FieldKind::Datetime => chrono::DateTime::parse_from_rfc3339(raw)
            .map(|t| BoundValue::DateTime(t.with_timezone(&chrono::Utc)))
            .map_err(|_| field_err(col, "expected RFC3339 datetime"))?,
        _ => return Err(field_err(col, "unsupported kind for filter")),
    };
    Ok(v)
}

fn parse_bool(raw: &str) -> Result<bool, String> {
    match raw.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("expected `true` or `false`".into()),
    }
}

fn field_err(col: &str, reason: impl Into<String>) -> Error {
    Error::Validation(ValidationErrors::field(col, reason))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustapi_core::{Field, FieldKind};
    use serde_json::json;
    use uuid::Uuid;

    fn ct() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![
                Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: true,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "views".into(),
                    kind: FieldKind::Integer,
                    required: false,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "published".into(),
                    kind: FieldKind::Boolean,
                    required: false,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn empty_returns_none() {
        let f = parse("", &ct()).unwrap();
        assert!(matches!(f, Filter::None));
    }

    #[test]
    fn ignores_non_filter_params() {
        let f = parse("page=1&pageSize=25&sort=created_at:desc", &ct()).unwrap();
        assert!(matches!(f, Filter::None));
    }

    #[test]
    fn single_eq_string() {
        let f = parse("filters[title][$eq]=hi", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!("expected All") };
        assert_eq!(conds.len(), 1);
        assert_eq!(conds[0].column, "title");
        assert_eq!(conds[0].op, Op::Eq);
    }

    #[test]
    fn integer_coerces() {
        let f = parse("filters[views][$ne]=7", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        match &conds[0].value {
            FilterValue::Bound(BoundValue::I64(n)) => assert_eq!(*n, 7),
            other => panic!("expected I64, got {other:?}"),
        }
    }

    #[test]
    fn bad_integer_rejected() {
        let err = parse("filters[views][$eq]=not-a-number", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn unknown_field_rejected() {
        let err = parse("filters[ghost][$eq]=1", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn unknown_op_rejected() {
        let err = parse("filters[title][$bogus]=hi", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn malformed_bracket_rejected() {
        let err = parse("filters[title]=hi", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn null_true_and_false() {
        let f = parse("filters[views][$null]=true", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert!(matches!(conds[0].value, FilterValue::Null(true)));

        let f = parse("filters[views][$null]=false", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert!(matches!(conds[0].value, FilterValue::Null(false)));
    }

    #[test]
    fn null_value_invalid() {
        let err = parse("filters[views][$null]=maybe", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn eq_null_rewrites_to_typed_null() {
        let f = parse("filters[views][$eq]=null", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Null(k)) => assert_eq!(*k, FieldKind::Integer),
            other => panic!("expected typed Null, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_col_op_rejected() {
        let err = parse(
            "filters[views][$eq]=1&filters[views][$eq]=2",
            &ct(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn same_col_different_ops_allowed() {
        let f = parse("filters[views][$eq]=1&filters[views][$ne]=5", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert_eq!(conds.len(), 2);
    }

    #[test]
    fn boolean_case_insensitive() {
        let f = parse("filters[published][$eq]=True", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert!(matches!(conds[0].value, FilterValue::Bound(BoundValue::Bool(true))));
    }

    #[test]
    fn system_column_filterable() {
        let f = parse("filters[id][$null]=false", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert_eq!(conds[0].column, "id");
    }
}
```

- [ ] **Step 2: Add `url` dependency** — append to `[dependencies]` in `crates/http/Cargo.toml`:

```toml
url = "2"
```

- [ ] **Step 3: Wire module** — append to `crates/http/src/lib.rs`:

```rust
pub mod filter;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rustapi-http --lib filter`
Expected: PASS — 14 tests.

- [ ] **Step 5: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/http Cargo.lock
git commit -m "feat(http): filter parser for ?filters[col][\$op]=value"
```

---

### Task 6: Wire parser into the list handler

**Files:**
- Modify: `crates/http/src/routes/content.rs`

- [ ] **Step 1: Update the `list` handler** in `crates/http/src/routes/content.rs`

Find:

```rust
async fn list(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    Query(params): Query<ListParams>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentRead, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let opts = parse_list(&ct, params, state.config.page_size_max)?;
    let offset: i64 = ((opts.page - 1) as i64) * (opts.page_size as i64);

    let (list_sql, list_binds) = rustapi_sql::select_list(
        &ct.name,
        &rustapi_sql::Filter::None,
        &opts.sort,
        opts.page_size as i64,
        offset,
    )
    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let q = bind_all(sqlx::query(&list_sql), &list_binds);
    let rows = q.fetch_all(&state.pool).await.map_err(db)?;

    let mut data = Vec::with_capacity(rows.len());
    for r in &rows {
        data.push(row_to_json(&ct, r)?);
    }

    let (count_sql, count_binds) =
        rustapi_sql::count(&ct.name, &rustapi_sql::Filter::None)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let cq = bind_all_as(sqlx::query_as::<_, (i64,)>(&count_sql), &count_binds);
    let total: i64 = cq.fetch_one(&state.pool).await.map_err(db)?.0;

    Ok(Json(json!({
        "data": data,
        "meta": {
            "page": opts.page,
            "pageSize": opts.page_size,
            "total": total
        }
    })))
}
```

Replace with:

```rust
async fn list(
    State(state): State<AppState>,
    Path(ct_name): Path<String>,
    Query(params): Query<ListParams>,
    axum::extract::RawQuery(raw_query): axum::extract::RawQuery,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentRead, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    let opts = parse_list(&ct, params, state.config.page_size_max)?;
    let offset: i64 = ((opts.page - 1) as i64) * (opts.page_size as i64);

    let filter = crate::filter::parse(raw_query.as_deref().unwrap_or(""), &ct)?;

    let (list_sql, list_binds) = rustapi_sql::select_list(
        &ct.name,
        &filter,
        &opts.sort,
        opts.page_size as i64,
        offset,
    )
    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let q = bind_all(sqlx::query(&list_sql), &list_binds);
    let rows = q.fetch_all(&state.pool).await.map_err(db)?;

    let mut data = Vec::with_capacity(rows.len());
    for r in &rows {
        data.push(row_to_json(&ct, r)?);
    }

    let (count_sql, count_binds) = rustapi_sql::count(&ct.name, &filter)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let cq = bind_all_as(sqlx::query_as::<_, (i64,)>(&count_sql), &count_binds);
    let total: i64 = cq.fetch_one(&state.pool).await.map_err(db)?.0;

    Ok(Json(json!({
        "data": data,
        "meta": {
            "page": opts.page,
            "pageSize": opts.page_size,
            "total": total
        }
    })))
}
```

- [ ] **Step 2: Build workspace**

Run: `cargo build --workspace`
Expected: PASS.

- [ ] **Step 3: Run existing http + content tests**

Run: `cargo test -p rustapi-http`
Expected: PASS — all existing http unit tests still pass.

Run: `cargo test -p rustapi --test integration_content`
Expected: PASS — all 7 content integration tests still pass (filter param absent → `Filter::None` path).

- [ ] **Step 4: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/routes/content.rs
git commit -m "feat(http): wire filter parser into list handler"
```

---

### Task 7: End-to-end integration tests

**Files:**
- Create: `crates/bin/tests/integration_filters.rs`

- [ ] **Step 1: Write `crates/bin/tests/integration_filters.rs`**

```rust
mod common;
use common::TestApp;
use serde_json::{json, Value};

async fn make_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string", "required": true, "max_length": 64},
                {"name": "views", "kind": "integer"},
                {"name": "published", "kind": "boolean", "default": false},
                {"name": "category", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
}

async fn seed(app: &TestApp) {
    let rows = vec![
        json!({"title": "a", "views": 0,    "published": true,  "category": "x"}),
        json!({"title": "b", "views": 5,    "published": false, "category": "x"}),
        json!({"title": "c", "views": 10,   "published": true,  "category": "y"}),
        json!({"title": "d", "views": null, "published": true,  "category": "y"}),
        json!({"title": "e", "views": 20,   "published": false, "category": null}),
    ];
    for row in rows {
        let resp = app
            .admin(app.client.post(app.url("/api/post")))
            .json(&row)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    }
}

async fn list_body(app: &TestApp, query: &str) -> Value {
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post?{query}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    resp.json().await.unwrap()
}

#[tokio::test]
async fn eq_string_filter() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$eq]=c").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "c");
}

#[tokio::test]
async fn ne_integer_filter() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$ne]=0").await;
    // SQL `<>` excludes NULL — d (views=null) is NOT returned. a (views=0) is filtered.
    // Remaining: b (5), c (10), e (20).
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn null_true_returns_nulls() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$null]=true").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "d");
}

#[tokio::test]
async fn null_false_returns_non_nulls() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$null]=false").await;
    assert_eq!(body["meta"]["total"], 4);
}

#[tokio::test]
async fn implicit_and_combines() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[category][$eq]=x&filters[published][$eq]=true").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "a");
}

#[tokio::test]
async fn count_reflects_filter() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[published][$eq]=true").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn pagination_and_filter_compose() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[published][$eq]=true&page=1&pageSize=2&sort=views:asc",
    )
    .await;
    assert_eq!(body["meta"]["total"], 3);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    // published=true rows by views asc: a(0), c(10), d(null treated last by asc nullslast? Postgres default is NULLS LAST for ASC).
    assert_eq!(body["data"][0]["title"], "a");
    assert_eq!(body["data"][1]["title"], "c");
}

#[tokio::test]
async fn eq_null_rewrites_to_is_null() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$eq]=null").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "d");
}

#[tokio::test]
async fn unknown_field_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[ghost][$eq]=1")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn unknown_op_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[title][$bogus]=1")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn malformed_int_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[views][$eq]=abc")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn duplicate_col_op_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[views][$eq]=1&filters[views][$eq]=2")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p rustapi --test integration_filters`
Expected: PASS — 12 tests.

- [ ] **Step 3: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/bin/tests/integration_filters.rs
git commit -m "test(bin): integration coverage for \$eq \$ne \$null filters"
```

---

### Task 8: Full workspace verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run all workspace tests**

Run: `cargo test --workspace`
Expected: PASS — 87 prior + 13 new sql unit + 14 new http unit + 12 new integration = **126 tests** across the workspace.

If any prior test fails, do NOT mark complete. Open a follow-up task to fix the regression, then re-run.

- [ ] **Step 2: Final clippy sweep**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS, zero warnings.

- [ ] **Step 3: Confirm git status clean**

Run: `git status`
Expected: working tree clean.

---

## Self-Review Notes

- Spec §2.1 wire format → Task 5 parser (regex + bracket extraction).
- Spec §2.1 value coercion table → Task 5 `coerce_value` / `coerce_bound`.
- Spec §2.1 `$eq=null` rewrite → Task 5 + verified end-to-end Task 7 `eq_null_rewrites_to_is_null`.
- Spec §2.2 whitelist (user + system columns) → Task 5 `field_for` / `is_system_column`.
- Spec §2.3 op-kind compatibility seam → Task 5 currently no-op (all three ops apply to all kinds); future ops slot into `coerce_value`.
- Spec §3.1 Filter/Condition/Op/FilterValue types → Task 1.
- Spec §3.2 WHERE emission matrix → Task 3 `render_where` + tests.
- Spec §3.3 `select_list` / `count` placeholder shifting → Task 4.
- Spec §3.4 `DmlError::InvalidFilter` → Task 3.
- Spec §3.5 unit tests → Task 3 (13 tests) + Task 4 (2 tests).
- Spec §4.1 parser unit tests → Task 5 (14 tests).
- Spec §4.2 handler wiring with RawQuery alongside Query<ListParams> → Task 6.
- Spec §5 integration tests → Task 7 (12 tests covering all listed scenarios).
- Spec §6 error shape — `ValidationErrors::field` is already used; existing IntoResponse path emits `details.fields[...]`.
- Spec §7 out-of-scope items confirmed untouched (PUT, DDL mapping, RBAC, TraceLayer, JsonRejection).
- Backwards compat: no filter params → parser returns `Filter::None` → SQL builders take the existing zero-WHERE branch, so v1 integration tests stay green.
- `regex` already in workspace via `rustapi-core`. `url` is the one new dep (small, well-known, used for query decoding so we don't reinvent URL parsing).
