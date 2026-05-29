# Phase 2.3 `$or` / `$not` Combinators Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add recursive `$or` / `$and` / `$not` combinators to the filter parser and SQL emitter, preserving all phase 2.1 / 2.2 wire formats byte-for-byte.

**Architecture:** `Filter` enum becomes recursive (`None` / `All(Vec<Filter>)` / `Any(Vec<Filter>)` / `Not(Box<Filter>)` / `Leaf(Condition)`). Parser tokenizes bracket segments into a tree with depth-8 and leaf-100 caps. Emitter recurses, wrapping group children in parens and eliding single-child groups so phase 2.1/2.2 SQL stays unchanged. Strict 422 on empty groups, gaps, dups, and non-unary `$not`.

**Tech Stack:** Rust 1.88, axum 0.7, sqlx 0.8 (Postgres), `regex` crate (already in use), `url::form_urlencoded` (already in use), `testcontainers` for integration tests.

**Spec:** `docs/superpowers/specs/2026-05-29-filters-or-not-design.md`

---

## File Structure

**Modify:**
- `crates/sql/src/filter.rs` — enum recursion (`Any`, `Not`, `Leaf`); add tree-construction helpers.
- `crates/sql/src/lib.rs` — re-exports (no surface change beyond `Filter` variants).
- `crates/sql/src/dml.rs` — `render_where` becomes recursive, single-child elision, paren wrapping.
- `crates/http/src/filter.rs` — split key tokenization from leaf-coercion; build tree top-down; depth/leaf-cap enforcement; gap/dup/empty checks; `$not` unary check.

**Create:**
- `crates/bin/tests/integration_filters_2_3.rs` — Postgres integration coverage.

**No changes:**
- `crates/core` — `BoundValue`, `FieldKind`, `Condition` shape unchanged.
- `crates/schema` — unaffected.
- `crates/bin/src/*` — handler signatures unchanged.

The migration is deliberately staged so each task compiles and tests pass before moving on:
1. Add new `Filter` variants and `Leaf` wrapper while keeping `All(Vec<Filter>)` semantics intact (Task 1).
2. Migrate emitter to recursive form (Task 2).
3. Migrate parser to wrap conditions in `Filter::Leaf` (Task 3) — no behavior change.
4. Extend parser with combinator tokens and tree walking (Tasks 4–7).
5. Integration tests (Task 8).

---

## Task 1: Filter Enum Recursion

**Files:**
- Modify: `crates/sql/src/filter.rs`

The current `Filter::All` takes `Vec<Condition>`. After this task it takes `Vec<Filter>`, with `Filter::Leaf(Condition)` as the new leaf node. Two new variants land: `Filter::Any(Vec<Filter>)` and `Filter::Not(Box<Filter>)`.

- [ ] **Step 1: Write the failing tests**

Open `crates/sql/src/filter.rs`. Replace the existing `#[cfg(test)] mod tests` block with this (keep the imports at the top of that block unchanged):

```rust
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

    // Phase 2.3 — recursive variants.

    #[test]
    fn leaf_variant_holds_condition() {
        let c = Condition::new("title", FieldKind::String, Op::Eq, FilterValue::Null(true));
        let f = Filter::Leaf(c.clone());
        let Filter::Leaf(inner) = f else { panic!("expected Leaf") };
        assert_eq!(inner.column, c.column);
    }

    #[test]
    fn any_variant_holds_vec() {
        let f = Filter::Any(vec![
            Filter::Leaf(Condition::new("a", FieldKind::Integer, Op::Eq, FilterValue::Null(true))),
            Filter::Leaf(Condition::new("b", FieldKind::Integer, Op::Eq, FilterValue::Null(true))),
        ]);
        let Filter::Any(xs) = f else { panic!("expected Any") };
        assert_eq!(xs.len(), 2);
    }

    #[test]
    fn not_variant_holds_box() {
        let f = Filter::Not(Box::new(Filter::Leaf(Condition::new(
            "a",
            FieldKind::Integer,
            Op::Eq,
            FilterValue::Null(true),
        ))));
        let Filter::Not(inner) = f else { panic!("expected Not") };
        assert!(matches!(*inner, Filter::Leaf(_)));
    }

    #[test]
    fn all_variant_holds_vec_of_filter() {
        let f = Filter::All(vec![
            Filter::Leaf(Condition::new("a", FieldKind::Integer, Op::Eq, FilterValue::Null(true))),
            Filter::Any(vec![
                Filter::Leaf(Condition::new("b", FieldKind::Integer, Op::Eq, FilterValue::Null(true))),
            ]),
        ]);
        let Filter::All(xs) = f else { panic!("expected All") };
        assert_eq!(xs.len(), 2);
        assert!(matches!(xs[0], Filter::Leaf(_)));
        assert!(matches!(xs[1], Filter::Any(_)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustapi-sql --lib filter::tests`
Expected: COMPILE ERROR — `Filter::Leaf`, `Filter::Any`, `Filter::Not` undefined; existing `Filter::All(Vec<Condition>)` does not accept `Vec<Filter>`.

- [ ] **Step 3: Update the enum**

Replace the `Filter` enum definition (currently lines 7–14 of `crates/sql/src/filter.rs`) with:

```rust
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Filter {
    #[default]
    None,
    /// Implicit AND across children. Empty vec is treated as `None` by the
    /// emitter. Single-child vecs are elided (no redundant parens).
    All(Vec<Filter>),
    /// Logical OR across children. Empty vec is rejected by the parser; the
    /// emitter has a defensive guard.
    Any(Vec<Filter>),
    /// Logical NOT. Unary by construction (parser enforces).
    Not(Box<Filter>),
    /// Terminal leaf — a single column condition.
    Leaf(Condition),
}
```

Leave the doc comment block above the enum unchanged in spirit but update it: change the "Phase 2.1 shipped" comment at the top of the file (lines 1–3) to:

```rust
//! Filter expressions. Phase 2.1 shipped `$eq` / `$ne` / `$null` combined with
//! implicit AND. Phase 2.2 added order / set / string operators. Phase 2.3 adds
//! recursive combinators (`$or`, `$and`, `$not`) — `Filter` is now a tree.
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustapi-sql --lib filter::tests`
Expected: PASS — `default_is_none`, `condition_new_builds_struct`, the four `op_allows_kind_*` tests, plus the four new variant smoke tests.

This will break the SQL emitter (`crates/sql/src/dml.rs`) and HTTP parser (`crates/http/src/filter.rs`) at compile time. That is expected — Tasks 2 and 3 fix them.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/filter.rs
git commit -m "$(cat <<'EOF'
feat(sql): make Filter recursive with Any/Not/Leaf

