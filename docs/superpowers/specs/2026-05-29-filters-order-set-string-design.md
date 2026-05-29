# rustapi — Phase 2.2: Order + Set + String Filter Operators (Design)

**Date:** 2026-05-29
**Status:** Approved for implementation planning
**Scope:** Second slice of roadmap §8 phase 2. Adds 11 new filter operators on
`GET /api/:type`. Composes with the phase 2.1 `$eq` / `$ne` / `$null` operators
already shipped.

This builds on the [v1 core design](2026-05-28-rustapi-core-design.md) and
the [phase 2.1 filters design](2026-05-29-filters-eq-ne-null-design.md), which
shipped the `Filter::All(Vec<Condition>)` seam, the `render_where` emitter, and
the bracket-syntax parser.

---

## 1. Goals & Non-Goals

### Goals

- Add 11 operators to `GET /api/:type`:
  - **Order:** `$gt`, `$gte`, `$lt`, `$lte`
  - **Set:** `$in`, `$nin`
  - **String:** `$contains`, `$startsWith`, `$endsWith`, `$containsi`
- Implicit AND across params keeps working (phase 2.1 invariant).
- Strict op-kind compatibility matrix enforced at parse time with 422.
- Wire format stays Strapi-style brackets; set ops use `[col][$op][idx]`.
- `$in / $nin` lists are capped at 100 entries.
- LIKE metacharacters (`%`, `_`, `\`) in user input are escaped server-side so
  values are treated as literals.

### Non-Goals (this slice)

- No `$or` / `$not` combinators (phase 2.3).
- No regex / iregex operators.
- No order operators on `string`, `text`, `boolean`, or `uuid` kinds (PG allows
  these comparisons but no user need surfaced; gated behind 422 for now).
- No `$between` — use `$gte` + `$lte`.
- No `$exists` — use `$null=false`.
- No relations / `?populate=` (phase 2.4).

---

## 2. Wire Format

### 2.1 Order, set, string examples

```
GET /api/post
  ?filters[views][$gt]=5
  &filters[views][$lte]=100
  &filters[category][$in][0]=tech
  &filters[category][$in][1]=design
  &filters[title][$contains]=hello
  &filters[title][$containsi]=WORLD
  &filters[slug][$startsWith]=blog-
  &filters[slug][$endsWith]=-2026
```

### 2.2 Bracket regex

The phase 2.1 regex is extended to allow an optional integer index suffix and
mixed-case operator names (for `$containsi`):

```
^filters\[(?P<col>[^\[\]]+)\]\[(?P<op>\$[a-zA-Z]+)\](?:\[(?P<idx>\d+)\])?$
```

- Non-set ops must have no index suffix. Stray index on a non-set op → 422
  `unexpected list index for operator`.
- Set ops (`$in`, `$nin`) must have an index suffix. Missing index → 422
  `set operator requires bracketed list indices`.

### 2.3 Set operator (`$in` / `$nin`) value handling

- Index numbers in `[idx]` need not be contiguous or zero-based; the parser
  collects them, sorts by ascending index, and emits the values in that order.
  So `[0]=x&[2]=y` and `[5]=x&[7]=y` both produce the list `[x, y]` in the
  SQL `IN (...)` clause.
- Duplicate `(col, op, idx)` triples → 422 `duplicate set operator entry`.
- Empty list (e.g. `?filters[x][$in]=` with no indices) → 422 `set operator
  requires non-empty list`.
- Hard cap: more than **100** entries per list → 422 `set operator limited to
  100 items`.
- All entries coerce to the column's kind; mixing fails per-value as a
  coercion error (the parser stops at the first failure and 422s).
- Literal value `null` inside a set entry is **rejected**. Clients filter for
  NULL via `$null=true|false`. NULL inside a SQL `NOT IN (...)` would make the
  whole expression unknown; rejecting up front avoids the foot-gun.

### 2.4 String operator value handling

For `$contains`, `$startsWith`, `$endsWith`, `$containsi` only:

- Raw value is coerced to `String`/`Text` (rejection for any other kind).
- `escape_like(raw)` is applied: substitutes `\` → `\\`, then `%` → `\%`, then
  `_` → `\_`. Backslash first so we don't double-escape our own substitutions.
- The escaped value is wrapped per op:

| Op           | Wrapped bound value     |
|--------------|-------------------------|
| `$contains`  | `%<escaped>%`           |
| `$containsi` | `%<escaped>%`           |
| `$startsWith`| `<escaped>%`            |
| `$endsWith`  | `%<escaped>`            |

- The literal string `"null"` is **not** rewritten to `IS NULL` for string ops.
  The phase 2.1 `$eq=null` rewrite only applies to `$eq` and `$ne`.

### 2.5 Op-kind compatibility matrix

Enforced by `op_allows_kind(op, kind) -> bool` at parse time. Mismatch → 422
`operator $<op> invalid for kind <kind>`.

| Op                                              | string | text | integer | float | boolean | datetime | uuid |
|-------------------------------------------------|:------:|:----:|:-------:|:-----:|:-------:|:--------:|:----:|
| `Eq`, `Ne`, `IsNull` (phase 2.1)                |   ✓    |  ✓   |    ✓    |   ✓   |    ✓    |    ✓     |  ✓   |
| `Gt`, `Gte`, `Lt`, `Lte`                        |   ✗    |  ✗   |    ✓    |   ✓   |    ✗    |    ✓     |  ✗   |
| `In`, `NotIn`                                   |   ✓    |  ✓   |    ✓    |   ✓   |    ✓    |    ✓     |  ✓   |
| `Contains`, `StartsWith`, `EndsWith`, `ContainsI` | ✓   |  ✓   |    ✗    |   ✗   |    ✗    |    ✗     |  ✗   |

---

## 3. `rustapi-sql` Changes

### 3.1 `Op` and `FilterValue`

`crates/sql/src/filter.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Op {
    // Phase 2.1
    Eq, Ne, IsNull,
    // Phase 2.2 (this slice)
    Gt, Gte, Lt, Lte,
    In, NotIn,
    Contains, StartsWith, EndsWith, ContainsI,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FilterValue {
    Bound(BoundValue),
    Null(bool),
    /// Used by `$in` / `$nin`. Empty list is rejected upstream by the parser
    /// AND defensively re-checked by `render_where`.
    List(Vec<BoundValue>),
}
```

### 3.2 `op_allows_kind`

```rust
pub fn op_allows_kind(op: Op, kind: FieldKind) -> bool { ... }
```

- Lives in `crates/sql/src/filter.rs` next to the types it relates to.
- Implements the §2.5 matrix exhaustively.
- Returns `false` for unknown future `Op` / `FieldKind` variants (both are
  `#[non_exhaustive]`).
- Exposed via `crates/sql/src/lib.rs` re-export for the parser.

### 3.3 `render_where` extension

`crates/sql/src/dml.rs`. Per-condition emission rules:

**Order ops** (`Gt | Gte | Lt | Lte`):

| `(op, value)`                | SQL fragment              |
|------------------------------|---------------------------|
| `(Gt,  Bound(v))`            | `"col" >  $N::<cast>`     |
| `(Gte, Bound(v))`            | `"col" >= $N::<cast>`     |
| `(Lt,  Bound(v))`            | `"col" <  $N::<cast>`     |
| `(Lte, Bound(v))`            | `"col" <= $N::<cast>`     |
| `(order, Null(_))` or `(order, List(_))` | `Err(DmlError::InvalidFilter("order op requires Bound value"))` |
| `(order, Bound(Null(_)))`    | `Err(DmlError::InvalidFilter("order op cannot compare against NULL"))` |

**Set ops** (`In | NotIn`):

| `(op, value)`                | SQL fragment                                  |
|------------------------------|-----------------------------------------------|
| `(In,    List(vs))` non-empty| `"col" IN ($N::<cast>, $N+1::<cast>, ...)`   |
| `(NotIn, List(vs))` non-empty| `"col" NOT IN ($N::<cast>, $N+1::<cast>, ...)`|
| `(set, List(empty))`         | `Err(DmlError::InvalidFilter("set op requires non-empty List"))` |
| `(set, Bound(_) \| Null(_))` | `Err(DmlError::InvalidFilter("set op requires List value"))` |

Each entry is bound with `pg_cast(c.kind)`. The parser guarantees all entries
share the column kind; the emitter does not re-check.

**String ops** (`Contains | StartsWith | EndsWith | ContainsI`):

| `(op, value)`                  | SQL fragment                              |
|--------------------------------|-------------------------------------------|
| `(Contains \| StartsWith \| EndsWith, Bound(Str(_)))` | `"col" LIKE $N::text ESCAPE '\'`  |
| `(ContainsI, Bound(Str(_)))`   | `"col" ILIKE $N::text ESCAPE '\'`         |
| `(string op, anything else)`   | `Err(DmlError::InvalidFilter("string op requires Bound(Str)"))` |

Bound values are pre-wrapped and pre-escaped by the parser (see §2.4), so the
emitter binds them as-is.

### 3.4 Refactor: per-group emit helpers

`render_where` is already ~50 lines; adding 8 op groups inline would make the
match unwieldy. The body extracts three helpers:

```rust
fn emit_order(op: Op, col: &str, v: &BoundValue, kind: FieldKind, placeholder: &mut usize, binds: &mut Vec<BoundValue>) -> Result<String, DmlError>;
fn emit_set(op: Op, col: &str, list: &[BoundValue], kind: FieldKind, placeholder: &mut usize, binds: &mut Vec<BoundValue>) -> Result<String, DmlError>;
fn emit_like(op: Op, col: &str, v: &BoundValue, placeholder: &mut usize, binds: &mut Vec<BoundValue>) -> Result<String, DmlError>;
```

The main `render_where` match dispatches by op-group, falls through to phase
2.1 logic for `Eq | Ne | IsNull`. Each helper returns the fragment OR an
`InvalidFilter` error. All helpers thread the `placeholder` cursor explicitly
to keep the bind-vs-placeholder invariant the same as phase 2.1 (placeholder
increments only when a bind is pushed).

### 3.5 Unit tests (no DB)

Golden-string tests appended to the `where_tests` module:

- One test per new op covering happy-path fragment and bind vector.
- `$in` with two values → `"col" IN ($1::int8, $2::int8)`.
- `$nin` with three values.
- `$in` with empty `List` → `InvalidFilter`.
- `$gt` with `Null(_)` value → `InvalidFilter`.
- `$contains` against non-Str `Bound` → `InvalidFilter`.
- `$containsi` emits `ILIKE` not `LIKE`.
- `ESCAPE '\'` clause present on all four string ops.
- Combined: `$gt` + `$contains` + phase-2.1 `$eq` → all ANDed in placeholder
  order with correct casts.

---

## 4. `rustapi-http` Changes

### 4.1 Parser extension

`crates/http/src/filter.rs`:

1. Regex updated per §2.2 (mixed-case ops + optional `[idx]`).
2. New `op` mapping for the 11 strings → `Op` variants.
3. `op_allows_kind` check immediately after resolving the kind. Failure → 422
   with `details.fields[0] = {field: col, reason: "operator $<op> invalid for kind <kind>"}`.
4. Set-op handling:
   - Maintain a `HashMap<(String, Op), BTreeMap<usize, BoundValue>>` (column-op
     pairs to indexed values).
   - Each pass-through inserts at the parsed `idx`.
   - Duplicate `(col, op, idx)` → 422 `duplicate set operator entry`.
   - Missing index for set op → 422 (per §2.2).
   - At end of loop: validate non-empty + size ≤ 100; build a single
     `Condition` per `(col, op)` with `FilterValue::List(values)`.
5. String-op handling:
   - After `coerce_bound` returns a `Str`, apply `escape_like` + per-op wrap
     in a helper `fn wrap_like(op: Op, escaped: String) -> String`.
6. Coercion failure (bad integer in `$in` list, bad datetime in `$gt`, etc.)
   returns 422 with field-level reason.

### 4.2 New helpers

```rust
fn escape_like(raw: &str) -> String;   // replaces \, %, _
fn wrap_like(op: Op, escaped: String) -> String;  // adds % wildcards per op
```

Both pure functions, exhaustively tested.

### 4.3 Parser unit tests (new)

- Each new op string maps to the right `Op` variant.
- `$gt` on string → 422.
- `$contains` on integer → 422.
- `$gt` on datetime parses RFC3339.
- `$in[0]=a&$in[1]=b` → `Condition` with `List([a, b])`.
- `$in` missing index → 422.
- `$in[0]=a&$in[0]=b` (duplicate index) → 422.
- `$in` with no entries (e.g. only `?filters[a][$in]=` with no idx — caught as
  missing index path) → 422.
- `$in` with 101 entries → 422.
- `$contains=50%` → bound value is `%50\%%`.
- `$contains=_x` → bound value is `%\_x%`.
- `$contains=a\b` → bound value is `%a\\b%`.
- `$startsWith=foo` → bound value is `foo%`.
- `$endsWith=foo` → bound value is `%foo`.
- `$containsi=FOO` → bound value is `%FOO%` AND op is `ContainsI`.
- Mixing a string op with an integer column → 422.

---

## 5. `rustapi-bin` Integration Tests

`crates/bin/tests/integration_filters_2_2.rs`. Seed five rows on the v2.1
`post` type (`title:string`, `views:integer`, `published:boolean`, `category:string`).

| title | views | published | category |
|-------|------:|:---------:|----------|
| `foo`     | 0    | true   | `tech`    |
| `barfoo`  | 5    | false  | `tech`    |
| `foobar`  | 10   | true   | `design`  |
| `null-vw` | null | true   | `design`  |
| `xyz`     | 20   | false  | null      |

Tests:

- `gt_excludes_nulls` — `views[$gt]=5` → 2 rows (`foobar`, `xyz`).
- `gte_inclusive` — `views[$gte]=5` → 3 rows.
- `lt_basic` — `views[$lt]=10` → 2 rows.
- `lte_inclusive` — `views[$lte]=10` → 3 rows.
- `gt_on_string_rejected_422`.
- `gt_on_datetime_works` — uses `created_at[$gt]=<rfc3339>` to fetch recent.
- `in_two_categories` — `category[$in][0]=tech&[1]=design` → 4 rows.
- `nin_excludes` — `views[$nin][0]=0&[1]=20` → 2 rows (NULL excluded by `NOT IN`).
- `in_single_value` — `category[$in][0]=tech` behaves as `$eq`.
- `in_missing_index_rejected_422`.
- `in_duplicate_index_rejected_422`.
- `in_over_cap_rejected_422` — 101 entries.
- `contains_basic` — `title[$contains]=foo` → 3 rows (`foo`, `barfoo`, `foobar`).
- `containsi_case_insensitive` — `title[$containsi]=FOO` → 3 rows.
- `starts_with` — `title[$startsWith]=foo` → 2 rows.
- `ends_with` — `title[$endsWith]=foo` → 2 rows.
- `contains_literal_percent` — insert a `50% off` row, query `title[$contains]=50%`,
  assert it matches.
- `contains_on_integer_rejected_422`.
- `compose_multiple_groups` — `title[$contains]=foo&views[$gt]=0&category[$in][0]=tech&[1]=design`.
- `count_reflects_filter_for_new_ops`.

All v1, phase 2.1, and existing 2.1 integration tests must remain green.

---

## 6. Errors

All errors continue to use the v1 JSON shape (`{error: {code, message, details?}}`).
This slice adds these specific 422 reasons:

| Condition                                       | Reason text                                       |
|-------------------------------------------------|---------------------------------------------------|
| Op-kind incompatibility                         | `operator $<op> invalid for kind <kind>`          |
| Set op missing index                            | `set operator requires bracketed list indices`    |
| Non-set op with stray index                     | `unexpected list index for operator`              |
| Duplicate `(col, op, idx)` for set op           | `duplicate set operator entry`                    |
| Empty set list                                  | `set operator requires non-empty list`            |
| Set list over cap                               | `set operator limited to 100 items`               |
| `null` literal inside set list                  | `set operator entries cannot be null`             |
| Coercion failure inside set list                | per-kind reason (`expected integer`, etc.)        |

`details.db` is unaffected; the parser rejects bad operators before SQL runs.

---

## 7. Out of Scope (for explicit reference in plan)

The following are deliberately not touched:

- v1 PUT semantics, DDL mapping (already fixed).
- Phase 2.1 `$eq` / `$ne` / `$null` syntax and semantics.
- `Action` granularity / RBAC.
- `TraceLayer` wiring / handler `#[instrument]`.
- `JsonRejection` → `ApiError` mapping.
- `$or` / `$not` (phase 2.3).
- Relations (phase 2.4).

---

## 8. Open Questions

None at sign-off.
