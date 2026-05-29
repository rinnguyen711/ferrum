# rustapi — Phase 2.1: Equality + Null Filters (Design)

**Date:** 2026-05-29
**Status:** Approved for implementation planning
**Scope:** First slice of roadmap §8 phase 2. Adds `$eq`, `$ne`, `$null` filter operators
on `GET /api/:type`. No schema changes, no new field kinds, no relations.

This builds on the [v1 core design](2026-05-28-rustapi-core-design.md), which already
shipped a non-exhaustive `Filter` enum and `&Filter` parameters on `select_list`/`count`
as a seam for this work.

---

## 1. Goals & Non-Goals

### Goals

- Clients can filter list responses by user fields and system columns using
  Strapi-style bracket syntax: `?filters[col][$op]=value`.
- Operators in this slice: `$eq`, `$ne`, `$null`.
- Implicit AND across multiple filter params.
- `meta.total` on the list response reflects the filtered count.
- Invalid filters fail fast with 422 `validation_failed` plus a clear reason.
- Preserve v1 invariants: identifier choke point, error shape, warnings-as-errors.

### Non-Goals (this slice)

- No order operators (`$gt $gte $lt $lte`) — phase 2.2.
- No set operators (`$in $nin`) — phase 2.2.
- No string operators (`$contains $startsWith $endsWith $containsi`) — phase 2.2.
- No `$or` / `$not` / nested combinators — phase 2.3.
- No filtering on relation fields (relations land in phase 2.4).
- No full-text search.
- No GROUP BY / aggregates.

---

## 2. Wire Format

Strapi-style nested brackets in the query string:

```
GET /api/post
  ?filters[title][$eq]=hello
  &filters[views][$ne]=0
  &filters[published][$null]=false
  &filters[deleted_at][$null]=true
  &page=1
  &pageSize=25
  &sort=created_at:desc
```

Rules:

- Each filter param matches the regex `^filters\[(?P<col>[^\[\]]+)\]\[(?P<op>\$[a-z]+)\]$`.
- Multiple params combine with implicit `AND`.
- Different ops on the same column allowed: `?filters[a][$eq]=1&filters[a][$ne]=5` →
  `"a" = 1 AND "a" <> 5`.
- Repeating the same `[col][op]` pair is rejected as ambiguous (422).
- Anything that doesn't match the regex but starts with `filters[` is rejected (422).
- Other query params (`page`, `pageSize`, `sort`) keep their v1 semantics unchanged.
- Backwards-compatible: requests without any `filters[...]` params behave exactly as
  v1 (parser returns `Filter::None`).

### 2.1 Value coercion

Per `(operator, field kind)`:

| Op       | Kind         | Wire value → bound type                                                                 |
|----------|--------------|-----------------------------------------------------------------------------------------|
| `$eq`    | string, text | string passthrough                                                                      |
| `$eq`    | integer      | `value.parse::<i64>()` — non-numeric → 422                                              |
| `$eq`    | float        | `value.parse::<f64>()` — non-numeric → 422                                              |
| `$eq`    | boolean      | `"true"` / `"false"` (case-insensitive) — anything else → 422                                              |
| `$eq`    | datetime     | RFC3339 → `DateTime<Utc>` — bad → 422                                                   |
| `$ne`    | (same as `$eq`) |                                                                                      |
| `$null`  | any kind     | `"true"` / `"false"` (case-insensitive) — anything else → 422                                              |

The literal string `null` as the value of `$eq` or `$ne` is rewritten to a typed
`Null` `BoundValue` and emitted as `IS NULL` / `IS NOT NULL` respectively. Clients
that want explicit null semantics SHOULD use `$null=true|false` for clarity, but the
shortcut is supported because URL encoding of literal `null` is common.

### 2.2 Whitelist

A column is filterable iff it is:

- a user field declared on the content type, **or**
- one of the system columns `id`, `created_at`, `updated_at`.

Identical to the sort whitelist already enforced in v1
`crates/http/src/query.rs::is_sortable`. The same helper SHOULD be reused.

### 2.3 Op-kind compatibility matrix

In this slice all three operators apply to all kinds, so no rejection on this axis.
The compatibility check is still implemented as a typed function so phase 2.2 can
populate it cheaply (e.g. `$gt` not valid on `boolean`, `$contains` not valid on
`integer`).

---

## 3. `rustapi-sql` Changes

### 3.1 `Filter` model

Extend `crates/sql/src/filter.rs` to add the AND-of-conditions variant and the
condition / operator / value types:

```rust
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Filter {
    #[default]
    None,
    All(Vec<Condition>),   // implicit AND; empty vec behaves like None
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Condition {
    pub column: String,    // already validated by ident regex upstream
    pub op: Op,
    pub value: FilterValue,
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
    /// Used by `$eq` / `$ne`.  When the inner `BoundValue` is `Null(kind)`,
    /// the SQL emitter rewrites to `IS NULL` / `IS NOT NULL`.
    Bound(rustapi_core::BoundValue),
    /// Used by `$null`: true = IS NULL, false = IS NOT NULL.
    Null(bool),
}
```

All public enums marked `#[non_exhaustive]` so phase 2.2/2.3 additions are
non-breaking.

### 3.2 WHERE emission

Add `fn render_where(filter: &Filter, start_placeholder: usize) -> (String, Vec<BoundValue>)`
to `crates/sql/src/dml.rs`. Returns the SQL fragment (including the leading ` WHERE `
or empty string) plus the binds in placeholder order. Callers pass
`start_placeholder = 1`; the helper increments per bound condition.

Per-condition emission:

| `(op, value)`                                  | SQL fragment           | Bind?                  |
|------------------------------------------------|------------------------|------------------------|
| `(Eq,  Bound(Null(_)))`                        | `"col" IS NULL`        | no                     |
| `(Ne,  Bound(Null(_)))`                        | `"col" IS NOT NULL`    | no                     |
| `(Eq,  Bound(v))`                              | `"col" = $N::<cast>`   | v                      |
| `(Ne,  Bound(v))`                              | `"col" <> $N::<cast>`  | v                      |
| `(IsNull, Null(true))`                         | `"col" IS NULL`        | no                     |
| `(IsNull, Null(false))`                        | `"col" IS NOT NULL`    | no                     |
| `(IsNull, Bound(_))` (invariant violation)     | builder returns `Err`  | —                      |
| `(Eq|Ne, Null(_))` (invariant violation)       | builder returns `Err`  | —                      |

`<cast>` is produced by reviving `pg_cast(kind)` (currently dead code) so
explicit-NULL binds remain typed. Both Postgres and the rustapi binder agree on the
type for every placeholder.

Identifiers go through `quote_ident` exactly once per condition. No `format!` of
raw column names.

### 3.3 `select_list` and `count`

Signatures unchanged. Behavior:

- `Filter::None` or `Filter::All(empty)` → emit no `WHERE` clause; `LIMIT $1 OFFSET $2`
  exactly as v1.
- `Filter::All(non-empty)` → emit `... WHERE <render_where> ORDER BY ... LIMIT $N OFFSET $N+1`.
  The pagination binds are appended after the filter binds.
- `count` reuses `render_where`; no LIMIT/OFFSET.

Bind order in the returned `Vec<BoundValue>`: filter binds first (positional `$1..$k`),
then `limit` (`$k+1`), then `offset` (`$k+2`). `count` returns only the filter binds.

### 3.4 Builder errors

`render_where` returns `Result<(String, Vec<BoundValue>), DmlError>`. New error
variants on `DmlError`:

- `InvalidFilter(&'static str)` — for the invariant violations in §3.2.

(Bad identifier still surfaces through `IdentError`.)

### 3.5 Unit tests (no DB)

Golden-string assertions on `select_list` / `count` / `render_where` outputs:

- `Filter::None` and `Filter::All(empty)` emit the same SQL as v1.
- One `$eq` on string → `WHERE "title" = $1::text` and the pagination shifts to `$2 $3`.
- One `$ne` on integer → `WHERE "views" <> $1::int8`.
- `$null=true` → `WHERE "x" IS NULL`, no binds added.
- `$null=false` → `WHERE "x" IS NOT NULL`.
- `$eq=null` (Bound::Null) → `WHERE "x" IS NULL`, no binds added.
- Two conds combined → `WHERE "a" = $1::int8 AND "b" <> $2::text`.
- `count` matches the same WHERE clause; no LIMIT/OFFSET.
- Bad identifier in `Condition.column` → `DmlError::Ident`.
- Invariant violations (`IsNull` with `Bound`, `Eq` with `Null(_)`) → `DmlError::InvalidFilter`.

---

## 4. `rustapi-http` Changes

### 4.1 New module: `rustapi-http::filter`

`crates/http/src/filter.rs`. Public entry point:

```rust
pub fn parse(
    raw_query: &str,
    ct: &rustapi_core::ContentType,
) -> Result<rustapi_sql::Filter, rustapi_core::Error>;
```

Behavior:

1. URL-decode the query string and split on `&`.
2. For each `k=v`, only consider `k.starts_with("filters[")`.
3. Match each key against `^filters\[(?P<col>[^\[\]]+)\]\[(?P<op>\$[a-z]+)\]$`. Mismatch
   → 422 `validation_failed` with message `malformed filter param`.