Filter::All now holds Vec<Filter> instead of Vec<Condition>. Adds
Filter::Leaf(Condition) as the terminal node, Filter::Any(Vec<Filter>)
for $or, and Filter::Not(Box<Filter>) for $not. Emitter and parser
broken at compile time until follow-up tasks migrate them.
EOF
)"
```

---

## Task 2: Recursive Emitter

**Files:**
- Modify: `crates/sql/src/dml.rs`

`render_where` becomes a thin wrapper that delegates to a recursive helper. Group children get paren-wrapped; single-child `All`/`Any` skip the wrap so phase 2.1/2.2 output stays identical.

- [ ] **Step 1: Write the failing tests**

Open `crates/sql/src/dml.rs`. Inside the existing `mod where_tests` block (search for `mod where_tests {`), update the existing `select_list_empty_all_keeps_v1_placeholders` test in the `mod tests` block above it — it currently builds `Filter::All(vec![])` with the old `Vec<Condition>` signature. The new `Filter::All(vec![])` builds the same way (an empty `Vec<Filter>`), so the test body needs no changes; only confirm it still type-checks after Task 1.

Then update `select_list_with_filter_shifts_pagination` and `count_with_filter` in the same `mod tests` block: each currently calls `Filter::All(vec![Condition::new(...)])`. Wrap each `Condition::new(...)` in `Filter::Leaf(...)`. Example for `select_list_with_filter_shifts_pagination`:

```rust
let f = Filter::All(vec![Filter::Leaf(Condition::new(
    "title",
    FieldKind::String,
    Op::Eq,
    FilterValue::Bound(BoundValue::Str("hi".into())),
))]);
```

Apply the same `Filter::Leaf(...)` wrap to every test in `where_tests` that uses `Filter::All(vec![Condition::new(...)...])`. There are roughly 20 such tests; the change is mechanical (wrap each `Condition::new(...)` in `Filter::Leaf(...)`). Do NOT change assertion strings — single-child elision is what keeps them identical.

After mechanical wrapping, append the following NEW tests to the end of `mod where_tests`:

```rust
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
        vec![BoundValue::I64(10), BoundValue::I64(20), BoundValue::I64(30)]
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustapi-sql --lib`
Expected: COMPILE ERROR — `render_where` is still written against the flat `Vec<Condition>` shape from phase 2.2. The pattern `Filter::All(c)` now binds a `Vec<Filter>`, not a `Vec<Condition>`, so the inner loop fails to compile.

- [ ] **Step 3: Rewrite `render_where`**

Replace the entire body of `render_where` (currently lines 307–403 of `crates/sql/src/dml.rs`) with this:

```rust
pub fn render_where(filter: &Filter, start_placeholder: usize) -> Result<(String, Vec<BoundValue>), DmlError> {
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

fn render_node(
    node: &Filter,
    buf: &mut String,
    binds: &mut Vec<BoundValue>,
    placeholder: &mut usize,
) -> Result<(), DmlError> {
    match node {
        Filter::None => Err(DmlError::InvalidFilter("Filter::None inside group")),
        Filter::Leaf(c) => render_leaf(c, buf, binds, placeholder),
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
            return Err(DmlError::InvalidFilter("order op cannot compare against NULL"));
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
            let op_str = if matches!(c.op, Op::In) { "IN" } else { "NOT IN" };
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustapi-sql`
Expected: PASS — all where-tests including the new ones; back-compat tests in `mod tests` (after the `Filter::Leaf(...)` mechanical wrap) still produce identical SQL strings thanks to single-child elision.

- [ ] **Step 5: Lint and ratchet**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: clean.

Run: `cargo check --workspace`
Expected: errors in `crates/http/src/filter.rs` (parser still uses old `Filter::All(Vec<Condition>)` directly). Tasks 3+ fix this; compile error here is expected.

- [ ] **Step 6: Commit**

```bash
git add crates/sql/src/dml.rs
git commit -m "$(cat <<'EOF'
feat(sql): recursive render_where for All/Any/Not/Leaf

Splits per-leaf rendering into render_leaf and recurses through
groups via render_node. Single-child All/Any groups elide their
parens so existing phase 2.1/2.2 SQL output stays byte-identical.
Empty Any/All-inside-tree reach the defensive InvalidFilter guard.
HTTP parser still broken at compile time — Task 3 wraps Leafs.
EOF
)"
```

---

## Task 3: Parser Leaf Wrap (Behavior-Neutral)

**Files:**
- Modify: `crates/http/src/filter.rs`

This task does NOT introduce combinator parsing. Goal: make `crates/http/src/filter.rs` compile again under the new `Filter::All(Vec<Filter>)` shape by wrapping every produced `Condition` in `Filter::Leaf`. No behavior change; every existing parser test must still pass.

- [ ] **Step 1: Confirm existing tests describe v2.2 behavior**

Read `crates/http/src/filter.rs` `mod tests`. Tests like `single_eq_string`, `integer_coerces`, `in_two_values_collects_into_list` all destructure `Filter::All(conds)`. After this task, `conds` will be `Vec<Filter>` (each `Filter::Leaf(Condition)`), so destructuring needs to change.

Update the destructuring pattern in every test that does `let Filter::All(conds) = f else { panic!() };` so that subsequent `conds[i].column`, `conds[i].op`, `conds[i].value`, `conds[i].op` accesses still work. Add a small helper in `mod tests`:

```rust
fn leaves(f: Filter) -> Vec<Condition> {
    let Filter::All(xs) = f else { panic!("expected All") };
    xs.into_iter()
        .map(|x| match x {
            Filter::Leaf(c) => c,
            other => panic!("expected Leaf, got {other:?}"),
        })
        .collect()
}
```

Then mechanically replace each pattern `let Filter::All(conds) = f else { panic!() };` with `let conds = leaves(f);`. Inside test bodies, `conds[i].column` etc. continue to work since `conds` is now `Vec<Condition>`.

Apply this mechanical refactor to every test in `crates/http/src/filter.rs::mod tests` that destructures `Filter::All`.

- [ ] **Step 2: Run tests to verify they still fail at compile**

Run: `cargo test -p rustapi-http --lib`
Expected: COMPILE ERROR — `parse()` still produces `Filter::All(conds: Vec<Condition>)` directly.

- [ ] **Step 3: Wrap conditions in `Filter::Leaf`**

In `crates/http/src/filter.rs`, find the tail of `parse()` (around lines 79–84):

```rust
    if conds.is_empty() {
        Ok(Filter::None)
    } else {
        Ok(Filter::All(conds))
    }
}
```

Replace with:

```rust
    if conds.is_empty() {
        Ok(Filter::None)
    } else {
        Ok(Filter::All(conds.into_iter().map(Filter::Leaf).collect()))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustapi-http`
Expected: PASS — all v2.1/v2.2 parser tests green under new shape.

Run: `cargo test --workspace`
Expected: PASS — all 188 baseline tests plus the new sql crate tests from Tasks 1 and 2.

- [ ] **Step 5: Lint**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/filter.rs
git commit -m "$(cat <<'EOF'
refactor(http): wrap parser-produced Conditions in Filter::Leaf

Behavior-neutral migration to the recursive Filter shape. Tests
still pass with identical assertions; SQL output unchanged.
EOF
)"
```

---

## Task 4: Parser — Key Tokenization

**Files:**
- Modify: `crates/http/src/filter.rs`

Refactor `parse_key` to return a sequence of segments instead of `(col, op, idx)`. This unblocks combinator parsing in Task 5. Behavior change: keys with combinator tokens at the head will now also tokenize successfully (but `parse()` will reject them until Task 5 wires the tree builder).

- [ ] **Step 1: Write failing unit tests**

Add this module inside `crates/http/src/filter.rs` (place it above the existing `#[cfg(test)]` block, OR add the tests inside the existing one — either works):

```rust
#[cfg(test)]
mod tokenize_tests {
    use super::*;

    #[test]
    fn flat_leaf() {
        let segs = tokenize_key("filters[title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn flat_leaf_with_in_index() {
        let segs = tokenize_key("filters[views][$in][0]").unwrap();
        assert_eq!(segs, vec![
            Segment::Name("views".into()),
            Segment::Op("$in".into()),
            Segment::Index(0),
        ]);
    }

    #[test]
    fn or_group_index_then_leaf() {
        let segs = tokenize_key("filters[$or][0][title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Combinator("$or".into()),
            Segment::Index(0),
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn not_wraps_leaf() {
        let segs = tokenize_key("filters[$not][title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Combinator("$not".into()),
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn nested_or_in_or() {
        let segs = tokenize_key("filters[$or][0][$or][1][title][$eq]").unwrap();
        assert_eq!(segs, vec![
            Segment::Combinator("$or".into()),
            Segment::Index(0),
            Segment::Combinator("$or".into()),
            Segment::Index(1),
            Segment::Name("title".into()),
            Segment::Op("$eq".into()),
        ]);
    }

    #[test]
    fn missing_filters_prefix_rejected() {
        assert!(tokenize_key("title[$eq]").is_err());
    }

    #[test]
    fn unbalanced_brackets_rejected() {
        assert!(tokenize_key("filters[title][$eq").is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustapi-http --lib tokenize_tests`
Expected: COMPILE ERROR — `Segment`, `tokenize_key` undefined.

- [ ] **Step 3: Add `Segment` and `tokenize_key`**

Add this near the top of `crates/http/src/filter.rs`, after the existing imports (above `pub fn parse`):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Segment {
    /// `$or`, `$and`, `$not`
    Combinator(String),
    /// `$eq`, `$ne`, `$null`, `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$nin`,
    /// `$contains`, `$startsWith`, `$endsWith`, `$containsi`
    Op(String),
    /// Group child index (`$or[0]`, `$and[2]`) or set-value index (`$in[3]`).
    Index(usize),
    /// Column name.
    Name(String),
}

/// Split a `filters[...]...` key into ordered `Segment`s. Performs no
/// semantic validation — that's the tree builder's job.
pub(crate) fn tokenize_key(k: &str) -> Result<Vec<Segment>, Error> {
    let rest = k.strip_prefix("filters").ok_or_else(|| {
        Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}` (missing `filters` prefix)"
        )))
    })?;

    let mut segments = Vec::new();
    let mut cur = rest;
    while !cur.is_empty() {
        let inner = cur
            .strip_prefix('[')
            .and_then(|s| {
                let close = s.find(']')?;
                Some((&s[..close], &s[close + 1..]))
            })
            .ok_or_else(|| {
                Error::Validation(ValidationErrors::single(format!(
                    "malformed filter param `{k}` (unbalanced brackets)"
                )))
            })?;
        let (raw, tail) = inner;
        if raw.is_empty() {
            return Err(Error::Validation(ValidationErrors::single(format!(
                "malformed filter param `{k}` (empty bracket)"
            ))));
        }
        let seg = classify_segment(raw);
        segments.push(seg);
        cur = tail;
    }
    if segments.is_empty() {
        return Err(Error::Validation(ValidationErrors::single(format!(
            "malformed filter param `{k}` (no segments)"
        ))));
    }
    Ok(segments)
}

fn classify_segment(raw: &str) -> Segment {
    match raw {
        "$or" | "$and" | "$not" => Segment::Combinator(raw.to_string()),
        s if s.starts_with('$') => Segment::Op(s.to_string()),
        s => match s.parse::<usize>() {
            Ok(n) => Segment::Index(n),
            Err(_) => Segment::Name(s.to_string()),
        },
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustapi-http --lib tokenize_tests`
Expected: PASS — all 7 new tests green.

Run: `cargo test --workspace`
Expected: PASS — `parse()` still uses the old `parse_key` (untouched), so existing tests unaffected.

- [ ] **Step 5: Lint**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: clean. (The new `Segment`/`tokenize_key`/`classify_segment` are crate-private and unused outside tests for now; `pub(crate)` keeps them visible to Task 5 without leaking. If clippy complains about `dead_code` on `classify_segment` due to it being only called internally, that's expected to clear in Task 5; for now add `#[allow(dead_code)]` to `Segment`, `tokenize_key`, and `classify_segment` to keep the workspace ratchet clean.)

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/filter.rs
git commit -m "$(cat <<'EOF'
feat(http): segment-based key tokenizer

Adds Segment enum and tokenize_key for filter URL keys. Splits on
brackets, classifies each segment as Combinator/Op/Index/Name.
Not yet wired into parse() — Task 5 builds the filter tree on top.
EOF
)"
```

---

## Task 5: Parser — Tree Builder Skeleton

**Files:**
- Modify: `crates/http/src/filter.rs`

Replace the flat-loop body of `parse()` with a tree-builder that handles top-level leaves AND combinator groups. This task introduces the tree but leaves caps (depth/leaf-count) for Task 6.

- [ ] **Step 1: Write failing tests**

Append these to the existing `mod tests` block in `crates/http/src/filter.rs`:

```rust
#[test]
fn or_two_leaves() {
    let f = parse(
        "filters[$or][0][title][$eq]=foo&filters[$or][1][title][$eq]=bar",
        &ct(),
    )
    .unwrap();
    // Top-level All wraps the single $or group.
    let Filter::All(xs) = f else { panic!("expected All, got {f:?}") };
    assert_eq!(xs.len(), 1);
    let Filter::Any(ys) = &xs[0] else { panic!("expected Any, got {:?}", xs[0]) };
    assert_eq!(ys.len(), 2);
    for child in ys {
        assert!(matches!(child, Filter::Leaf(_)));
    }
}

#[test]
fn and_two_leaves() {
    let f = parse(
        "filters[$and][0][title][$eq]=foo&filters[$and][1][views][$gt]=1",
        &ct(),
    )
    .unwrap();
    let Filter::All(xs) = f else { panic!() };
    assert_eq!(xs.len(), 1);
    // Explicit $and is also an All, but not flattened.
    let Filter::All(inner) = &xs[0] else { panic!() };
    assert_eq!(inner.len(), 2);
}

#[test]
fn not_unary_leaf() {
    let f = parse("filters[$not][title][$eq]=foo", &ct()).unwrap();
    let Filter::All(xs) = f else { panic!() };
    let Filter::Not(inner) = &xs[0] else { panic!() };
    assert!(matches!(**inner, Filter::Leaf(_)));
}

#[test]
fn not_wraps_or() {
    let f = parse(
        "filters[$not][$or][0][title][$eq]=foo&filters[$not][$or][1][title][$eq]=bar",
        &ct(),
    )
    .unwrap();
    let Filter::All(xs) = f else { panic!() };
    let Filter::Not(inner) = &xs[0] else { panic!() };
    let Filter::Any(ys) = inner.as_ref() else { panic!() };
    assert_eq!(ys.len(), 2);
}

#[test]
fn mixed_top_level_and_or() {
    let f = parse(
        "filters[published][$eq]=true\
         &filters[$or][0][title][$eq]=foo\
         &filters[$or][1][title][$eq]=bar",
        &ct(),
    )
    .unwrap();
    let Filter::All(xs) = f else { panic!() };
    assert_eq!(xs.len(), 2);
    assert!(matches!(xs[0], Filter::Leaf(_)));
    assert!(matches!(xs[1], Filter::Any(_)));
}

#[test]
fn nested_and_inside_or() {
    let f = parse(
        "filters[$or][0][$and][0][title][$eq]=foo\
         &filters[$or][0][$and][1][views][$gt]=5\
         &filters[$or][1][title][$eq]=bar",
        &ct(),
    )
    .unwrap();
    let Filter::All(xs) = f else { panic!() };
    let Filter::Any(ys) = &xs[0] else { panic!() };
    assert_eq!(ys.len(), 2);
    let Filter::All(zs) = &ys[0] else { panic!() };
    assert_eq!(zs.len(), 2);
    assert!(matches!(ys[1], Filter::Leaf(_)));
}

#[test]
fn not_with_index_rejected() {
    let err = parse("filters[$not][0][title][$eq]=foo", &ct()).unwrap_err();
    assert!(matches!(err, Error::Validation(_)));
}

#[test]
fn or_gap_in_indices_rejected() {
    let err = parse(
        "filters[$or][0][title][$eq]=foo&filters[$or][2][title][$eq]=bar",
        &ct(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Validation(_)));
}

#[test]
fn or_duplicate_index_rejected() {
    let err = parse(
        "filters[$or][0][title][$eq]=foo&filters[$or][0][title][$eq]=bar",
        &ct(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Validation(_)));
}

#[test]
fn empty_or_rejected() {
    // No way to express empty $or via URL keys directly, but if a `$or`
    // bracket is opened without children (e.g. `filters[$or]=x`), it
    // should reject. Mechanically, this exercises the malformed-key path.
    let err = parse("filters[$or]=foo", &ct()).unwrap_err();
    assert!(matches!(err, Error::Validation(_)));
}

#[test]
fn in_inside_or() {
    let f = parse(
        "filters[$or][0][views][$in][0]=1&filters[$or][0][views][$in][1]=2\
         &filters[$or][1][title][$eq]=foo",
        &ct(),
    )
    .unwrap();
    let Filter::All(xs) = f else { panic!() };
    let Filter::Any(ys) = &xs[0] else { panic!() };
    assert_eq!(ys.len(), 2);
    let Filter::Leaf(c) = &ys[0] else { panic!() };
    assert_eq!(c.op, Op::In);
    let FilterValue::List(vs) = &c.value else { panic!() };
    assert_eq!(vs.len(), 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustapi-http --lib`
Expected: failures — `or_two_leaves`, `not_unary_leaf`, etc. all panic because the current `parse_key` regex rejects keys with `$or`/`$and`/`$not` segments.

- [ ] **Step 3: Replace `parse()` body with tree builder**

The new `parse()` walks segments per key into a mutable tree. Replace the entire body of `pub fn parse(...)` (currently lines 13–84 of `crates/http/src/filter.rs`) with the following. Keep the `pub fn parse` signature exactly as-is.

```rust
pub fn parse(raw_query: &str, ct: &ContentType) -> Result<Filter, Error> {
    let mut root = TreeNode::group_all();
    let mut set_buckets: SetBuckets = std::collections::HashMap::new();

    for (k, v) in form_urlencoded::parse(raw_query.as_bytes()) {
        if !k.starts_with("filters[") && k != "filters" {
            continue;
        }
        let segs = tokenize_key(&k)?;
        insert_segments(&mut root, &segs, &v, ct, &mut set_buckets)?;
    }

    flush_set_buckets(&mut root, set_buckets, ct)?;
    finalize(root)
}
```

Then add the supporting types and functions (place them anywhere in the file, but recommended right after `tokenize_key`):

```rust
type SetBuckets = std::collections::HashMap<SetKey, SetBucket>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SetKey {
    /// Path of (combinator, index) pairs from root to the leaf's parent.
    path: Vec<(String, usize)>,
    column: String,
    op: Op,
}

struct SetBucket {
    kind: FieldKind,
    /// BTreeMap so we walk in index order and detect gaps cheaply.
    values: std::collections::BTreeMap<usize, BoundValue>,
}

/// Mutable in-progress tree. Leaves are produced directly; group ordering
/// is preserved via BTreeMap on indices.
#[derive(Debug)]
enum TreeNode {
    Leaf(Condition),
    GroupAll(std::collections::BTreeMap<usize, TreeNode>),
    GroupAny(std::collections::BTreeMap<usize, TreeNode>),
    Not(Box<Option<TreeNode>>),
}

impl TreeNode {
    fn group_all() -> Self {
        TreeNode::GroupAll(std::collections::BTreeMap::new())
    }
    fn group_any() -> Self {
        TreeNode::GroupAny(std::collections::BTreeMap::new())
    }
    fn not_empty() -> Self {
        TreeNode::Not(Box::new(None))
    }
}

fn insert_segments(
    root: &mut TreeNode,
    segs: &[Segment],
    raw_val: &str,
    ct: &ContentType,
    set_buckets: &mut SetBuckets,
) -> Result<(), Error> {
    // Root counts as one position in the "top-level All" — we always feed
    // segments into root via a synthetic next-free-index, since top-level
    // keys are unordered. Use a stable index = current child count.
    let next_idx = root_next_index(root);
    insert_into(root, next_idx, segs, &mut Vec::new(), raw_val, ct, set_buckets)
}

fn root_next_index(root: &TreeNode) -> usize {
    match root {
        TreeNode::GroupAll(m) => m.len(),
        _ => 0,
    }
}

fn insert_into(
    parent: &mut TreeNode,
    at_idx: usize,
    segs: &[Segment],
    path: &mut Vec<(String, usize)>,
    raw_val: &str,
    ct: &ContentType,
    set_buckets: &mut SetBuckets,
) -> Result<(), Error> {
    match segs.first() {
        Some(Segment::Combinator(tag)) if tag == "$not" => {
            // $not consumes one index slot in its parent. Build/locate a
            // singleton GroupAll holder at index 0 inside Not's slot, and
            // delegate. We finalize the Not by unwrapping the single child.
            let inner_segs = &segs[1..];
            if inner_segs.is_empty() {
                return Err(generic_err("$not requires a child"));
            }
            // $not must NOT be followed immediately by an Index segment.
            if matches!(inner_segs.first(), Some(Segment::Index(_))) {
                return Err(generic_err("$not must be unary"));
            }
            let parent_map = parent_group_map_mut(parent)?;
            let entry = parent_map
                .entry(at_idx)
                .or_insert_with(TreeNode::not_empty);
            let TreeNode::Not(slot) = entry else {
                return Err(generic_err("$not collides with non-$not child at same index"));
            };
            // Use a stable singleton GroupAll holder inside `slot`. This lets
            // the recursive insert use the same group-map machinery and lets
            // set-op flush walk the path uniformly.
            if slot.is_none() {
                **slot = Some(TreeNode::group_all());
            }
            let holder = slot
                .as_mut()
                .as_mut()
                .expect("just initialized");
            path.push(("$not".to_string(), 0));
            // Inside the singleton holder, the $not child always lives at index 0.
            insert_into(holder, 0, inner_segs, path, raw_val, ct, set_buckets)?;
            path.pop();
            Ok(())
        }
        Some(Segment::Combinator(tag)) => {
            // $or or $and — expect Index next.
            let group_tag = tag.clone();
            let idx_seg = segs.get(1);
            let Some(Segment::Index(child_idx)) = idx_seg else {
                return Err(generic_err(&format!(
                    "{group_tag} group requires bracketed index next"
                )));
            };
            let parent_map = parent_group_map_mut(parent)?;
            let group_entry = parent_map
                .entry(at_idx)
                .or_insert_with(|| if group_tag == "$or" {
                    TreeNode::group_any()
                } else {
                    TreeNode::group_all()
                });
            ensure_matches_combinator(group_entry, &group_tag)?;
            path.push((group_tag, *child_idx));
            let remainder = &segs[2..];
            insert_into(group_entry, *child_idx, remainder, path, raw_val, ct, set_buckets)?;
            path.pop();
            Ok(())
        }
        Some(Segment::Name(col)) => {
            // Leaf — segs is [Name, Op] or [Name, Op, Index] for $in/$nin.
            let op_seg = segs.get(1).ok_or_else(|| field_err(col, "missing operator"))?;
            let Segment::Op(op_str) = op_seg else {
                return Err(field_err(col, "expected operator after column"));
            };
            let op = map_op(op_str, col)?;
            let field = field_for(ct, col)?;
            let kind = field.kind();
            if !rustapi_sql::op_allows_kind(op, kind) {
                return Err(field_err(
                    col,
                    format!("operator `{op_str}` invalid for kind `{kind:?}`"),
                ));
            }
            let extra = segs.get(2);
            let is_set_op = matches!(op, Op::In | Op::NotIn);
            match (is_set_op, extra) {
                (true, Some(Segment::Index(i))) => {
                    if raw_val.eq_ignore_ascii_case("null") {
                        return Err(field_err(col, "set operator entries cannot be null"));
                    }
                    let bv = coerce_bound(kind, col, raw_val)?;
                    let key = SetKey {
                        path: path.clone(),
                        column: col.clone(),
                        op,
                    };
                    let bucket = set_buckets
                        .entry(key)
                        .or_insert_with(|| SetBucket {
                            kind,
                            values: std::collections::BTreeMap::new(),
                        });
                    if bucket.values.insert(*i, bv).is_some() {
                        return Err(field_err(col, "duplicate set operator entry"));
                    }
                    if bucket.values.len() > 100 {
                        return Err(field_err(col, "set operator limited to 100 items"));
                    }
                    Ok(())
                }
                (true, _) => Err(field_err(col, "set operator requires bracketed list indices")),
                (false, Some(_)) => Err(field_err(col, "unexpected list index for operator")),
                (false, None) => {
                    let value = coerce_value(field, op, col, raw_val)?;
                    insert_leaf(
                        parent,
                        at_idx,
                        Condition::new(col, kind, op, value),
                    )
                }
            }
        }
        Some(Segment::Op(_) | Segment::Index(_)) | None => {
            Err(generic_err("malformed filter key shape"))
        }
    }
}

fn insert_leaf(parent: &mut TreeNode, at_idx: usize, c: Condition) -> Result<(), Error> {
    let map = parent_group_map_mut(parent)?;
    if map.insert(at_idx, TreeNode::Leaf(c)).is_some() {
        return Err(generic_err("duplicate filter at same path"));
    }
    Ok(())
}

fn parent_group_map_mut(
    parent: &mut TreeNode,
) -> Result<&mut std::collections::BTreeMap<usize, TreeNode>, Error> {
    match parent {
        TreeNode::GroupAll(m) | TreeNode::GroupAny(m) => Ok(m),
        _ => Err(generic_err("internal: cannot insert into non-group parent")),
    }
}

fn ensure_matches_combinator(node: &TreeNode, tag: &str) -> Result<(), Error> {
    let ok = match (tag, node) {
        ("$or", TreeNode::GroupAny(_)) => true,
        ("$and", TreeNode::GroupAll(_)) => true,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(generic_err(&format!(
            "combinator `{tag}` collides with existing node at the same path"
        )))
    }
}

fn flush_set_buckets(
    root: &mut TreeNode,
    set_buckets: SetBuckets,
    _ct: &ContentType,
) -> Result<(), Error> {
    for (key, bucket) in set_buckets {
        if bucket.values.is_empty() {
            return Err(field_err(&key.column, "set operator requires non-empty list"));
        }
        // Detect index gaps: keys must be 0..len.
        for (expected, actual) in bucket.values.keys().enumerate() {
            if expected != *actual {
                return Err(field_err(&key.column, "gap in set operator indices"));
            }
        }
        let values: Vec<BoundValue> = bucket.values.into_values().collect();
        let cond = Condition::new(
            key.column.clone(),
            bucket.kind,
            key.op,
            FilterValue::List(values),
        );
        insert_at_path(root, &key.path, cond)?;
    }
    Ok(())
}

fn insert_at_path(
    root: &mut TreeNode,
    path: &[(String, usize)],
    cond: Condition,
) -> Result<(), Error> {
    let mut node = root;
    for (tag, idx) in path {
        if tag == "$not" {
            // ("$not", 0) means: step into the Not slot, then into its
            // singleton GroupAll holder.
            let TreeNode::Not(slot) = node else {
                return Err(generic_err("internal: expected Not at $not path step"));
            };
            node = slot.as_mut().as_mut().ok_or_else(|| {
                generic_err("internal: $not slot empty during flush")
            })?;
            // node is now the singleton GroupAll holder. The original
            // recursion deposited the bucketed leaf-parent at index 0.
            let map = parent_group_map_mut(node)?;
            node = map.get_mut(&0).ok_or_else(|| {
                generic_err("internal: $not holder empty during flush")
            })?;
        } else {
            let map = parent_group_map_mut(node)?;
            node = map.get_mut(idx).ok_or_else(|| {
                generic_err("internal: set-bucket path missing during flush")
            })?;
        }
    }
    let map = parent_group_map_mut(node)?;
    let next = map.len();
    map.insert(next, TreeNode::Leaf(cond));
    Ok(())
}

fn finalize(root: TreeNode) -> Result<Filter, Error> {
    let TreeNode::GroupAll(map) = root else {
        return Err(generic_err("internal: root is not All"));
    };
    if map.is_empty() {
        return Ok(Filter::None);
    }
    // Ordered walk via BTreeMap iteration; verify dense 0..len.
    for (expected, actual) in map.keys().enumerate() {
        if expected != *actual {
            return Err(generic_err("gap in top-level filter ordering"));
        }
    }
    let children: Vec<Filter> = map.into_values().map(node_to_filter).collect::<Result<_, _>>()?;
    Ok(Filter::All(children))
}

fn node_to_filter(node: TreeNode) -> Result<Filter, Error> {
    match node {
        TreeNode::Leaf(c) => Ok(Filter::Leaf(c)),
        TreeNode::GroupAll(map) => {
            if map.is_empty() {
                return Err(generic_err("empty $and group"));
            }
            for (expected, actual) in map.keys().enumerate() {
                if expected != *actual {
                    return Err(generic_err("gap in $and group indices"));
                }
            }
            let xs: Vec<Filter> = map.into_values().map(node_to_filter).collect::<Result<_, _>>()?;
            Ok(Filter::All(xs))
        }
        TreeNode::GroupAny(map) => {
            if map.is_empty() {
                return Err(generic_err("empty $or group"));
            }
            for (expected, actual) in map.keys().enumerate() {
                if expected != *actual {
                    return Err(generic_err("gap in $or group indices"));
                }
            }
            let xs: Vec<Filter> = map.into_values().map(node_to_filter).collect::<Result<_, _>>()?;
            Ok(Filter::Any(xs))
        }
        TreeNode::Not(slot) => {
            let inner = slot.ok_or_else(|| generic_err("$not requires a child"))?;
            // The holder is always a singleton GroupAll wrapping exactly one
            // child. Unwrap and recurse on the actual child.
            let unwrapped = match inner {
                TreeNode::GroupAll(mut m) if m.len() == 1 => {
                    let key = *m.keys().next().expect("len==1 just checked");
                    m.remove(&key).expect("key just observed")
                }
                TreeNode::GroupAll(_) => {
                    return Err(generic_err("$not holder must have exactly one child"));
                }
                other => other,
            };
            Ok(Filter::Not(Box::new(node_to_filter(unwrapped)?)))
        }
    }
}

fn generic_err(msg: &str) -> Error {
    Error::Validation(ValidationErrors::single(msg.to_string()))
}
```

Also delete the now-unused `parse_key` function (currently lines 105–122). Its `regex` use is replaced by `tokenize_key`. If `regex` becomes unused entirely in the file, `cargo clippy` will flag it — remove the `use regex...` line / extern crate reference if present.

Remove the `#[allow(dead_code)]` you added to `Segment`/`tokenize_key`/`classify_segment` in Task 4 — they're now in use by `parse()`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustapi-http`
Expected: PASS — all phase 2.1/2.2 tests still pass (top-level keys still produce `Filter::All(vec![Leaf, Leaf, ...])`), plus all new combinator tests added in Step 1.

Run: `cargo test --workspace`
Expected: PASS — full workspace stays green.

- [ ] **Step 5: Lint**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/filter.rs
git commit -m "$(cat <<'EOF'
feat(http): recursive parser for \$or / \$and / \$not

Replaces the flat parse loop with a tree builder that walks
Segment streams from tokenize_key. Top-level keys still produce
Filter::All(vec![Leaf, ...]) for byte-identical phase 2.1/2.2
output; combinator keys nest into Any/All/Not groups. Set-op
buckets carry the parent path so \$in inside \$or works.
EOF
)"
```

---

## Task 6: Depth and Leaf-Count Caps

**Files:**
- Modify: `crates/http/src/filter.rs`

Enforce depth ≤ 8 and total leaf count ≤ 100 in `parse()`. Both checks happen during tree construction so failure is immediate and cheap.

- [ ] **Step 1: Write failing tests**

Append to the existing `mod tests` block:

```rust
#[test]
fn depth_8_allowed() {
    // 8 combinator levels deep: $or > $or > ... > $or (×8) > leaf.
    let mut q = String::new();
    for _ in 0..8 {
        q.push_str("[$or][0]");
    }
    let key = format!("filters{q}[title][$eq]");
    let url = format!("{key}=foo");
    let _ = parse(&url, &ct()).unwrap();
}

#[test]
fn depth_9_rejected() {
    let mut q = String::new();
    for _ in 0..9 {
        q.push_str("[$or][0]");
    }
    let key = format!("filters{q}[title][$eq]");
    let url = format!("{key}=foo");
    let err = parse(&url, &ct()).unwrap_err();
    assert!(matches!(err, Error::Validation(_)));
}

#[test]
fn leaf_count_100_allowed() {
    let mut parts = Vec::new();
    for i in 0..100 {
        parts.push(format!("filters[$or][{i}][title][$eq]=v{i}"));
    }
    let url = parts.join("&");
    let _ = parse(&url, &ct()).unwrap();
}

#[test]
fn leaf_count_101_rejected() {
    let mut parts = Vec::new();
    for i in 0..101 {
        parts.push(format!("filters[$or][{i}][title][$eq]=v{i}"));
    }
    let url = parts.join("&");
    let err = parse(&url, &ct()).unwrap_err();
    assert!(matches!(err, Error::Validation(_)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustapi-http --lib`
Expected: `depth_9_rejected` and `leaf_count_101_rejected` fail (no cap enforced yet). `depth_8_allowed` and `leaf_count_100_allowed` pass.

- [ ] **Step 3: Add caps**

At the top of `parse()`, declare constants. Add inside `pub fn parse` body, above the loop:

```rust
const MAX_DEPTH: usize = 8;
const MAX_LEAVES: usize = 100;

let mut leaf_count: usize = 0;
```

Pass these into the recursion. Update the `insert_segments` and `insert_into` signatures to take `&mut usize` for `leaf_count` and a depth counter. Concretely:

Change `insert_segments` signature to:
```rust
fn insert_segments(
    root: &mut TreeNode,
    segs: &[Segment],
    raw_val: &str,
    ct: &ContentType,
    set_buckets: &mut SetBuckets,
    leaf_count: &mut usize,
    max_leaves: usize,
    max_depth: usize,
) -> Result<(), Error> {
    let next_idx = root_next_index(root);
    insert_into(
        root,
        next_idx,
        segs,
        &mut Vec::new(),
        raw_val,
        ct,
        set_buckets,
        leaf_count,
        max_leaves,
        max_depth,
        0,
    )
}
```

And `insert_into` gains a `depth: usize` parameter (and `leaf_count`/`max_leaves`/`max_depth` flowed through). At the very top of `insert_into`:

```rust
if depth > max_depth {
    return Err(generic_err("filter nesting depth exceeds 8"));
}
```

For non-set-op leaf insertion (the `(false, None)` arm) and the set-op insertion arm — increment `*leaf_count` by 1 (set ops count once as a leaf, not per value). Right after the increment:

```rust
if *leaf_count > max_leaves {
    return Err(generic_err("filter leaf count exceeds 100"));
}
```

For the set-op insertion, increment the leaf count only on the first index seen for that `SetKey` (i.e., when `set_buckets.get(&key).is_none()`). Adjust accordingly.

Every recursive call to `insert_into` from inside `$or`/`$and`/`$not` arms must pass `depth + 1`.

Update the call site in `parse()`:
```rust
insert_segments(
    &mut root,
    &segs,
    &v,
    ct,
    &mut set_buckets,
    &mut leaf_count,
    MAX_LEAVES,
    MAX_DEPTH,
)?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustapi-http`
Expected: PASS — all 4 new cap tests, all earlier tests still green.

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Lint**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/filter.rs
git commit -m "$(cat <<'EOF'
feat(http): depth cap 8 and leaf cap 100 on filter parser

Enforces both at tree-construction time so malformed payloads
fail fast. Set-op leaves count once regardless of value count;
\$in values keep their separate per-bucket cap of 100.
EOF
)"
```

---

## Task 7: Tighten Validation Edges

**Files:**
- Modify: `crates/http/src/filter.rs`

Sweep the remaining edge cases from the spec: `$not` empty, unknown combinator tokens, and confirm error messages match the spec table. Most are covered by Tasks 5 and 6; this task ensures specific messages and a few corner combos.

- [ ] **Step 1: Write failing tests**

Append:

```rust
#[test]
fn unknown_combinator_rejected() {
    let err = parse("filters[$xor][0][title][$eq]=foo", &ct()).unwrap_err();
    assert!(matches!(err, Error::Validation(_)));
}

#[test]
fn not_empty_after_strip_rejected() {
    // `filters[$not][$not]...` with no leaf below should reject.
    let err = parse("filters[$not][$not]", &ct()).unwrap_err();
    assert!(matches!(err, Error::Validation(_)));
}

#[test]
fn or_then_not_then_leaf() {
    let f = parse(
        "filters[$or][0][$not][title][$eq]=foo&filters[$or][1][views][$gt]=1",
        &ct(),
    )
    .unwrap();
    let Filter::All(xs) = f else { panic!() };
    let Filter::Any(ys) = &xs[0] else { panic!() };
    assert_eq!(ys.len(), 2);
    let Filter::Not(inner) = &ys[0] else { panic!() };
    assert!(matches!(**inner, Filter::Leaf(_)));
}

#[test]
fn explicit_top_level_and_not_flattened() {
    // `$and` at the top level is its own group, not merged into the
    // implicit top-level All.
    let f = parse(
        "filters[published][$eq]=true\
         &filters[$and][0][title][$eq]=foo\
         &filters[$and][1][views][$gt]=0",
        &ct(),
    )
    .unwrap();
    let Filter::All(xs) = f else { panic!() };
    assert_eq!(xs.len(), 2);
    assert!(matches!(xs[0], Filter::Leaf(_)));
    let Filter::All(inner) = &xs[1] else { panic!("expected nested All") };
    assert_eq!(inner.len(), 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustapi-http --lib`
Expected: `unknown_combinator_rejected` and `not_empty_after_strip_rejected` likely fail (depends on Task 5 wiring); `or_then_not_then_leaf` and `explicit_top_level_and_not_flattened` should pass already if Task 5 was correct.

- [ ] **Step 3: Tighten classifications**

In `crates/http/src/filter.rs::classify_segment`, treat any `$x` token whitelisted operators OR combinators specially; the others become `Op` strings, which fall to `map_op` and fail. That's fine for unknown ops, but unknown combinators like `$xor` currently classify as `Combinator` and crash later with a less specific message.

Update `classify_segment`:

```rust
fn classify_segment(raw: &str) -> Segment {
    match raw {
        "$or" | "$and" | "$not" => Segment::Combinator(raw.to_string()),
        s if s.starts_with('$') => Segment::Op(s.to_string()),
        s => match s.parse::<usize>() {
            Ok(n) => Segment::Index(n),
            Err(_) => Segment::Name(s.to_string()),
        },
    }
}
```

(this is the same as Task 4 — keep as-is). The fix for `$xor` lands in `insert_into`: when the first segment is `Segment::Op` (because `$xor` starts with `$` but is not whitelisted), the current code falls through `Some(Segment::Op(_)) | Some(Segment::Index(_)) | None` and returns `"malformed filter key shape"`. Strengthen the message:

```rust
Some(Segment::Op(s)) => Err(generic_err(&format!(
    "unexpected operator `{s}` at this position (expected combinator or column)"
))),
Some(Segment::Index(_)) | None => Err(generic_err("malformed filter key shape")),
```

For `$not` with no child segment (`filters[$not]` alone), `tokenize_key` returns `[Combinator("$not")]`, then `insert_into` hits the `inner_segs.is_empty()` branch which already returns "$not requires a child" — covered.

For `filters[$not][$not]`, `tokenize_key` returns `[Combinator("$not"), Combinator("$not")]`. The recursive `insert_into` call for the inner `$not` has `inner_segs = [Combinator("$not")]`, which then recurses into another `inner_segs.is_empty()` failure — message "$not requires a child". Test should pass.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustapi-http`
Expected: PASS.

- [ ] **Step 5: Lint and full workspace**

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: clean.

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/filter.rs
git commit -m "$(cat <<'EOF'
feat(http): sharper error messages for combinator edges

Unknown \$xor-style tokens classify as Op and fail with a clear
position message. \$not with no child or chained \$not\$not paths
hit the existing 'requires a child' guard. Adds tests for
explicit top-level \$and not flattening and \$or wrapping \$not.
EOF
)"
```

---

## Task 8: Integration Tests

**Files:**
- Create: `crates/bin/tests/integration_filters_2_3.rs`

End-to-end coverage against a real Postgres via testcontainers (already used by sibling integration files).

- [ ] **Step 1: Create the test file**

Write the following to `crates/bin/tests/integration_filters_2_3.rs`. Follow the shape of `integration_filters_2_2.rs` (already exists at the same level — read it for the helper pattern):

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
async fn or_two_categories() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$or][0][category][$eq]=tech&filters[$or][1][category][$eq]=design",
    )
    .await;
    assert_eq!(body["meta"]["total"], 4);
}

#[tokio::test]
async fn or_mixing_ops() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$or][0][category][$eq]=tech&filters[$or][1][views][$gt]=15",
    )
    .await;
    // tech: foo, barfoo (2). views>15: xyz (1). Distinct rows = 3.
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn not_excludes_leaf_and_nulls() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    // NOT (views = 0) — by Postgres 3VL, NULL views also excluded.
    let body = list_body(&app, "filters[$not][views][$eq]=0").await;
    // Total rows = 5; views=0 = foo (1); views=NULL = null-vw (1).
    // NOT excludes both → 3 surviving rows.
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn not_of_or() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$not][$or][0][category][$eq]=tech&filters[$not][$or][1][category][$eq]=design",
    )
    .await;
    // Rows where category is neither tech nor design AND not null.
    // xyz has category=null → excluded by 3VL. So 0 rows match.
    assert_eq!(body["meta"]["total"], 0);
}

#[tokio::test]
async fn nested_or_inside_and() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$and][0][published][$eq]=true\
         &filters[$and][1][$or][0][category][$eq]=tech\
         &filters[$and][1][$or][1][category][$eq]=design",
    )
    .await;
    // published=true AND (tech OR design):
    //   foo (tech, pub=true), foobar (design, pub=true), null-vw (design, pub=true) → 3.
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn implicit_and_with_or() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[published][$eq]=true\
         &filters[$or][0][category][$eq]=tech\
         &filters[$or][1][category][$eq]=design",
    )
    .await;
    // Same as nested test above — should be 3.
    assert_eq!(body["meta"]["total"], 3);
}

