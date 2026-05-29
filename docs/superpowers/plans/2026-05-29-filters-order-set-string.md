# Phase 2.2: Order + Set + String Filter Operators Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 11 filter operators (`$gt $gte $lt $lte $in $nin $contains $startsWith $endsWith $containsi`) to `GET /api/:type`, composing with the phase 2.1 operators via implicit AND.

**Architecture:** Extend the existing `Op` enum and add a `FilterValue::List` variant in `rustapi-sql`. Split `render_where`'s per-op match into small per-group helpers. Extend the parser regex to allow optional `[idx]` and mixed-case op names. Enforce a strict op-kind matrix at parse time; escape LIKE metacharacters before binding.

**Tech Stack:** Same as v1 / phase 2.1 — Rust 1.88, axum, sqlx. No new deps.

**Prerequisites:** Phase 2.1 (`docs/superpowers/plans/2026-05-29-filters-eq-ne-null.md`) shipped.

**Spec:** `docs/superpowers/specs/2026-05-29-filters-order-set-string-design.md`

---

### Task 1: Extend `Op` and `FilterValue`; add `op_allows_kind`

**Files:**
- Modify: `crates/sql/src/filter.rs`

- [ ] **Step 1: Replace `crates/sql/src/filter.rs`** with:

```rust
//! Filter expressions. Phase 2.1 shipped `$eq` / `$ne` / `$null` combined with
//! implicit AND. Phase 2.2 adds order / set / string operators. Combinators
//! (`$or` / `$not`) land in phase 2.3.

use rustapi_core::{BoundValue, FieldKind};

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Filter {
    #[default]
    None,
    /// Implicit AND across conditions. An empty vec behaves like `None`.
    All(Vec<Condition>),
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Condition {
    /// Already validated as an identifier by upstream callers. The SQL emitter
    /// re-validates via `quote_ident`.
    pub column: String,
    /// Column kind, used by `render_where` to pick the right Postgres cast.
    pub kind: FieldKind,
    pub op: Op,
    pub value: FilterValue,
}

impl Condition {
    pub fn new(column: impl Into<String>, kind: FieldKind, op: Op, value: FilterValue) -> Self {
        Self { column: column.into(), kind, op, value }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Op {
    // Phase 2.1
    Eq,
    Ne,
    IsNull,
    // Phase 2.2 — order
    Gt,
    Gte,
    Lt,
    Lte,
    // Phase 2.2 — set
    In,
    NotIn,
    // Phase 2.2 — string
    Contains,
    StartsWith,
    EndsWith,
    ContainsI,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FilterValue {
    /// Used by `$eq` / `$ne` / order ops / string ops. When the inner
    /// `BoundValue` is `Null(kind)` the emitter rewrites `Eq`/`Ne` to
    /// `IS NULL` / `IS NOT NULL` (phase 2.1 behavior).
    Bound(BoundValue),
    /// Used by `$null`: true = IS NULL, false = IS NOT NULL.
    Null(bool),
    /// Used by `$in` / `$nin`. Empty list is rejected by the parser
    /// AND defensively re-checked by `render_where`.
    List(Vec<BoundValue>),
}

/// True iff `op` is meaningful for `kind`. The parser enforces the rejection;
/// the emitter trusts this contract.
pub fn op_allows_kind(op: Op, kind: FieldKind) -> bool {
    use FieldKind::*;
    use Op::*;
    match op {
        Eq | Ne | IsNull => matches!(
            kind,
            String | Text | Integer | Float | Boolean | Datetime | Uuid
        ),
        Gt | Gte | Lt | Lte => matches!(kind, Integer | Float | Datetime),
        In | NotIn => matches!(
            kind,
            String | Text | Integer | Float | Boolean | Datetime | Uuid
        ),
        Contains | StartsWith | EndsWith | ContainsI => {
            matches!(kind, String | Text)
        }
        // FieldKind and Op are both #[non_exhaustive]; future variants land
        // here and must explicitly opt into a matrix entry.
        _ => false,
    }
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
        let c = Condition::new("title", FieldKind::String, Op::Eq, FilterValue::Null(true));
        assert_eq!(c.column, "title");
        assert_eq!(c.kind, FieldKind::String);
        assert_eq!(c.op, Op::Eq);
    }

    #[test]
    fn op_allows_kind_order_only_on_numeric_and_datetime() {
        for kind in [FieldKind::Integer, FieldKind::Float, FieldKind::Datetime] {
            assert!(op_allows_kind(Op::Gt, kind));
            assert!(op_allows_kind(Op::Gte, kind));
            assert!(op_allows_kind(Op::Lt, kind));
            assert!(op_allows_kind(Op::Lte, kind));
        }
        for kind in [
            FieldKind::String,
            FieldKind::Text,
            FieldKind::Boolean,
            FieldKind::Uuid,
        ] {
            assert!(!op_allows_kind(Op::Gt, kind));
            assert!(!op_allows_kind(Op::Lt, kind));
        }
    }

    #[test]
    fn op_allows_kind_string_ops_only_on_string_kinds() {
        for kind in [FieldKind::String, FieldKind::Text] {
            assert!(op_allows_kind(Op::Contains, kind));
            assert!(op_allows_kind(Op::StartsWith, kind));
            assert!(op_allows_kind(Op::EndsWith, kind));
            assert!(op_allows_kind(Op::ContainsI, kind));
        }
        for kind in [
            FieldKind::Integer,
            FieldKind::Float,
            FieldKind::Boolean,
            FieldKind::Datetime,
            FieldKind::Uuid,
        ] {
            assert!(!op_allows_kind(Op::Contains, kind));
            assert!(!op_allows_kind(Op::ContainsI, kind));
        }
    }

    #[test]
    fn op_allows_kind_set_ops_on_every_kind() {
        for kind in [
            FieldKind::String,
            FieldKind::Text,
            FieldKind::Integer,
            FieldKind::Float,
            FieldKind::Boolean,
            FieldKind::Datetime,
            FieldKind::Uuid,
        ] {
            assert!(op_allows_kind(Op::In, kind));
            assert!(op_allows_kind(Op::NotIn, kind));
        }
    }

    #[test]
    fn op_allows_kind_phase_2_1_ops_unchanged() {
        for kind in [
            FieldKind::String,
            FieldKind::Text,
            FieldKind::Integer,
            FieldKind::Float,
            FieldKind::Boolean,
            FieldKind::Datetime,
            FieldKind::Uuid,
        ] {
            assert!(op_allows_kind(Op::Eq, kind));
            assert!(op_allows_kind(Op::Ne, kind));
            assert!(op_allows_kind(Op::IsNull, kind));
        }
    }
}
```

