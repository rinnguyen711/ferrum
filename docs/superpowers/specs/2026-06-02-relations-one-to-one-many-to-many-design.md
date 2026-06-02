# Relations: one-to-one + many-to-many — design

**Date:** 2026-06-02
**Phase:** 2 (close-out of the relations work)
**Status:** approved, pre-implementation

## 1. Goal

Finish the Phase 2 relations surface. v1 and prior Phase 2 work shipped
`many_to_one` relations (FK column `<field>_id` on the owning type, optional
`inverse` name for reverse populate). This adds the two remaining
cardinalities from the roadmap:

- **one_to_one** — owning-side FK with a `UNIQUE` constraint.
- **many_to_many** — auto-managed join table, replace-set writes, batched
  populate.

`many_to_one` already covers the **one-to-many** read shape (many children →
one parent; `inverse` reads the "one" side's children), so no separate
one-to-many work is needed.

## 2. Non-goals (explicit, this phase)

- Filtering on many-to-many fields (no row column to filter; deferred).
- Nested / multi-level populate (`?populate=author.posts`).
- Pagination of M:N links beyond the existing per-parent cap (25).
- User-supplied explicit join-table names.
- Ordering of M:N links.

## 3. Data model & validation (`crates/core/src/field.rs`)

### 3.1 Cardinality becomes a typed enum

`RelationMeta.cardinality` changes from `String` to:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    ManyToOne,
    OneToOne,
    ManyToMany,
}
```

`RelationMeta::from_value` parses the string against this enum; unknown values
→ `FieldError::BadCardinality`. The previous hard `== "many_to_one"` check is
removed.

### 3.2 Storage by cardinality

| cardinality    | physical storage         | row column   | unique |
|----------------|--------------------------|--------------|--------|
| `many_to_one`  | FK on owner row (exists) | `<field>_id` | no     |
| `one_to_one`   | FK on owner row          | `<field>_id` | **yes**|
| `many_to_many` | **join table**           | none         | n/a    |

### 3.3 Validation rules

- `many_to_one`: unchanged.
- `one_to_one`: same as `many_to_one`; the UNIQUE constraint is applied in DDL,
  not validation. `inverse` optional → reverse is a single object.
- `many_to_many`: reject `required` (`FieldError::ManyToManyCannotBeRequired`),
  reject `unique` (reuse `RelationFieldUniqueUnsupported`), reject non-null
  `default` (reuse `RelationFieldDefaultUnsupported`). `inverse` optional. No
  FK column.

### 3.4 Field helpers

- `Field::physical_column()` — unchanged for `many_to_one`/`one_to_one`
  (`<field>_id`). For `many_to_many` it must **not** be treated as a stored
  column.
- New `Field::is_stored_column() -> bool` — `false` for `many_to_many` relation
  fields, `true` otherwise. All INSERT/SELECT/UPDATE column-list builders and
  the write path filter on this.

### 3.5 New error variants

- `FieldError::ManyToManyCannotBeRequired`
- (reuse) `RelationFieldUniqueUnsupported`, `RelationFieldDefaultUnsupported`,
  `BadCardinality`.

## 4. DDL & schema lifecycle (`crates/sql/src/ddl.rs`, `ident.rs`, `crates/schema/src/service.rs`)

### 4.1 one_to_one

When emitting the owner's `CREATE TABLE` / `ADD COLUMN` for a `one_to_one`
field, add a UNIQUE constraint on `<field>_id` (unique index or inline
`UNIQUE`). FK clause otherwise identical to `many_to_one`
(`REFERENCES ct_<target>("id") ON DELETE RESTRICT`).

### 4.2 many_to_many join table

**Name:** `j_<owner>_<field>`, prefix `j_` to stay out of the `ct_`
content-type namespace. Keyed on owner + field, so distinct per declaring
field.

**Length:** when `j_<owner>_<field>` exceeds the Postgres 63-char identifier
limit, fall back to a deterministic hashed name: truncate the readable part and
append a short hex hash of the full logical name, e.g.
`j_<trunc>_<6hexhash>`. New helper in `ident.rs` (`join_table_name(owner,
field) -> Result<String, IdentError>`) is the single source of truth; all
callers go through it.

**Columns:**

```sql
CREATE TABLE "j_<owner>_<field>" (
  "<owner>_id"  uuid NOT NULL REFERENCES "ct_<owner>"("id")  ON DELETE CASCADE,
  "<target>_id" uuid NOT NULL REFERENCES "ct_<target>"("id") ON DELETE CASCADE,
  PRIMARY KEY ("<owner>_id", "<target>_id")
);
CREATE INDEX ON "j_<owner>_<field>" ("<target>_id");
```

PK `(owner_id, target_id)` dedups links and serves owner→target lookups; the
index serves target→owner (inverse) populate. FKs use `ON DELETE CASCADE` so
deleting either linked entry removes the join rows and the entry delete
succeeds. (Direct FK columns keep `ON DELETE RESTRICT`; join links are derived
data, so the divergence is intentional.)

### 4.3 SchemaService lifecycle

- **Create type** with M:N field(s) → create the main table, then each join
  table.
- **PATCH type** adding an M:N field → create its join table; dropping an M:N
  field → drop its join table.
- **Delete type** → drop all join tables this type *owns*, and all join tables
  that *target* this type, **before** dropping `ct_<type>` (FK ordering). The
  registry gains a reverse lookup "join tables targeting X" (mirrors the
  existing `inverse_lookup`).

## 5. Write path (entry create / PATCH)

`many_to_one` / `one_to_one` writes are unchanged — a single `<field>_id`
column value in the INSERT/UPDATE. `one_to_one` duplicate FK surfaces as the
existing PG unique-violation → 409 mapping.

### 5.1 many_to_many — replace-set, transactional

Request body sends `field: ["uuid1", "uuid2"]`.

- **POST /content/<type>**: insert the row (stored columns only), then insert
  join rows `(new_id, target_id)` for each M:N field. Row + all links in **one
  transaction**.
- **PATCH /content/<type>/<id>**: for each M:N field present in the body,
  replace-set: `DELETE FROM j_... WHERE <owner>_id = $1`, then insert the
  supplied IDs. Field **absent** from body → links untouched. Field present as
  `[]` → all links removed. One transaction.

### 5.2 Target validation (pre-check)

Before inserting links, dedup the input array and run one
`SELECT id FROM ct_<target> WHERE id = ANY($1)`. If the returned count is less
than the distinct input count, return **422** naming the missing id(s)
(`relation target not found`). Matches the explicit-validation style of the
codebase and gives a precise error rather than a raw PG FK violation.

### 5.3 New SQL helpers (`crates/sql/src/dml.rs`)

- `insert_links(join_table, owner_col, target_col, owner_id, target_ids)` —
  single multi-row insert via `UNNEST($2::uuid[])`.
- `delete_links(join_table, owner_col, owner_id)`.

The write handler splits incoming fields into stored columns (existing path)
and M:N fields (link path), both inside one `tx`.

## 6. Read / populate (`crates/http/src/populate.rs`)

### 6.1 Unpopulated reads

M:N fields are **omitted** from the row JSON unless requested via
`?populate=<field>`. No column, no extra query on the common path. Consistent
with how inverse relations already behave (they too appear only when
populated).

### 6.2 PopulateField variants

- `one_to_one` forward → existing `Forward` (FK col → single object), unchanged.
- `one_to_one` **inverse** → new `InverseOne { field_name, source, fk_col }`:
  same SELECT as `Inverse`, but returns a single object (or `null`), no cap (FK
  is UNIQUE so ≤1 child).
- `many_to_many` (owner→targets *and* inverse target→owners) → new
  `Many { field_name, join_table, self_col, other_col, target }`.

### 6.3 apply_many

1. Collect parent ids from `rows`.
2. One batched SELECT:
   ```sql
   SELECT j."<self_col>" AS parent, t.*
   FROM "j_..." j
   JOIN "ct_<target>" t ON t."id" = j."<other_col>"
   WHERE j."<self_col>" = ANY($1)
   ORDER BY j."<self_col>", t."id"
   LIMIT (INVERSE_LIMIT_PER_PARENT + 1) * N
   ```
3. Reuse the existing `group_inverse_children` (cap + truncation aware) → array
   per parent under `field_name`, plus `<field>_truncated: true` when a parent
   crosses the cap of 25. Parents with no links get `[]`.

`parse_populate` is extended to emit `Many` for M:N relation fields, and
`registry.inverse_lookup` is extended to resolve M:N inverse names (returning a
`Many` with `self_col`/`other_col` swapped).

## 7. Filtering

M:N fields are **not filterable** this phase (no row column). `many_to_one` /
`one_to_one` FK columns remain filterable by id exactly as today. Documented
limitation.

## 8. Error taxonomy summary

| condition                          | status | source                              |
|------------------------------------|--------|-------------------------------------|
| M:N field declared `required`      | 422    | `ManyToManyCannotBeRequired`        |
| relation field `unique`/`default`  | 422    | existing relation field errors      |
| unknown cardinality string         | 422    | `BadCardinality`                    |
| M:N write references missing id    | 422    | pre-check, `relation target not found` |
| one_to_one duplicate FK            | 409    | existing PG unique-violation mapping|

## 9. Testing

Mirrors the existing per-crate unit + `crates/bin/tests/relations.rs`
integration style.

- **core**: `Cardinality` parse (all three + unknown); M:N validation
  rejections (required / unique / default); 1:1 validation passes;
  `is_stored_column()` matrix.
- **sql**: join-table `CREATE TABLE` + index string; `join_table_name`
  hash-suffix fallback when too long; `insert_links` / `delete_links` SQL;
  one_to_one UNIQUE in column def.
- **http**: `parse_populate` resolves M:N → `Many` and 1:1 inverse →
  `InverseOne`; `apply_many` grouping (leans on already-tested
  `group_inverse_children`).
- **bin integration (testcontainers)**: create types with 1:1 + M:N; POST with
  link arrays; PATCH replace-set (add / remove / `[]`); populate forward +
  inverse + M:N; cascade removal of links on entry delete; join table dropped
  on field/type delete; 1:1 unique violation → 409; bad target id → 422.

## 10. Implementation shape

Single plan, ~6–8 sequential bottom-up tasks (core → sql → schema service →
http write → http populate → integration), each TDD + commit, matching the
existing `docs/superpowers/plans` format. 1:1 and M:N share the
cardinality-enum spine, so they ship together rather than as two plans.

## 11. Open questions

None at sign-off.
