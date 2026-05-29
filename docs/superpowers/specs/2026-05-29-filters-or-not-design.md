# Phase 2.3 — `$or` / `$not` Combinators Design

**Date:** 2026-05-29
**Status:** Approved
**Depends on:** Phase 2.1 (`$eq`/`$ne`/`$null`), Phase 2.2 (order/set/string ops)
**Follows up:** Phase 2.4 (relations + `?populate=`)

## Goal

Add recursive logical combinators (`$or`, `$and`, `$not`) on top of the existing filter operator surface. Strapi-style bracketed URL syntax. Backwards-compatible with every phase 2.1 and 2.2 wire format.

## Wire Format

### Group combinators
```
?filters[$or][0][title][$eq]=foo
 &filters[$or][1][views][$gt]=10
```
→ `(title = 'foo') OR (views > 10)`

```
?filters[$and][0][category][$eq]=tech
 &filters[$and][1][published][$eq]=true
```
→ `(category = 'tech') AND (published = true)` — explicit `$and` is allowed but redundant at top level.

### Negation
```
?filters[$not][title][$contains]=draft
```
→ `NOT (title LIKE '%draft%' ESCAPE '\')`

`$not` is **strictly unary**. It wraps exactly one child (leaf OR group). Array form (`filters[$not][0]...`) is rejected `422`.

### Nesting
```
?filters[$or][0][$and][0][category][$eq]=tech
 &filters[$or][0][$and][1][views][$gt]=10
 &filters[$or][1][$not][title][$contains]=draft
```
→ `((category = 'tech') AND (views > 10)) OR (NOT (title LIKE '%draft%' ESCAPE '\'))`

Depth and leaf-count caps apply (see Validation).

### Mixed with implicit AND
```
?filters[published][$eq]=true
 &filters[$or][0][category][$eq]=tech
 &filters[$or][1][category][$eq]=design
 &sort=created_at:desc
 &page=1&pageSize=20
```
→ `published = true AND ((category = 'tech') OR (category = 'design'))`

Top-level keys without a combinator prefix are joined by implicit AND, exactly as in phase 2.1/2.2. Combinators sit alongside them as additional AND-joined nodes.

## Filter Type

```rust
// crates/sql/src/filter.rs

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum Filter {
    #[default]
    None,
    All(Vec<Filter>),
    Any(Vec<Filter>),
    Not(Box<Filter>),
    Leaf(Condition),
}
```

`Condition` is unchanged from phase 2.2 (`column`, `kind`, `op`, `value`).

### Migration from 2.2

Phase 2.2 had `Filter::All(Vec<Condition>)`. After this change, each `Condition` is wrapped in `Filter::Leaf`. Behavior identical; the wire format and SQL output for every existing query stay byte-for-byte the same.

## Parser

`crates/http/src/filter.rs` extension.

### Tokenization

Each `filters[...]...` key is split into bracketed segments. Each segment is one of:
- A combinator token: `$or`, `$and`, `$not`
- A numeric index: `0`, `1`, `2`, ... (only valid inside a group)
- A column name: validated against the content type's column set
- An op token: `$eq`, `$ne`, `$null`, `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$nin`, `$contains`, `$startsWith`, `$endsWith`, `$containsi`
- A `$in`/`$nin` value-list index: `0`, `1`, ... (terminal segment)

### Tree construction

Top-down walk per request:

1. Start with an empty root group node. All top-level keys parse into it.
2. For each query key:
   - If first segment is a column name → parse as leaf, append to root.
   - If first segment is `$or` / `$and` → step into (or create) the corresponding group node at root; recurse on remaining segments.
   - If first segment is `$not` → step into (or create) the unary `$not` node at root; recurse on remaining segments.
3. Recursion at a group node consumes one index segment, then either another combinator or a column-name leaf.
4. Recursion at a `$not` node consumes either a combinator or a column-name leaf (no index).

After all keys are consumed, the root becomes:
- `Filter::None` if zero conditions parsed
- `Filter::All(vec![only_child])` collapsed to `only_child` if exactly one
- `Filter::All(vec![...])` otherwise

Group nodes with `$and` also use `Filter::All`; `$or` uses `Filter::Any`; `$not` uses `Filter::Not`.

### Validation

All validation happens at parse time. SQL emitter trusts the tree.

| Rule | Error message | HTTP |
|---|---|---|
| Depth > 8 segments | `filter nesting depth exceeds 8` | 422 |
| Total leaves > 100 | `filter leaf count exceeds 100` | 422 |
| Empty group (`$or`/`$and` with no children) | `empty $or/$and group` | 422 |
| Index gap in a group | `gap in $or/$and indices` | 422 |
| Duplicate index in a group | `duplicate index in $or/$and group` | 422 |
| `$not` with array form | `$not must be unary` | 422 |
| `$not` empty | `$not requires a child` | 422 |
| Unknown combinator token | `unknown combinator` | 422 |
| Leaf-level validation (kind mismatch, etc.) | unchanged from phase 2.1/2.2 | 422 |

**Depth counting:** depth = number of group/`$not` segments traversed before reaching a leaf's column segment. A bare top-level leaf (`filters[col][$eq]=v`) is depth 0. `filters[$or][0][col][$eq]=v` is depth 1. The cap is 8, so the deepest legal leaf has 8 combinator segments above its column.

**Leaf-count counting:** total `Filter::Leaf` nodes in the parsed tree. Cap = 100. Matches the existing `$in`/`$nin` value cap.

## SQL Emitter

`crates/sql/src/dml.rs` extension. `render_where` becomes recursive.