- [ ] **Step 2: Run unit tests**

Run: `cargo test -p rustapi-sql --lib filter`
Expected: PASS — 6 tests (2 prior + 4 new matrix tests).

- [ ] **Step 3: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS, no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/sql/src/filter.rs
git commit -m "feat(sql): extend Op with 11 phase-2.2 variants plus FilterValue::List and op_allows_kind"
```

---

### Task 2: Re-export `op_allows_kind`

**Files:**
- Modify: `crates/sql/src/lib.rs`

- [ ] **Step 1: Replace `crates/sql/src/lib.rs`** with:

```rust
#![forbid(unsafe_code)]

pub mod ddl;
pub mod dml;
pub mod filter;
pub mod ident;
pub mod sort;

pub use ddl::{add_column, create_table, drop_column, drop_table, DdlError};
pub use dml::{
    count, delete, insert, render_where, select_by_id, select_list, update,
    DmlError, SqlAndBinds,
};
pub use filter::{op_allows_kind, Condition, Filter, FilterValue, Op};
pub use ident::{quote_ident, table_name, IdentError};
pub use sort::{Sort, SortDir};
```

- [ ] **Step 2: Build**

Run: `cargo build --workspace`
Expected: PASS.

- [ ] **Step 3: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/sql/src/lib.rs
git commit -m "feat(sql): re-export op_allows_kind"
```

---

### Task 3: Order op emission

**Files:**
- Modify: `crates/sql/src/dml.rs`
- Test: append to existing `mod where_tests` block

- [ ] **Step 1: Add `emit_order` helper and route order ops through it**

Open `crates/sql/src/dml.rs`. Locate the existing `render_where` function. Inside the per-condition `match (&c.op, &c.value)` block, ADD these arms BEFORE the existing `(Op::Eq | Op::Ne, FilterValue::Null(_)) => Err(...)` arm:

```rust
            (Op::Gt | Op::Gte | Op::Lt | Op::Lte, FilterValue::Bound(BoundValue::Null(_))) => {
                return Err(DmlError::InvalidFilter("order op cannot compare against NULL"));
            }
            (Op::Gt | Op::Gte | Op::Lt | Op::Lte, FilterValue::Bound(v)) => {
                let cast = pg_cast(c.kind);
                binds.push(v.clone());
                let p = placeholder;
                placeholder += 1;
                let sym = order_symbol(c.op);
                format!("{col} {sym} ${p}::{cast}")
            }
            (Op::Gt | Op::Gte | Op::Lt | Op::Lte, _) => {
                return Err(DmlError::InvalidFilter("order op requires Bound value"));
            }
```