4. Verify `col` is in the whitelist (§2.2). Else 422 `unknown filter field <col>`.
5. Map `op` string → `Op` enum. Else 422 `unknown operator <op>`.
6. Reject duplicate `(col, op)` pairs → 422 `duplicate filter (col, op)`.
7. Op-kind compatibility check via a typed function (no rejections in this slice).
8. Coerce `v` per (op, kind) per §2.1. Coercion failure → 422 with field-level reason
   populated in `ValidationErrors::fields`.
9. Apply the `$eq`/`$ne` literal-null rewrite (§2.1).
10. Build `Vec<Condition>` and return `Filter::All`. If list is empty, return
    `Filter::None`.

### 4.2 Handler wiring

In `crates/http/src/routes/content.rs::list`:

- Add an `axum::extract::RawQuery` extractor alongside the existing
  `Query<ListParams>`. axum runs both extractors over the same query string
  independently: `Query<ListParams>` continues to extract `page`/`pageSize`/`sort`
  per v1, and `RawQuery` gives the new parser the full `?...` string so it can
  read bracketed keys that axum's `Query<HashMap<_, _>>` does not expose cleanly.
- Call `filter::parse(raw, &ct)?` after `parse_list`.
- Pass the resulting `Filter` to `rustapi_sql::select_list(..., &filter, ...)` and
  `rustapi_sql::count(..., &filter)`. Bind values are still walked by `bind_all` /
  `bind_all_as` in placeholder order — the existing helpers handle the longer
  argument vectors without change.
- `meta.total` is whatever `count(..., &filter)` returns.

### 4.3 Unit tests (parser only)

In `filter.rs`:

- Empty raw query → `Filter::None`.
- Single `$eq` on string → one `Condition` with `Op::Eq`.
- Unknown field → 422.
- Unknown operator → 422.
- Malformed bracket shape → 422.
- Bad integer coercion → 422 with field in `ValidationErrors.fields`.
- `$null=true` and `$null=false`.
- `$eq=null` rewrites to a `Bound(Null(kind))` condition.
- Duplicate `(col, op)` rejected.
- Non-filter params (`page`, `pageSize`, `sort`) ignored.

---

## 5. `rustapi-bin` Integration Tests

`crates/bin/tests/integration_filters.rs`. Setup: content type `post` with
`title: string (required)`, `views: integer`, `published: boolean`, `category: string`.
Insert 5 rows with varied values; explicitly null `views` on one row.

Tests:

- `eq_string_filter` — `?filters[title][$eq]=foo` returns only the matching row.
- `ne_integer_filter` — `?filters[views][$ne]=0` returns rows whose views ≠ 0
  (and does **not** return the NULL row, matching SQL `<>` semantics).
- `null_true_returns_nulls` — `?filters[views][$null]=true` returns only the row
  with NULL `views`.
- `null_false_returns_non_nulls` — `?filters[views][$null]=false` returns the
  non-null rows.
- `implicit_and_combines` — `?filters[category][$eq]=a&filters[published][$eq]=true`
  returns the intersection.
- `count_reflects_filter` — `meta.total` matches the filtered count, not the
  unfiltered count.
- `pagination_and_filter_compose` — `?filters[...]&page=1&pageSize=2` returns the
  first 2 of the filtered set; `meta.total` is the filtered total.
- `unknown_field_rejected_422`.
- `unknown_op_rejected_422`.
- `malformed_int_rejected_422`.
- `eq_null_rewrites_to_is_null` — `?filters[views][$eq]=null` returns the NULL row.
- `duplicate_col_op_rejected_422`.

All v1 integration tests must remain green; none of their request shapes change.

---

## 6. Errors

All errors continue to use the v1 JSON shape (`{error: {code, message, details?}}`).
Filter-specific errors use:

| Condition                               | Code                  | Status | `details.fields[0]` |
|-----------------------------------------|-----------------------|--------|---------------------|
| Malformed `filters[...]` bracket shape  | `validation_failed`   | 422    | — (message only)    |
| Unknown filter field                    | `validation_failed`   | 422    | `{field, reason}`   |
| Unknown operator                        | `validation_failed`   | 422    | `{field, reason}`   |
| Duplicate `(col, op)`                   | `validation_failed`   | 422    | `{field, reason}`   |
| Op-kind incompatibility (future)        | `validation_failed`   | 422    | `{field, reason}`   |
| Value coercion failure                  | `validation_failed`   | 422    | `{field, reason}`   |

`details.db` is unaffected by this slice — the DB cannot reject a well-formed
filter at parse time.

---

## 7. Out of Scope (for explicit reference in plan)

The following are deliberately not touched:

- v1 PUT semantics (already fixed post-review).
- DDL error mapping (already fixed post-review).
- `Action` granularity / RBAC.
- `TraceLayer` wiring / handler `#[instrument]`.
- `JsonRejection` → `ApiError` mapping.

---

## 8. Open Questions

None at sign-off.