```rust
fn render_node(node: &Filter, buf: &mut String, binds: &mut Vec<BoundValue>) -> Result<()> {
    match node {
        Filter::None => Ok(()),
        Filter::Leaf(c) => render_leaf(c, buf, binds), // existing per-op path
        Filter::All(xs) if xs.len() == 1 => render_node(&xs[0], buf, binds),
        Filter::All(xs) => render_joined(xs, " AND ", buf, binds),
        Filter::Any(xs) if xs.len() == 1 => render_node(&xs[0], buf, binds),
        Filter::Any(xs) => render_joined(xs, " OR ", buf, binds),
        Filter::Not(x) => {
            buf.push_str("NOT (");
            render_node(x, buf, binds)?;
            buf.push(')');
            Ok(())
        }
    }
}
```

`render_joined` wraps each child in `(...)` and joins with the separator.

**Defensive guard:** `Filter::All(vec![])` and `Filter::Any(vec![])` should never reach the emitter (parser rejects). If they somehow do, emitter returns `DmlError::InvalidFilter("empty group reached emitter")`. Treated as an internal invariant violation, not user error.

**Single-child elision:** `All([x])` and `Any([x])` render as bare `x` — no redundant parens. This keeps phase 2.1/2.2 output identical: a single condition emits the same SQL it did before.

**Paren placement:** every group child is wrapped in `(...)` to make precedence explicit and avoid relying on SQL operator precedence rules. Leaves emit their existing form inside those parens.

## NULL Semantics

`NOT (col = v)` follows Postgres 3VL: NULL rows are excluded from the result of `NOT (...)`, because `col = v` is `NULL` for NULL `col`, and `NOT NULL = NULL`, which fails the `WHERE` predicate.

This is documented behavior, not a bug. Users who want "not equal OR null" must spell it out:
```
?filters[$or][0][col][$ne]=v&filters[$or][1][col][$null]=true
```

No magic rewriting. The contract is: `$not` is logical NOT; NULL handling is whatever Postgres does. Same as Strapi.

## Back-Compatibility

- Every phase 2.1 / 2.2 wire format parses identically.
- Output SQL for those queries is byte-for-byte identical (single-child elision in `Filter::All` collapses the wrap).
- Existing integration tests (`integration_filters.rs`, `integration_filters_2_2.rs`) pass without modification.

## Tests

### `crates/sql/src/filter.rs` unit
- `Filter::default()` is `Filter::None` (unchanged)
- `All` / `Any` / `Not` variant constructors (smoke)
- (No `op_allows_kind` changes — leaves still use existing per-kind matrix)

### `crates/sql/src/dml.rs` unit
- `All([leaf, leaf])` → `(a) AND (b)`
- `Any([leaf, leaf])` → `(a) OR (b)`
- `Not(leaf)` → `NOT (a)`
- `All([leaf])` → `a` (no parens)
- `Any([Not(leaf)])` → `NOT (a)` (single-child elision through `Not`)
- Nested: `All([Any([l1, l2]), Not(l3)])` → `((a) OR (b)) AND (NOT (c))`
- Bind ordering matches placeholder ordering across nested groups
- Empty group invariant guard returns `InvalidFilter`

### `crates/http/src/filter.rs` unit
- Top-level `$or` two children → `Filter::All([Filter::Any([Leaf, Leaf])])`
- Top-level `$and` two children → `Filter::All([Filter::All([Leaf, Leaf])])` (explicit `$and` doesn't auto-flatten)
- `$not` wrapping a leaf
- `$not` wrapping a group
- Mixed: top-level leaf + `$or` group
- Depth-cap rejection at 9 levels deep
- Leaf-cap rejection at 101 leaves
- Gap rejection: `$or[0]` + `$or[2]`
- Duplicate-index rejection: `$or[0]` twice
- `$not` array form rejection
- `$not` empty rejection
- Empty `$or` group rejection
- `op_allows_kind` violation inside a group still rejects
- `$in` inside `$or` works (value-list bracket index parses correctly)

### `crates/bin/tests/integration_filters_2_3.rs` (new file)
Real Postgres via testcontainers. Seeds a representative dataset and exercises:
- `$or` of two `$eq` clauses
- `$or` mixing different ops (`$eq` and `$gt`)
- `$not` of a leaf
- `$not` of an `$or` group
- Nested `$or` inside `$and` inside top-level
- `$not` excludes nulls (NULL 3VL spot-check)
- Mixed implicit-AND top-level + `$or` group
- Depth cap rejection (422)
- Leaf cap rejection (422)
- Empty `$or` rejection (422)
- Combinator + pagination + sort all compose

Workspace test count target: ~210 tests after this phase (188 baseline + ~22 new).

## Out of Scope

- `$nor`, `$xor`, `$any`, `$all` — not Strapi-standard, can add later if needed
- Relations / `?populate=` — phase 2.4
- Filter on related fields (`filters[author][name][$eq]=...`) — phase 2.4
- Full-text search operators (`$search`, `$matches`) — separate phase
- Query plan introspection / EXPLAIN passthrough — not a CMS surface concern

## Risk / Decisions Log

- **Recursive `Filter` enum (option A) chosen over flat groups (option B).** Same upfront effort; option B would block real nesting and require a rewrite later.
- **Depth cap = 8.** Generous for any realistic query, well below stack-overflow risk for the recursive parser/emitter.
- **Leaf cap = 100.** Same number as `$in` value cap — one mental model, one error knob.
- **Strict 422 on empty groups, gaps, dups, `$not` arity.** Consistent with phase 2.2 `$in` strictness; loud failure beats silent surprise.
- **`$not` unary only.** Matches Strapi semantics. Users wanting `NOT(a AND b)` write `$not` around `$and` explicitly.
- **No NULL rewrite for `$not`.** Postgres 3VL is documented; rewriting would be magic and hide intent.
- **Single-child group elision in emitter.** Preserves byte-identical SQL for phase 2.1/2.2 queries — critical for back-compat test stability.