Add the helper `order_symbol` below the `kind_of` function was (or at the end of the file, before `mod tests`):

```rust
fn order_symbol(op: Op) -> &'static str {
    match op {
        Op::Gt => ">",
        Op::Gte => ">=",
        Op::Lt => "<",
        Op::Lte => "<=",
        _ => "?", // unreachable — caller filters by op group
    }
}
```

- [ ] **Step 2: Append tests** to the existing `mod where_tests` block:

```rust
    #[test]
    fn gt_integer() {
        let f = Filter::All(vec![Condition::new(
            "views",
            FieldKind::Integer,
            Op::Gt,
            FilterValue::Bound(BoundValue::I64(5)),
        )]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"views\" > $1::int8");
        assert_eq!(binds, vec![BoundValue::I64(5)]);
    }

    #[test]
    fn gte_float() {
        let f = Filter::All(vec![Condition::new(
            "score",
            FieldKind::Float,
            Op::Gte,
            FilterValue::Bound(BoundValue::F64(0.5)),
        )]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"score\" >= $1::float8");
        assert_eq!(binds, vec![BoundValue::F64(0.5)]);
    }

    #[test]
    fn lt_datetime() {
        use chrono::{DateTime, Utc};
        let t: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let f = Filter::All(vec![Condition::new(
            "created_at",
            FieldKind::Datetime,
            Op::Lt,
            FilterValue::Bound(BoundValue::DateTime(t)),
        )]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"created_at\" < $1::timestamptz");
    }

    #[test]
    fn lte_integer() {
        let f = Filter::All(vec![Condition::new(
            "views",
            FieldKind::Integer,
            Op::Lte,
            FilterValue::Bound(BoundValue::I64(100)),
        )]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"views\" <= $1::int8");
    }

    #[test]
    fn order_op_rejects_typed_null() {
        let f = Filter::All(vec![Condition::new(
            "views",
            FieldKind::Integer,
            Op::Gt,
            FilterValue::Bound(BoundValue::Null(FieldKind::Integer)),
        )]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn order_op_rejects_filter_value_null() {
        let f = Filter::All(vec![Condition::new(
            "views",
            FieldKind::Integer,
            Op::Gt,
            FilterValue::Null(true),
        )]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rustapi-sql --lib where_tests`
Expected: PASS — prior 14 + new 6 = 20 tests.

- [ ] **Step 4: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/dml.rs
git commit -m "feat(sql): render_where emits \$gt/\$gte/\$lt/\$lte"
```

---

### Task 4: Set op emission

**Files:**
- Modify: `crates/sql/src/dml.rs`
- Test: append to existing `mod where_tests` block

- [ ] **Step 1: Add set-op arms inside `render_where`**

In `crates/sql/src/dml.rs`, inside the per-condition match block, add BEFORE the catch-all `Err` arms:

```rust
            (Op::In | Op::NotIn, FilterValue::List(vs)) if vs.is_empty() => {
                return Err(DmlError::InvalidFilter("set op requires non-empty List"));
            }
            (Op::In | Op::NotIn, FilterValue::List(vs)) => {
                let cast = pg_cast(c.kind);
                let mut placeholders = Vec::with_capacity(vs.len());
                for v in vs {
                    binds.push(v.clone());
                    let p = placeholder;
                    placeholder += 1;
                    placeholders.push(format!("${p}::{cast}"));
                }
                let list = placeholders.join(", ");
                let op_str = if matches!(c.op, Op::In) { "IN" } else { "NOT IN" };
                format!("{col} {op_str} ({list})")
            }
            (Op::In | Op::NotIn, _) => {
                return Err(DmlError::InvalidFilter("set op requires List value"));
            }