#[tokio::test]
async fn or_with_pagination_and_sort() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    seed(&app).await;
    let body = list_body(
        &app,
        "filters[$or][0][views][$gt]=5\
         &filters[$or][1][views][$null]=true\
         &sort=views:asc\
         &page=1&pageSize=2",
    )
    .await;
    assert_eq!(body["meta"]["total"], 3); // 10, 20, null
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn depth_cap_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let mut q = String::new();
    for _ in 0..9 {
        q.push_str("[$or][0]");
    }
    q.push_str("[title][$eq]=foo");
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post?filters{q}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn leaf_cap_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let mut parts = Vec::new();
    for i in 0..101 {
        parts.push(format!("filters[$or][{i}][title][$eq]=v{i}"));
    }
    let q = parts.join("&");
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/post?{q}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn empty_or_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(app.url("/api/post?filters[$or]=foo")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn or_gap_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(
            app.url("/api/post?filters[$or][0][title][$eq]=a&filters[$or][2][title][$eq]=b"),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn not_with_index_rejected_422() {
    let app = TestApp::spawn().await;
    make_type(&app).await;
    let resp = app
        .admin(app.client.get(
            app.url("/api/post?filters[$not][0][title][$eq]=a"),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}
```

- [ ] **Step 2: Run the new file**

Run: `cargo test -p rustapi-bin --test integration_filters_2_3`
Expected: PASS — 12 tests green.

- [ ] **Step 3: Full workspace test**

Run: `cargo test --workspace`
Expected: PASS — target around 210 tests total.

Run: `cargo clippy --all-targets -- -Dwarnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/bin/tests/integration_filters_2_3.rs
git commit -m "$(cat <<'EOF'
test(bin): integration coverage for phase 2.3 combinators

Real-Postgres coverage of \$or, \$not, \$and composition, NULL
3VL semantics for NOT, mixed implicit-AND + \$or top level,
depth cap (422), leaf cap (422), empty \$or (422), gap (422),
\$not with index (422), and combinator + pagination + sort.
EOF
)"
```

---

## Done Criteria

- All 8 tasks above committed sequentially.
- `cargo test --workspace` passes, ~210 tests total.
- `cargo clippy --all-targets -- -Dwarnings` clean.
- Every phase 2.1 and 2.2 integration test still passes unchanged (no expected-output drift).
- `git log --oneline` shows discrete, focused commits per task.

After completing, invoke `superpowers:finishing-a-development-branch` (no branch flow in this repo today, but the skill's checklist still applies).