```

- [ ] **Step 2: Append tests** to `mod where_tests`:

```rust
    #[test]
    fn in_list_emits_parens() {
        let f = Filter::All(vec![Condition::new(
            "views",
            FieldKind::Integer,
            Op::In,
            FilterValue::List(vec![BoundValue::I64(1), BoundValue::I64(2), BoundValue::I64(3)]),
        )]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE \"views\" IN ($1::int8, $2::int8, $3::int8)"
        );
        assert_eq!(
            binds,
            vec![BoundValue::I64(1), BoundValue::I64(2), BoundValue::I64(3)]
        );
    }

    #[test]
    fn not_in_string() {
        let f = Filter::All(vec![Condition::new(
            "category",
            FieldKind::String,
            Op::NotIn,
            FilterValue::List(vec![
                BoundValue::Str("a".into()),
                BoundValue::Str("b".into()),
            ]),
        )]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE \"category\" NOT IN ($1::text, $2::text)"
        );
    }

    #[test]
    fn empty_in_list_rejected() {
        let f = Filter::All(vec![Condition::new(
            "views",
            FieldKind::Integer,
            Op::In,
            FilterValue::List(vec![]),
        )]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn in_with_non_list_rejected() {
        let f = Filter::All(vec![Condition::new(
            "views",
            FieldKind::Integer,
            Op::In,
            FilterValue::Bound(BoundValue::I64(1)),
        )]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn in_placeholders_continue_after_other_binds() {
        // Eq then In: $1 from Eq, then $2/$3 from In.
        let f = Filter::All(vec![
            Condition::new(
                "title",
                FieldKind::String,
                Op::Eq,
                FilterValue::Bound(BoundValue::Str("x".into())),
            ),
            Condition::new(
                "views",
                FieldKind::Integer,
                Op::In,
                FilterValue::List(vec![BoundValue::I64(1), BoundValue::I64(2)]),
            ),
        ]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE \"title\" = $1::text AND \"views\" IN ($2::int8, $3::int8)"
        );
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rustapi-sql --lib where_tests`
Expected: PASS — prior 20 + new 5 = 25 tests.

- [ ] **Step 4: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/dml.rs
git commit -m "feat(sql): render_where emits \$in / \$nin"
```

---

### Task 5: String op emission

**Files:**
- Modify: `crates/sql/src/dml.rs`
- Test: append to existing `mod where_tests` block

- [ ] **Step 1: Add string-op arms inside `render_where`**

In `crates/sql/src/dml.rs`, inside the per-condition match block, add BEFORE the catch-all `Err` arms (after the set-op arms from Task 4):

```rust
            (Op::Contains | Op::StartsWith | Op::EndsWith, FilterValue::Bound(BoundValue::Str(s))) => {
                binds.push(BoundValue::Str(s.clone()));
                let p = placeholder;
                placeholder += 1;
                format!("{col} LIKE ${p}::text ESCAPE '\\'")
            }
            (Op::ContainsI, FilterValue::Bound(BoundValue::Str(s))) => {
                binds.push(BoundValue::Str(s.clone()));
                let p = placeholder;
                placeholder += 1;
                format!("{col} ILIKE ${p}::text ESCAPE '\\'")
            }
            (Op::Contains | Op::StartsWith | Op::EndsWith | Op::ContainsI, _) => {
                return Err(DmlError::InvalidFilter("string op requires Bound(Str)"));
            }
```

- [ ] **Step 2: Append tests** to `mod where_tests`:

```rust
    #[test]
    fn contains_uses_like_escape() {
        let f = Filter::All(vec![Condition::new(
            "title",
            FieldKind::String,
            Op::Contains,
            FilterValue::Bound(BoundValue::Str("%foo%".into())),
        )]);
        let (sql, binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"title\" LIKE $1::text ESCAPE '\\'");
        assert_eq!(binds, vec![BoundValue::Str("%foo%".into())]);
    }

    #[test]
    fn containsi_uses_ilike() {
        let f = Filter::All(vec![Condition::new(
            "title",
            FieldKind::String,
            Op::ContainsI,
            FilterValue::Bound(BoundValue::Str("%foo%".into())),
        )]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"title\" ILIKE $1::text ESCAPE '\\'");
    }

    #[test]
    fn starts_with_emits_like() {
        let f = Filter::All(vec![Condition::new(
            "slug",
            FieldKind::Text,
            Op::StartsWith,
            FilterValue::Bound(BoundValue::Str("blog-%".into())),
        )]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"slug\" LIKE $1::text ESCAPE '\\'");
    }

    #[test]
    fn ends_with_emits_like() {
        let f = Filter::All(vec![Condition::new(
            "slug",
            FieldKind::Text,
            Op::EndsWith,
            FilterValue::Bound(BoundValue::Str("%-2026".into())),
        )]);
        let (sql, _binds) = render_where(&f, 1).unwrap();
        assert_eq!(sql, " WHERE \"slug\" LIKE $1::text ESCAPE '\\'");
    }

    #[test]
    fn string_op_rejects_non_string_bound() {
        let f = Filter::All(vec![Condition::new(
            "views",
            FieldKind::Integer,
            Op::Contains,
            FilterValue::Bound(BoundValue::I64(7)),
        )]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }

    #[test]
    fn string_op_rejects_null_filter_value() {
        let f = Filter::All(vec![Condition::new(
            "title",
            FieldKind::String,
            Op::Contains,
            FilterValue::Null(true),
        )]);
        assert!(matches!(
            render_where(&f, 1),
            Err(DmlError::InvalidFilter(_))
        ));
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rustapi-sql --lib where_tests`
Expected: PASS — prior 25 + new 6 = 31 tests.

- [ ] **Step 4: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/dml.rs
git commit -m "feat(sql): render_where emits \$contains/\$startsWith/\$endsWith/\$containsi"
```

---

### Task 6: Parser — op mapping, op-kind check, escape/wrap helpers, non-set ops

**Files:**
- Modify: `crates/http/src/filter.rs`

- [ ] **Step 1: Replace the regex and op mapping**

In `crates/http/src/filter.rs`, find the `parse_key` function:

```rust
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
```

Replace with:

```rust
fn parse_key(k: &str) -> Result<(String, String, Option<usize>), Error> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"^filters\[(?P<col>[^\[\]]+)\]\[(?P<op>\$[a-zA-Z]+)\](?:\[(?P<idx>\d+)\])?$",
        )
        .unwrap()
    });
    let caps = re.captures(k).ok_or_else(|| {
        Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}`"
        )))
    })?;
    let idx = caps
        .name("idx")
        .map(|m| m.as_str().parse::<usize>().expect("regex \\d+ already validated"));
    Ok((caps["col"].to_string(), caps["op"].to_string(), idx))
}
```

- [ ] **Step 2: Add helpers `escape_like` and `wrap_like`** to the end of `crates/http/src/filter.rs`, before the `#[cfg(test)]` block:

```rust
/// Escape LIKE metacharacters in user input. Order matters: backslash first
/// so we don't double-escape our own substitutions.
fn escape_like(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn wrap_like(op: Op, escaped: String) -> String {
    match op {
        Op::Contains | Op::ContainsI => format!("%{escaped}%"),
        Op::StartsWith => format!("{escaped}%"),
        Op::EndsWith => format!("%{escaped}"),
        _ => escaped, // unreachable; caller filters by op group
    }
}
```

- [ ] **Step 3: Add tests for the helpers** to the existing `mod tests` block in `crates/http/src/filter.rs`:

```rust
    #[test]
    fn escape_like_handles_metacharacters() {
        assert_eq!(escape_like("foo"), "foo");
        assert_eq!(escape_like("50%"), "50\\%");
        assert_eq!(escape_like("a_b"), "a\\_b");
        assert_eq!(escape_like("a\\b"), "a\\\\b");
        // Backslash-first ordering: input \% becomes \\\% not \\\\%.
        assert_eq!(escape_like("\\%"), "\\\\\\%");
    }

    #[test]
    fn wrap_like_per_op() {
        assert_eq!(wrap_like(Op::Contains, "foo".into()), "%foo%");
        assert_eq!(wrap_like(Op::ContainsI, "foo".into()), "%foo%");
        assert_eq!(wrap_like(Op::StartsWith, "foo".into()), "foo%");
        assert_eq!(wrap_like(Op::EndsWith, "foo".into()), "%foo");
    }
```

- [ ] **Step 4: Run helper tests**

Run: `cargo test -p rustapi-http --lib filter::tests::escape_like_handles_metacharacters filter::tests::wrap_like_per_op`
Expected: PASS — 2 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/filter.rs
git commit -m "feat(http): filter regex allows [idx] suffix; add escape_like/wrap_like helpers"
```

---

### Task 7: Parser — wire new operators end-to-end (including set ops and op-kind check)

**Files:**
- Modify: `crates/http/src/filter.rs`

- [ ] **Step 1: Replace the `parse` function** in `crates/http/src/filter.rs`. Find the existing `parse` (it currently uses `(col, op_str) = parse_key(&k)?` and a small `match op_str.as_str()` block) and replace the whole function body with:

```rust
pub fn parse(raw_query: &str, ct: &ContentType) -> Result<Filter, Error> {
    // Phase 2.1 non-set conds are stored directly into `conds`.
    // Phase 2.2 set-op entries are buffered in `set_buckets` keyed by
    // (col, op) with index-ordered values, then materialized into a single
    // `Condition::List` at the end.
    use std::collections::BTreeMap;
    let mut seen: HashSet<(String, Op)> = HashSet::new();
    let mut conds: Vec<Condition> = Vec::new();
    let mut set_buckets: std::collections::HashMap<(String, Op), BTreeMap<usize, BoundValue>> =
        std::collections::HashMap::new();
    let mut set_kinds: std::collections::HashMap<(String, Op), FieldKind> =
        std::collections::HashMap::new();

    for (k, v) in form_urlencoded::parse(raw_query.as_bytes()) {
        if !k.starts_with("filters[") {
            continue;
        }
        let (col, op_str, idx) = parse_key(&k)?;
        let op = map_op(&op_str, &col)?;
        let field = field_for(ct, &col)?;
        let kind = field.kind();

        if !rustapi_sql::op_allows_kind(op, kind) {
            return Err(field_err(
                &col,
                format!("operator `{op_str}` invalid for kind `{kind:?}`"),
            ));
        }

        let is_set_op = matches!(op, Op::In | Op::NotIn);
        match (is_set_op, idx) {
            (true, None) => {
                return Err(field_err(&col, "set operator requires bracketed list indices"));
            }
            (false, Some(_)) => {
                return Err(field_err(&col, "unexpected list index for operator"));
            }
            (true, Some(i)) => {
                if v.eq_ignore_ascii_case("null") {
                    return Err(field_err(&col, "set operator entries cannot be null"));
                }
                let bv = coerce_bound(kind, &col, &v)?;
                let bucket = set_buckets.entry((col.clone(), op)).or_default();
                if bucket.insert(i, bv).is_some() {
                    return Err(field_err(&col, "duplicate set operator entry"));
                }
                set_kinds.insert((col.clone(), op), kind);
                if bucket.len() > 100 {
                    return Err(field_err(&col, "set operator limited to 100 items"));
                }
            }
            (false, None) => {
                if !seen.insert((col.clone(), op)) {
                    return Err(field_err(&col, "duplicate filter operator on column"));
                }
                let value = coerce_value(field, op, &col, &v)?;
                conds.push(Condition::new(col, kind, op, value));
            }
        }
    }

    // Materialize set buckets into Conditions.
    for ((col, op), bucket) in set_buckets {
        if bucket.is_empty() {
            return Err(field_err(&col, "set operator requires non-empty list"));
        }
        let kind = set_kinds[&(col.clone(), op)];
        let values: Vec<BoundValue> = bucket.into_values().collect();
        conds.push(Condition::new(col, kind, op, FilterValue::List(values)));
    }

    if conds.is_empty() {
        Ok(Filter::None)
    } else {
        Ok(Filter::All(conds))
    }
}

fn map_op(op_str: &str, col: &str) -> Result<Op, Error> {
    Ok(match op_str {
        "$eq" => Op::Eq,
        "$ne" => Op::Ne,
        "$null" => Op::IsNull,
        "$gt" => Op::Gt,
        "$gte" => Op::Gte,
        "$lt" => Op::Lt,
        "$lte" => Op::Lte,
        "$in" => Op::In,
        "$nin" => Op::NotIn,
        "$contains" => Op::Contains,
        "$startsWith" => Op::StartsWith,
        "$endsWith" => Op::EndsWith,
        "$containsi" => Op::ContainsI,
        other => return Err(field_err(col, format!("unknown operator `{other}`"))),
    })
}
```

- [ ] **Step 2: Extend `coerce_value`** in the same file to handle string-op wrap/escape. Find the existing `coerce_value` function and replace with:

```rust
fn coerce_value(field: FieldOrSystem<'_>, op: Op, col: &str, raw: &str) -> Result<FilterValue, Error> {
    let kind = field.kind();
    match op {
        Op::IsNull => parse_bool(raw)
            .map(FilterValue::Null)
            .map_err(|reason| field_err(col, reason)),
        Op::Eq | Op::Ne => {
            if raw.eq_ignore_ascii_case("null") {
                return Ok(FilterValue::Bound(BoundValue::Null(kind)));
            }
            coerce_bound(kind, col, raw).map(FilterValue::Bound)
        }
        Op::Gt | Op::Gte | Op::Lt | Op::Lte => {
            if raw.eq_ignore_ascii_case("null") {
                return Err(field_err(col, "order operator cannot compare against null"));
            }
            coerce_bound(kind, col, raw).map(FilterValue::Bound)
        }
        Op::Contains | Op::StartsWith | Op::EndsWith | Op::ContainsI => {
            let escaped = escape_like(raw);
            let wrapped = wrap_like(op, escaped);
            Ok(FilterValue::Bound(BoundValue::Str(wrapped)))
        }
        // Set ops are handled in `parse`, not here.
        Op::In | Op::NotIn => {
            Err(field_err(col, "internal: set op routed through coerce_value"))
        }
        // Unreachable today: every Op variant above is handled. The wildcard
        // exists because `Op` is `#[non_exhaustive]` so a future variant
        // compiles silently until both `map_op` and this match get updated.
        _ => Err(field_err(col, "unsupported operator")),
    }
}
```

- [ ] **Step 3: Add new parser tests** to `mod tests` in `crates/http/src/filter.rs`:

```rust
    #[test]
    fn gt_on_string_rejected() {
        let err = parse("filters[title][$gt]=hi", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn contains_on_integer_rejected() {
        let err = parse("filters[views][$contains]=7", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn gt_integer_parses() {
        let f = parse("filters[views][$gt]=10", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert_eq!(conds[0].op, Op::Gt);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::I64(n)) => assert_eq!(*n, 10),
            other => panic!("expected I64, got {other:?}"),
        }
    }

    #[test]
    fn in_two_values_collects_into_list() {
        let f = parse("filters[views][$in][0]=1&filters[views][$in][1]=2", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert_eq!(conds.len(), 1);
        assert_eq!(conds[0].op, Op::In);
        match &conds[0].value {
            FilterValue::List(vs) => {
                assert_eq!(vs.len(), 2);
                assert!(matches!(vs[0], BoundValue::I64(1)));
                assert!(matches!(vs[1], BoundValue::I64(2)));
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn in_missing_index_rejected() {
        let err = parse("filters[views][$in]=1", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn non_set_op_with_index_rejected() {
        let err = parse("filters[views][$eq][0]=1", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_duplicate_index_rejected() {
        let err = parse(
            "filters[views][$in][0]=1&filters[views][$in][0]=2",
            &ct(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_null_entry_rejected() {
        let err = parse("filters[views][$in][0]=null", &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn in_over_cap_rejected() {
        let mut q = String::new();
        for i in 0..=100 {
            if !q.is_empty() {
                q.push('&');
            }
            q.push_str(&format!("filters[views][$in][{i}]={i}"));
        }
        let err = parse(&q, &ct()).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn contains_escapes_and_wraps() {
        let f = parse("filters[title][$contains]=50%25", &ct()).unwrap();
        // `%25` URL-decodes to `%`, which then escapes to `\%`, then wraps to `%50\%%`.
        let Filter::All(conds) = f else { panic!() };
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "%50\\%%"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn starts_with_wraps_one_side() {
        let f = parse("filters[title][$startsWith]=foo", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "foo%"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn ends_with_wraps_one_side() {
        let f = parse("filters[title][$endsWith]=foo", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "%foo"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn containsi_op_variant() {
        let f = parse("filters[title][$containsi]=FOO", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert_eq!(conds[0].op, Op::ContainsI);
        match &conds[0].value {
            FilterValue::Bound(BoundValue::Str(s)) => assert_eq!(s, "%FOO%"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn gte_on_datetime_rfc3339() {
        let f = parse("filters[created_at][$gte]=2026-01-01T00:00:00Z", &ct()).unwrap();
        let Filter::All(conds) = f else { panic!() };
        assert_eq!(conds[0].op, Op::Gte);
        assert!(matches!(conds[0].value, FilterValue::Bound(BoundValue::DateTime(_))));
    }
```

- [ ] **Step 4: Run all filter tests**

Run: `cargo test -p rustapi-http --lib filter`
Expected: PASS — prior 17 (15 from 2.1 + 2 helper tests in Task 6) + new 13 = 30 tests.

- [ ] **Step 5: Clippy clean**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/filter.rs
git commit -m "feat(http): parser handles order/set/string ops with op-kind check"
```

---

### Task 8: Integration tests + workspace verification

**Files:**
- Create: `crates/bin/tests/integration_filters_2_2.rs`

- [ ] **Step 1: Write `crates/bin/tests/integration_filters_2_2.rs`**

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
        json!({"title": "foo",     "views": 0,    "published": true,  "category": "tech"}),
        json!({"title": "barfoo",  "views": 5,    "published": false, "category": "tech"}),
        json!({"title": "foobar",  "views": 10,   "published": true,  "category": "design"}),
        json!({"title": "null-vw", "views": null, "published": true,  "category": "design"}),
        json!({"title": "xyz",     "views": 20,   "published": false, "category": null}),
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
async fn gt_excludes_nulls() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$gt]=5").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn gte_inclusive() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$gte]=5").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn lt_basic() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$lt]=10").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn lte_inclusive() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[views][$lte]=10").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn gt_on_string_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[title][$gt]=hi")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn in_two_categories() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[category][$in][0]=tech&filters[category][$in][1]=design",
    )
    .await;
    assert_eq!(body["meta"]["total"], 4);
}

#[tokio::test]
async fn nin_excludes() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[views][$nin][0]=0&filters[views][$nin][1]=20",
    )
    .await;
    // PG `NOT IN` excludes NULLs.
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn in_single_value_behaves_like_eq() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[category][$in][0]=tech").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn in_missing_index_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[views][$in]=1")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn in_duplicate_index_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(
            app.url("/api/post?filters[views][$in][0]=1&filters[views][$in][0]=2"),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn in_over_cap_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let mut q = String::new();
    for i in 0..=100 {
        if !q.is_empty() {
            q.push('&');
        }
        q.push_str(&format!("filters[views][$in][{i}]={i}"));
    }
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post?{q}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn contains_basic() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$contains]=foo").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn containsi_case_insensitive() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$containsi]=FOO").await;
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn starts_with_basic() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$startsWith]=foo").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn ends_with_basic() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(&app, "filters[title][$endsWith]=foo").await;
    assert_eq!(body["meta"]["total"], 2);
}

#[tokio::test]
async fn contains_literal_percent() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    // Insert a literal "50% off" title.
    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "50% off", "category": "deal"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    // `%25` URL-decodes to `%`; server escapes to `\%`; wraps to `%50\%%`.
    let body = list_body(&app, "filters[title][$contains]=50%25").await;
    assert_eq!(body["meta"]["total"], 1);
    assert_eq!(body["data"][0]["title"], "50% off");
}

#[tokio::test]
async fn contains_on_integer_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[views][$contains]=5")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn compose_multiple_groups() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[title][$contains]=foo&filters[views][$gt]=0&filters[category][$in][0]=tech&filters[category][$in][1]=design",
    )
    .await;
    // foo (views=0, tech) excluded by views>0
    // barfoo (5, tech) ✓
    // foobar (10, design) ✓
    // null-vw (null, design) excluded by views>0
    // xyz (20, null) excluded by category in {tech, design}
    assert_eq!(body["meta"]["total"], 2);
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p rustapi --test integration_filters_2_2`
Expected: PASS — 18 tests.

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace`
Expected: PASS. Should be ~169 tests (133 from end of phase 2.1 + 4 unit in filter.rs + 17 unit in dml.rs + 14 unit in http/filter.rs + 18 new integration ≈ 186; accept whatever pile of green tests `cargo test` reports as long as none fail).

If any prior test fails: do NOT mark complete. Open a follow-up task to fix the regression and re-run.

- [ ] **Step 4: Final clippy sweep**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: PASS, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/bin/tests/integration_filters_2_2.rs
git commit -m "test(bin): integration coverage for phase 2.2 filter operators"
```

---

## Self-Review Notes

- Spec §2.1 wire format examples → Task 7 (`in_two_values_collects_into_list`, `gt_integer_parses`, `containsi_op_variant`).
- Spec §2.2 regex (mixed-case ops + optional idx) → Task 6.
- Spec §2.3 set value handling (index sorting, dup detection, 100-cap, null rejection) → Task 7 parser + integration tests.
- Spec §2.4 LIKE escape + wrap rules → Task 6 helpers + Task 7 `contains_escapes_and_wraps` test + integration `contains_literal_percent`.
- Spec §2.5 op-kind matrix → Task 1 `op_allows_kind` + Task 7 parser check + integration rejection tests.
- Spec §3.1 `Op` variants + `FilterValue::List` → Task 1.
- Spec §3.2 `op_allows_kind` → Task 1 (lives in same file per spec).
- Spec §3.3 emission rules → Tasks 3, 4, 5 covering all 11 ops.
- Spec §3.4 helper extraction: plan uses inline match arms (smaller than three free helpers) since the per-group logic stays short enough. Single-responsibility preserved; revisit if a future slice adds more groups.
- Spec §3.5 unit tests → Tasks 3, 4, 5 (16 new tests in `where_tests`).
- Spec §4.1 parser changes → Tasks 6 + 7.
- Spec §4.2 `escape_like` and `wrap_like` → Task 6.
- Spec §4.3 parser unit tests → Task 7 (13 new tests).
- Spec §5 integration tests → Task 8 (18 tests covering every listed scenario plus a composition test).
- Spec §6 errors — the parser uses the existing `field_err` / `ValidationErrors::field` paths so `details.fields[0]` carries the field + reason. No new error code surface.
- Spec §7 out-of-scope items confirmed untouched.
- Backwards compat: phase 2.1 syntax stays valid because `parse_key` now returns `(col, op, Option<usize>)`; non-set ops continue to take the `idx = None` path and behave identically. Confirmed by leaving all 2.1 parser tests in place — they must remain green at Task 7's verification step.
- `regex` and `url` already added in phase 2.1; no new deps in this slice.
