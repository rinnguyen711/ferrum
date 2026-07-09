# Relations: one_to_one + many_to_many Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `one_to_one` (FK + UNIQUE) and `many_to_many` (auto join table, replace-set writes, batched populate) relation cardinalities, closing out Phase 2 relations.

**Architecture:** Promote `RelationMeta.cardinality` to a typed `Cardinality` enum. `one_to_one` reuses the existing `<field>_id` FK column path plus a UNIQUE constraint. `many_to_many` introduces an auto-named join table (`j_<owner>_<field>`, hash-suffixed when too long), managed by `SchemaService` across create/patch/delete, written transactionally with replace-set semantics, and read via a new batched `apply_many` populate pass that reuses the existing per-parent cap machinery.

**Tech Stack:** Rust, axum, sqlx (Postgres), testcontainers. Workspace crates: `ferrum-core`, `ferrum-sql`, `ferrum-schema`, `ferrum-http`, `ferrum` (bin).

**Spec:** [docs/superpowers/specs/2026-06-02-relations-one-to-one-many-to-many-design.md](../specs/2026-06-02-relations-one-to-one-many-to-many-design.md)

---

## File Structure

- `crates/core/src/field.rs` — `Cardinality` enum; `RelationMeta` uses it; M:N validation; `is_stored_column()`. (Task 1, 2)
- `crates/sql/src/ident.rs` — `join_table_name(owner, field)` with hash-suffix fallback. (Task 3)
- `crates/sql/src/ddl.rs` — one_to_one UNIQUE; `create_join_table`, `drop_join_table`. (Task 4, 5)
- `crates/sql/src/dml.rs` — `insert_links`, `delete_links`. (Task 6)
- `crates/sql/src/lib.rs` — re-exports for the new sql fns. (Task 3, 4, 5, 6)
- `crates/schema/src/registry.rs` — `m2m_targets(type)` reverse lookup; `inverse_lookup` returns M:N inverse. (Task 7)
- `crates/schema/src/service.rs` — join-table lifecycle on create/patch/delete. (Task 8)
- `crates/http/src/entry.rs` — split M:N field values out of `body_to_binds` into a link plan. (Task 9)
- `crates/http/src/routes/content.rs` — transactional write of row + links; M:N populate dispatch. (Task 10, 12)
- `crates/http/src/populate.rs` — `PopulateField::Many` + `InverseOne`; `apply_many`; parse + inverse resolution. (Task 11)
- `crates/bin/tests/relations_m2m.rs` — integration coverage. (Task 13)

---

## Task 1: `Cardinality` enum in core

**Files:**
- Modify: `crates/core/src/field.rs`

- [ ] **Step 1: Write the failing test**

Add to the `relation_meta_tests` module in `crates/core/src/field.rs`:

```rust
    #[test]
    fn cardinality_parses_all_three() {
        for (s, expected) in [
            ("many_to_one", Cardinality::ManyToOne),
            ("one_to_one", Cardinality::OneToOne),
            ("many_to_many", Cardinality::ManyToMany),
        ] {
            let m = RelationMeta::from_value(&json!({"target":"user","cardinality":s})).unwrap();
            assert_eq!(m.cardinality, expected);
        }
    }

    #[test]
    fn cardinality_rejects_unknown() {
        assert_eq!(
            RelationMeta::from_value(&json!({"target":"user","cardinality":"nonsense"}))
                .unwrap_err(),
            FieldError::BadCardinality
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ferrum-core cardinality_parses_all_three`
Expected: FAIL — `Cardinality` not found; `m.cardinality` is a `String`.

- [ ] **Step 3: Add the enum and switch `RelationMeta`**

In `crates/core/src/field.rs`, add the enum (near `RelationMeta`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    ManyToOne,
    OneToOne,
    ManyToMany,
}

impl Cardinality {
    fn parse(s: &str) -> Result<Self, FieldError> {
        match s {
            "many_to_one" => Ok(Cardinality::ManyToOne),
            "one_to_one" => Ok(Cardinality::OneToOne),
            "many_to_many" => Ok(Cardinality::ManyToMany),
            _ => Err(FieldError::BadCardinality),
        }
    }
}
```

Change the `RelationMeta` struct field type:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RelationMeta {
    pub target: String,
    pub cardinality: Cardinality,
    pub inverse: Option<String>,
}
```

In `RelationMeta::from_value`, replace the cardinality block:

```rust
        let cardinality_str = obj
            .get("cardinality")
            .and_then(|x| x.as_str())
            .ok_or(FieldError::RelationMetaShape)?;
        let cardinality = Cardinality::parse(cardinality_str)?;
```

(Delete the old `if cardinality != "many_to_one" { return Err(FieldError::BadCardinality); }` and the `.to_string()`.)

- [ ] **Step 4: Fix the existing cardinality test that asserted the old behavior**

The existing `reject_bad_cardinality` test asserts `one_to_many` and `many_to_many` are rejected. `many_to_many` is now valid. Edit it to:

```rust
    #[test]
    fn reject_bad_cardinality() {
        assert_eq!(
            RelationMeta::from_value(&json!({"target":"user","cardinality":"one_to_many"}))
                .unwrap_err(),
            FieldError::BadCardinality
        );
        assert_eq!(
            RelationMeta::from_value(&json!({"target":"user","cardinality":"nonsense"}))
                .unwrap_err(),
            FieldError::BadCardinality
        );
    }
```

Also update `parse_minimal_meta` / `parse_with_inverse` assertions that compare
`m.cardinality` to the string `"many_to_one"`: change to
`assert_eq!(m.cardinality, Cardinality::ManyToOne);`.

- [ ] **Step 5: Run the whole core crate to catch every string-compare site**

Run: `cargo test -p ferrum-core`
Expected: compile errors point to any remaining `cardinality == "..."` or
`cardinality: "...".into()` usages (e.g. in `dml.rs`/`ddl.rs`/`service.rs` only
*read* via `relation_meta()`, so core should now compile + pass). Fix any
remaining core-internal references until green.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/field.rs
git commit -m "feat(core): Cardinality enum (many_to_one/one_to_one/many_to_many)"
```

---

## Task 2: M:N field validation + `is_stored_column`

**Files:**
- Modify: `crates/core/src/field.rs`

- [ ] **Step 1: Write the failing tests**

Add to `relation_meta_tests`:

```rust
    #[test]
    fn many_to_many_rejects_required() {
        let f = Field {
            name: "tags".into(),
            kind: FieldKind::Relation,
            required: true,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"target":"tag","cardinality":"many_to_many"}),
        };
        assert_eq!(f.validate().unwrap_err(), FieldError::ManyToManyCannotBeRequired);
    }

    #[test]
    fn many_to_many_basic_ok() {
        let f = Field {
            name: "tags".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"target":"tag","cardinality":"many_to_many"}),
        };
        assert!(f.validate().is_ok());
    }

    #[test]
    fn one_to_one_basic_ok() {
        let f = Field {
            name: "profile".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"target":"profile","cardinality":"one_to_one"}),
        };
        assert!(f.validate().is_ok());
    }

    #[test]
    fn is_stored_column_matrix() {
        let mk = |card: &str| Field {
            name: "r".into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"target":"x","cardinality":card}),
        };
        assert!(mk("many_to_one").is_stored_column());
        assert!(mk("one_to_one").is_stored_column());
        assert!(!mk("many_to_many").is_stored_column());
        // Non-relation always stored.
        let s = Field { name: "t".into(), kind: FieldKind::String, required: false, unique: false, default: serde_json::Value::Null, max_length: None, kind_meta: json!({}) };
        assert!(s.is_stored_column());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ferrum-core many_to_many_rejects_required is_stored_column_matrix`
Expected: FAIL — `ManyToManyCannotBeRequired` and `is_stored_column` undefined.

- [ ] **Step 3: Add the error variant**

In the `FieldError` enum in `crates/core/src/field.rs`, add:

```rust
    #[error("many_to_many relation field cannot be required")]
    ManyToManyCannotBeRequired,
```

- [ ] **Step 4: Add the M:N validation branch**

In `Field::validate`, inside the existing `if self.kind == FieldKind::Relation {`
block, before `let _ = RelationMeta::from_value(...)`, add a cardinality-aware
check. Replace the relation block body with:

```rust
        if self.kind == FieldKind::Relation {
            if self.unique {
                return Err(FieldError::RelationFieldUniqueUnsupported);
            }
            if !self.default.is_null() {
                return Err(FieldError::RelationFieldDefaultUnsupported);
            }
            let meta = RelationMeta::from_value(&self.kind_meta)?;
            if meta.cardinality == Cardinality::ManyToMany && self.required {
                return Err(FieldError::ManyToManyCannotBeRequired);
            }
            return Ok(());
        }
```

- [ ] **Step 5: Add `is_stored_column`**

In `impl Field`, after `physical_column`:

```rust
    /// Whether this field maps to a physical column on the type's own table.
    /// Many-to-many relations live in a join table and have no row column.
    pub fn is_stored_column(&self) -> bool {
        if self.kind == FieldKind::Relation {
            return self
                .relation_meta()
                .map(|m| m.cardinality != Cardinality::ManyToMany)
                .unwrap_or(true);
        }
        true
    }
```

- [ ] **Step 6: Run tests to verify pass**

Run: `cargo test -p ferrum-core`
Expected: PASS (all core tests).

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/field.rs
git commit -m "feat(core): m2m validation (no required) + Field::is_stored_column"
```

---

## Task 3: `join_table_name` with hash-suffix fallback

**Files:**
- Modify: `crates/sql/src/ident.rs`
- Modify: `crates/sql/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/sql/src/ident.rs`:

```rust
    #[test]
    fn join_table_short_name_readable() {
        assert_eq!(join_table_name("post", "tags").unwrap(), "\"j_post_tags\"");
    }

    #[test]
    fn join_table_long_name_hashed_and_under_limit() {
        let owner = "a".repeat(40);
        let field = "b".repeat(40);
        let q = join_table_name(&owner, &field).unwrap();
        // Strip the surrounding quotes to measure the raw identifier length.
        let raw = q.trim_matches('"');
        assert!(raw.len() <= 63, "ident too long: {} ({})", raw.len(), raw);
        assert!(raw.starts_with("j_"));
        // Deterministic: same inputs → same name.
        assert_eq!(join_table_name(&owner, &field).unwrap(), q);
    }

    #[test]
    fn join_table_rejects_bad_idents() {
        assert!(join_table_name("Bad", "tags").is_err());
        assert!(join_table_name("post", "Bad Field").is_err());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ferrum-sql join_table`
Expected: FAIL — `join_table_name` not found.

- [ ] **Step 3: Implement `join_table_name`**

Add to `crates/sql/src/ident.rs` (after `table_name`):

```rust
/// Deterministic join-table name for a many-to-many relation declared on
/// `owner.<field>`. Normally `j_<owner>_<field>`. When that would exceed the
/// Postgres 63-char identifier limit, truncate the readable part and append a
/// short hash of the full logical name so the result stays unique and stable.
pub fn join_table_name(owner: &str, field: &str) -> Result<String, IdentError> {
    if !is_valid_ident(owner) {
        return Err(IdentError(owner.to_string()));
    }
    if !is_valid_ident(field) {
        return Err(IdentError(field.to_string()));
    }
    let readable = format!("j_{owner}_{field}");
    if readable.len() <= 63 {
        return quote_ident(&readable);
    }
    // Hash the full logical name; keep 8 hex chars. Truncate the readable head
    // to leave room for "_<8hex>" within 63.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    readable.hash(&mut hasher);
    let hash = format!("{:08x}", (hasher.finish() & 0xffff_ffff) as u32);
    let head_budget = 63 - 1 - hash.len(); // 1 for the underscore separator
    let head: String = readable.chars().take(head_budget).collect();
    quote_ident(&format!("{head}_{hash}"))
}
```

Note: `quote_ident` calls `is_valid_ident`, which enforces `^[a-z][a-z0-9_]{0,62}$`.
The hashed name is all lowercase hex + `_` + the already-validated head, so it
passes. `DefaultHasher` is stable within a single binary build, which is
sufficient — the name is recomputed from the registry on every DDL op, never
persisted independently.

- [ ] **Step 4: Re-export from lib**

In `crates/sql/src/lib.rs`, extend the ident re-export line:

```rust
pub use ident::{join_table_name, quote_ident, table_name, IdentError};
```

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test -p ferrum-sql join_table`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/sql/src/ident.rs crates/sql/src/lib.rs
git commit -m "feat(sql): join_table_name with hash-suffix fallback"
```

---

## Task 4: one_to_one UNIQUE in DDL

**Files:**
- Modify: `crates/sql/src/ddl.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/sql/src/ddl.rs`:

```rust
    #[test]
    fn create_table_one_to_one_emits_unique_fk() {
        let mut f = field("profile", FieldKind::Relation);
        f.kind_meta = json!({"target":"profile","cardinality":"one_to_one"});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(
            sql.contains("\"profile_id\" uuid UNIQUE REFERENCES \"ct_profile\"(\"id\") ON DELETE RESTRICT"),
            "got: {sql}"
        );
    }

    #[test]
    fn many_to_one_still_has_no_unique() {
        let mut f = field("author", FieldKind::Relation);
        f.kind_meta = json!({"target":"user","cardinality":"many_to_one"});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(!sql.contains("\"author_id\" uuid UNIQUE"), "got: {sql}");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ferrum-sql one_to_one`
Expected: FAIL — no `UNIQUE` emitted for the FK.

- [ ] **Step 3: Add UNIQUE in `relation_column_def`**

In `crates/sql/src/ddl.rs`, change `relation_column_def`:

```rust
fn relation_column_def(f: &Field) -> Result<String, DdlError> {
    use ferrum_core::Cardinality;
    let meta = f.relation_meta().ok_or_else(|| {
        IdentError("relation field missing/invalid kind_meta".into())
    })?;
    let col = quote_ident(&f.physical_column())?;
    let target = table_name(&meta.target)?;
    let not_null = if f.required { " NOT NULL" } else { "" };
    let unique = if meta.cardinality == Cardinality::OneToOne { " UNIQUE" } else { "" };
    Ok(format!(
        "{col} uuid{not_null}{unique} REFERENCES {target}(\"id\") ON DELETE RESTRICT"
    ))
}
```

(`many_to_many` never reaches `column_def` — see Task 5, where `create_table`
skips non-stored columns.)

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p ferrum-sql`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/ddl.rs
git commit -m "feat(sql): one_to_one relation FK emits UNIQUE"
```

---

## Task 5: join-table DDL + skip M:N columns in create_table

**Files:**
- Modify: `crates/sql/src/ddl.rs`
- Modify: `crates/sql/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/sql/src/ddl.rs`:

```rust
    #[test]
    fn create_join_table_emits_table_and_index() {
        let (create, index) =
            create_join_table("post", "tags", "tag").unwrap();
        assert_eq!(
            create,
            "CREATE TABLE \"j_post_tags\" (\
\"post_id\" uuid NOT NULL REFERENCES \"ct_post\"(\"id\") ON DELETE CASCADE, \
\"tag_id\" uuid NOT NULL REFERENCES \"ct_tag\"(\"id\") ON DELETE CASCADE, \
PRIMARY KEY (\"post_id\", \"tag_id\"))"
        );
        assert_eq!(
            index,
            "CREATE INDEX ON \"j_post_tags\" (\"tag_id\")"
        );
    }

    #[test]
    fn drop_join_table_works() {
        assert_eq!(drop_join_table("post", "tags").unwrap(), "DROP TABLE \"j_post_tags\"");
    }

    #[test]
    fn create_table_skips_many_to_many_columns() {
        let mut f = field("tags", FieldKind::Relation);
        f.kind_meta = json!({"target":"tag","cardinality":"many_to_many"});
        let sql = create_table(&ct(vec![f])).unwrap();
        // No tags column on the row table at all.
        assert!(!sql.contains("tags"), "got: {sql}");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ferrum-sql join_table create_table_skips`
Expected: FAIL — `create_join_table` / `drop_join_table` undefined; `create_table`
currently calls `column_def` for the M:N field and panics or emits `tags_id`.

- [ ] **Step 3: Skip non-stored columns in `create_table` and `add_column`**

In `crates/sql/src/ddl.rs`, in `create_table`, change the field loop:

```rust
    for f in &ct.fields {
        if !f.is_stored_column() {
            continue;
        }
        cols.push(column_def(&ct.name, f)?);
    }
```

In `add_column`, guard at the top:

```rust
pub fn add_column(ct_name: &str, field: &Field) -> Result<String, DdlError> {
    // Many-to-many fields have no row column; the caller manages the join
    // table separately (see SchemaService).
    debug_assert!(field.is_stored_column(), "add_column called for non-stored field");
    let table = table_name(ct_name)?;
    let def = column_def(ct_name, field)?;
    Ok(format!("ALTER TABLE {table} ADD COLUMN {def}"))
}
```

- [ ] **Step 4: Add `create_join_table` and `drop_join_table`**

Add to `crates/sql/src/ddl.rs`:

```rust
/// Build the `CREATE TABLE` + `CREATE INDEX` statements for a many-to-many
/// join table on `owner.<field>` targeting `target`. Returns
/// `(create_table_sql, create_index_sql)`. Column names are `<owner>_id` and
/// `<target>_id`; both FKs cascade on delete so removing a linked entry drops
/// its links.
pub fn create_join_table(
    owner: &str,
    field: &str,
    target: &str,
) -> Result<(String, String), DdlError> {
    let jt = crate::ident::join_table_name(owner, field)?;
    let owner_tbl = table_name(owner)?;
    let target_tbl = table_name(target)?;
    let owner_col = quote_ident(&format!("{owner}_id"))?;
    let target_col = quote_ident(&format!("{target}_id"))?;
    let create = format!(
        "CREATE TABLE {jt} (\
{owner_col} uuid NOT NULL REFERENCES {owner_tbl}(\"id\") ON DELETE CASCADE, \
{target_col} uuid NOT NULL REFERENCES {target_tbl}(\"id\") ON DELETE CASCADE, \
PRIMARY KEY ({owner_col}, {target_col}))"
    );
    let index = format!("CREATE INDEX ON {jt} ({target_col})");
    Ok((create, index))
}

/// `DROP TABLE <join table for owner.field>`.
pub fn drop_join_table(owner: &str, field: &str) -> Result<String, DdlError> {
    let jt = crate::ident::join_table_name(owner, field)?;
    Ok(format!("DROP TABLE {jt}"))
}
```

- [ ] **Step 5: Re-export from lib**

In `crates/sql/src/lib.rs`, extend the ddl re-export line:

```rust
pub use ddl::{
    add_column, alter_enum_values, create_join_table, create_table, drop_column,
    drop_join_table, drop_table, DdlError,
};
```

- [ ] **Step 6: Run tests to verify pass**

Run: `cargo test -p ferrum-sql`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/sql/src/ddl.rs crates/sql/src/lib.rs
git commit -m "feat(sql): join-table DDL + skip m2m row columns"
```

---

## Task 6: link insert/delete DML

**Files:**
- Modify: `crates/sql/src/dml.rs`
- Modify: `crates/sql/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/sql/src/dml.rs`:

```rust
    #[test]
    fn insert_links_uses_unnest() {
        let owner = Uuid::nil();
        // (owner_type, field, target_type, owner_id) → target column is `tag_id`.
        let (sql, owner_bind, target_col) = insert_links("post", "tags", "tag", owner).unwrap();
        assert_eq!(
            sql,
            "INSERT INTO \"j_post_tags\" (\"post_id\", \"tag_id\") \
SELECT $1::uuid, x FROM UNNEST($2::uuid[]) AS x ON CONFLICT DO NOTHING"
        );
        assert_eq!(owner_bind, owner);
        assert_eq!(target_col, "tag_id");
    }

    #[test]
    fn delete_links_clears_owner() {
        let owner = Uuid::nil();
        let (sql, bind) = delete_links("post", "tags", owner).unwrap();
        assert_eq!(sql, "DELETE FROM \"j_post_tags\" WHERE \"post_id\" = $1::uuid");
        assert_eq!(bind, owner);
    }
```

Note: `insert_links` returns `(sql, owner_uuid, target_col_name)` — the target
column name is returned for the handler's logging/debug but the SQL binds the
target ids array as `$2`. The handler binds `owner` to `$1` and the
`Vec<Uuid>` of target ids to `$2`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ferrum-sql links`
Expected: FAIL — `insert_links` / `delete_links` undefined.

- [ ] **Step 3: Implement the helpers**

The target column is `<target>_id`; the caller (the HTTP handler) knows the
target from the field's `relation_meta`, so the target type is passed in. Add to
`crates/sql/src/dml.rs`:

```rust
use crate::ident::join_table_name;

/// Build the multi-row link INSERT for a many-to-many field. The caller binds
/// `$1` = owner id (`Uuid`) and `$2` = target ids (`Vec<Uuid>`). `ON CONFLICT
/// DO NOTHING` makes re-inserting an existing link a no-op (PK guards dupes).
/// Returns `(sql, owner_id, target_col_name)`.
pub fn insert_links(
    owner_type: &str,
    field: &str,
    target_type: &str,
    owner_id: Uuid,
) -> Result<(String, Uuid, String), DmlError> {
    let jt = join_table_name(owner_type, field)?;
    let owner_col = quote_ident(&format!("{owner_type}_id"))?;
    let target_col = quote_ident(&format!("{target_type}_id"))?;
    let sql = format!(
        "INSERT INTO {jt} ({owner_col}, {target_col}) \
SELECT $1::uuid, x FROM UNNEST($2::uuid[]) AS x ON CONFLICT DO NOTHING"
    );
    Ok((sql, owner_id, format!("{target_type}_id")))
}
// The Step 1 test already calls the 4-arg signature; no further test edits needed.

/// `DELETE FROM <join> WHERE <owner>_id = $1::uuid` — clears all links for an
/// owner ahead of a replace-set re-insert. Caller binds `$1` = owner id.
pub fn delete_links(
    owner_type: &str,
    field: &str,
    owner_id: Uuid,
) -> Result<(String, Uuid), DmlError> {
    let jt = join_table_name(owner_type, field)?;
    let owner_col = quote_ident(&format!("{owner_type}_id"))?;
    let sql = format!("DELETE FROM {jt} WHERE {owner_col} = $1::uuid");
    Ok((sql, owner_id))
}
```

- [ ] **Step 4: Re-export from lib**

In `crates/sql/src/lib.rs`, extend the dml re-export:

```rust
pub use dml::{
    count, delete, delete_links, insert, insert_links, render_where,
    select_by_id, select_list, update, DmlError, SqlAndBinds,
};
```

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test -p ferrum-sql`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/sql/src/dml.rs crates/sql/src/lib.rs
git commit -m "feat(sql): insert_links/delete_links for m2m join rows"
```

---

## Task 7: registry M:N reverse lookup + inverse resolution

**Files:**
- Modify: `crates/schema/src/registry.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/schema/src/registry.rs`:

```rust
    fn m2m_field(name: &str, target: &str, inverse: Option<&str>) -> Field {
        let mut meta = serde_json::Map::new();
        meta.insert("target".into(), json!(target));
        meta.insert("cardinality".into(), json!("many_to_many"));
        if let Some(i) = inverse {
            meta.insert("inverse".into(), json!(i));
        }
        Field {
            name: name.into(),
            kind: FieldKind::Relation,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::Value::Object(meta),
        }
    }

    #[tokio::test]
    async fn m2m_targets_lists_owned_join_tables() {
        let reg = SchemaRegistry::new();
        let mut post = ct("post");
        post.fields.push(m2m_field("tags", "tag", None));
        reg.insert(post).await;
        let hits = reg.m2m_targets("post").await;
        assert_eq!(hits, vec![("tags".to_string(), "tag".to_string())]);
    }

    #[tokio::test]
    async fn m2m_referencing_finds_join_tables_pointing_at_type() {
        let reg = SchemaRegistry::new();
        let mut post = ct("post");
        post.fields.push(m2m_field("tags", "tag", None));
        reg.insert(post).await;
        // join tables whose target is "tag": post.tags
        let hits = reg.m2m_referencing("tag").await;
        assert_eq!(hits, vec![("post".to_string(), "tags".to_string())]);
    }

    #[tokio::test]
    async fn inverse_lookup_resolves_m2m() {
        let reg = SchemaRegistry::new();
        reg.insert(ct("tag")).await;
        let mut post = ct("post");
        post.fields.push(m2m_field("tags", "tag", Some("posts")));
        reg.insert(post).await;
        let hit = reg.inverse_lookup_m2m("tag", "posts").await;
        assert_eq!(
            hit,
            Some(("post".to_string(), "tags".to_string(), "tag".to_string()))
        );
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ferrum-schema m2m`
Expected: FAIL — the three new methods are undefined.

- [ ] **Step 3: Implement the lookups**

Add to `impl SchemaRegistry` in `crates/schema/src/registry.rs`:

```rust
    /// Many-to-many fields *owned by* `owner` → `(field_name, target)` pairs.
    /// Used by SchemaService to create/drop join tables for a type.
    pub async fn m2m_targets(&self, owner: &str) -> Vec<(String, String)> {
        let map = self.inner.read().await;
        let mut out = Vec::new();
        if let Some(ct) = map.get(owner) {
            for f in &ct.fields {
                if let Some(meta) = f.relation_meta() {
                    if meta.cardinality == ferrum_core::Cardinality::ManyToMany {
                        out.push((f.name.clone(), meta.target));
                    }
                }
            }
        }
        out
    }

    /// Many-to-many fields *targeting* `target` → `(owner_type, field_name)`.
    /// Used to drop dependent join tables before dropping a target type.
    pub async fn m2m_referencing(&self, target: &str) -> Vec<(String, String)> {
        let map = self.inner.read().await;
        let mut out = Vec::new();
        for ct in map.values() {
            for f in &ct.fields {
                if let Some(meta) = f.relation_meta() {
                    if meta.cardinality == ferrum_core::Cardinality::ManyToMany
                        && meta.target == target
                    {
                        out.push((ct.name.clone(), f.name.clone()));
                    }
                }
            }
        }
        out
    }

    /// Resolve an inverse populate name against many-to-many relations. Returns
    /// `(owner_type, field_name, target_type)` where `owner.field` is the M:N
    /// relation whose `inverse` matches and whose `target` is `target_name`.
    pub async fn inverse_lookup_m2m(
        &self,
        target_name: &str,
        inverse_name: &str,
    ) -> Option<(String, String, String)> {
        let map = self.inner.read().await;
        for ct in map.values() {
            for f in &ct.fields {
                let Some(meta) = f.relation_meta() else { continue };
                if meta.cardinality == ferrum_core::Cardinality::ManyToMany
                    && meta.target == target_name
                    && meta.inverse.as_deref() == Some(inverse_name)
                {
                    return Some((ct.name.clone(), f.name.clone(), meta.target));
                }
            }
        }
        None
    }
```

Note the existing `inverse_lookup` (for many_to_one) returns
`(source, fk_column)` via `f.physical_column()`. M:N fields' `physical_column()`
returns `<field>_id` but there is no such column — that's fine because
`inverse_lookup` is only used for FK-column inverse populate; M:N inverse goes
through `inverse_lookup_m2m`. No change needed to `inverse_lookup` itself.

- [ ] **Step 4: Run tests to verify pass**

Run: `cargo test -p ferrum-schema`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/schema/src/registry.rs
git commit -m "feat(schema): registry m2m_targets/m2m_referencing/inverse_lookup_m2m"
```

---

## Task 8: SchemaService join-table lifecycle

**Files:**
- Modify: `crates/schema/src/service.rs`

This task is exercised end-to-end by the Task 13 integration tests (it needs a
live Postgres). Add focused unit coverage where possible and rely on the
integration suite for the DB round-trip.

- [ ] **Step 1: Create join tables on `create`**

In `crates/schema/src/service.rs`, in `create`, after the `create_table_sql`
execute and **before** `tx.commit()`, add join-table creation for each M:N field:

```rust
        sqlx::query(&create_table_sql)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;

        // Many-to-many fields need a join table each (created after the main
        // table so its FK to ct_<owner> resolves).
        for f in &ct.fields {
            if let Some(meta) = f.relation_meta() {
                if meta.cardinality == ferrum_core::Cardinality::ManyToMany {
                    let (create_jt, create_idx) =
                        ferrum_sql::create_join_table(&ct.name, &f.name, &meta.target)
                            .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                    sqlx::query(&create_jt).execute(&mut *tx).await.map_err(map_db_err)?;
                    sqlx::query(&create_idx).execute(&mut *tx).await.map_err(map_db_err)?;
                }
            }
        }

        tx.commit().await.map_err(internal)?;
```

- [ ] **Step 2: Create/drop join tables on `patch`**

In `patch`, the drop loop currently calls `drop_column` for every dropped field.
M:N fields have no column — drop their join table instead. And added M:N fields
need a join table, not `add_column`. Replace the two loops:

```rust
        for drop_name in &payload.drop_fields {
            // Find the field being dropped on the existing type to learn its kind.
            let dropped = existing.fields.iter().find(|f| &f.name == drop_name);
            let is_m2m = dropped
                .and_then(|f| f.relation_meta())
                .map(|m| m.cardinality == ferrum_core::Cardinality::ManyToMany)
                .unwrap_or(false);
            if is_m2m {
                let sql = ferrum_sql::drop_join_table(name, drop_name)
                    .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
            } else {
                let sql = ferrum_sql::drop_column(name, drop_name)
                    .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
            }
        }
        for f in &payload.add_fields {
            if let Some(meta) = f.relation_meta() {
                if meta.cardinality == ferrum_core::Cardinality::ManyToMany {
                    let (create_jt, create_idx) =
                        ferrum_sql::create_join_table(name, &f.name, &meta.target)
                            .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                    sqlx::query(&create_jt).execute(&mut *tx).await.map_err(map_db_err)?;
                    sqlx::query(&create_idx).execute(&mut *tx).await.map_err(map_db_err)?;
                    continue;
                }
            }
            let sql = ferrum_sql::add_column(name, f)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }
```

- [ ] **Step 3: Drop join tables on `delete`**

In `delete`, before dropping the main table, drop every join table this type
owns and every join table that targets it. Replace the body around `drop_sql`:

```rust
        let owned = self.registry.m2m_targets(name).await;
        let referencing = self.registry.m2m_referencing(name).await;
        let drop_sql = ferrum_sql::drop_table(name)
            .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;

        let mut tx = self.pool.begin().await.map_err(internal)?;
        // Drop dependent join tables first so the main DROP TABLE has no
        // lingering FK references. Owned + referencing are distinct fields, so
        // there is no double-drop within one type's own delete.
        for (field, _target) in &owned {
            let sql = ferrum_sql::drop_join_table(name, field)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }
        for (owner, field) in &referencing {
            if owner == name {
                continue; // already handled in `owned`
            }
            let sql = ferrum_sql::drop_join_table(owner, field)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }
        sqlx::query(&drop_sql).execute(&mut *tx).await.map_err(map_db_err)?;
        sqlx::query("DELETE FROM _content_types WHERE name = $1")
            .bind(name)
            .execute(&mut *tx)
            .await
            .map_err(map_db_err)?;
        tx.commit().await.map_err(internal)?;
```

- [ ] **Step 4: Extend cross-ref validation to M:N**

`validate_relation_cross_refs` already iterates `f.relation_meta()` and checks
target existence + inverse collisions — those apply to M:N unchanged. The only
M:N-specific rule (no `required`) is enforced in `Field::validate` (Task 2). No
change needed here; confirm by reading the function. Add a one-line comment in
the function doc noting it covers all cardinalities.

- [ ] **Step 5: Compile + run schema tests**

Run: `cargo test -p ferrum-schema`
Expected: PASS (unit tests; DB lifecycle covered in Task 13).

- [ ] **Step 6: Commit**

```bash
git add crates/schema/src/service.rs
git commit -m "feat(schema): join-table lifecycle on create/patch/delete"
```

---

## Task 9: split M:N values out of body_to_binds

**Files:**
- Modify: `crates/http/src/entry.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/http/src/entry.rs`:

```rust
    fn ct_with_m2m() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![
                Field {
                    name: "title".into(),
                    kind: FieldKind::String,
                    required: false,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({}),
                },
                Field {
                    name: "tags".into(),
                    kind: FieldKind::Relation,
                    required: false,
                    unique: false,
                    default: json!(null),
                    max_length: None,
                    kind_meta: json!({"target":"tag","cardinality":"many_to_many"}),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn m2m_array_becomes_link_plan_not_bind() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({
            "title": "hi",
            "tags": [a.to_string(), b.to_string()]
        }))
        .unwrap().as_object().unwrap().clone();
        let (out, _checks, links) = body_to_binds(&ct_with_m2m(), body, true).unwrap();
        // tags must NOT be a column bind.
        assert!(out.get("tags").is_none());
        assert!(out.get("title").is_some());
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].field, "tags");
        assert_eq!(links[0].target, "tag");
        assert_eq!(links[0].ids, vec![a, b]);
    }

    #[test]
    fn m2m_empty_array_is_explicit_clear() {
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({"tags": []}))
            .unwrap().as_object().unwrap().clone();
        let (_out, _checks, links) = body_to_binds(&ct_with_m2m(), body, true).unwrap();
        assert_eq!(links.len(), 1);
        assert!(links[0].ids.is_empty());
        assert!(links[0].present);
    }

    #[test]
    fn m2m_absent_field_no_link_plan() {
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({"title":"x"}))
            .unwrap().as_object().unwrap().clone();
        let (_out, _checks, links) = body_to_binds(&ct_with_m2m(), body, true).unwrap();
        assert!(links.is_empty());
    }

    #[test]
    fn m2m_rejects_non_array() {
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({"tags":"nope"}))
            .unwrap().as_object().unwrap().clone();
        assert!(matches!(body_to_binds(&ct_with_m2m(), body, true), Err(Error::Validation(_))));
    }

    #[test]
    fn m2m_rejects_bad_uuid_in_array() {
        let body: Map<String, Value> = serde_json::from_value::<Value>(json!({"tags":["not-a-uuid"]}))
            .unwrap().as_object().unwrap().clone();
        assert!(matches!(body_to_binds(&ct_with_m2m(), body, true), Err(Error::Validation(_))));
    }
```

Every existing `body_to_binds` call in the test module destructures 2 values;
update them to ignore the new third: change `let (out, checks) =` to
`let (out, checks, _links) =` (and similar) in the existing entry.rs tests.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ferrum-http m2m_array_becomes_link_plan`
Expected: FAIL — `body_to_binds` returns a 2-tuple; `LinkPlan` undefined.

- [ ] **Step 3: Add `LinkPlan` and extend the return type**

In `crates/http/src/entry.rs`, add near `RelationCheck`:

```rust
/// A pending many-to-many replace-set for one relation field. `present` is
/// always true when emitted (the field appeared in the body); `ids` may be
/// empty, meaning "remove all links". The handler runs the replace-set inside
/// the write transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPlan {
    pub field: String,
    pub target: String,
    pub ids: Vec<Uuid>,
    pub present: bool,
}
```

Change the signature and return type of `body_to_binds`:

```rust
pub fn body_to_binds(
    ct: &ContentType,
    mut body: Map<String, Value>,
    require_required: bool,
) -> Result<(BTreeMap<String, BoundValue>, Vec<RelationCheck>, Vec<LinkPlan>), Error> {
```

Initialize a `let mut links: Vec<LinkPlan> = Vec::new();` alongside `checks`,
and return `Ok((out, checks, links))`.

- [ ] **Step 4: Branch M:N relations to the link plan**

Inside the `for f in &ct.fields` loop, the relation branch currently calls
`coerce_relation` for all relations. Split on cardinality. Replace:

```rust
                if f.kind == FieldKind::Relation {
                    coerce_relation(f, v, &mut out, &mut checks)?;
                    continue;
                }
```

with:

```rust
                if f.kind == FieldKind::Relation {
                    let meta = f.relation_meta().ok_or_else(|| {
                        Error::Validation(ValidationErrors::field(&f.name, "missing relation kind_meta"))
                    })?;
                    if meta.cardinality == ferrum_core::Cardinality::ManyToMany {
                        links.push(coerce_m2m(f, &meta.target, v)?);
                    } else {
                        coerce_relation(f, v, &mut out, &mut checks)?;
                    }
                    continue;
                }
```

Add the helper `coerce_m2m`:

```rust
/// Parse a many-to-many field's JSON value (must be an array of uuid strings)
/// into a `LinkPlan`. An empty array is a valid "clear all links" instruction.
fn coerce_m2m(f: &Field, target: &str, v: &Value) -> Result<LinkPlan, Error> {
    let arr = v.as_array().ok_or_else(|| {
        Error::Validation(ValidationErrors::field(
            &f.name,
            "many_to_many value must be an array of uuid strings",
        ))
    })?;
    let mut ids = Vec::with_capacity(arr.len());
    let mut seen = std::collections::HashSet::new();
    for item in arr {
        let s = item.as_str().ok_or_else(|| {
            Error::Validation(ValidationErrors::field(&f.name, "many_to_many ids must be strings"))
        })?;
        let id = Uuid::parse_str(s)
            .map_err(|_| Error::Validation(ValidationErrors::field(&f.name, "invalid uuid")))?;
        if seen.insert(id) {
            ids.push(id);
        }
    }
    Ok(LinkPlan {
        field: f.name.clone(),
        target: target.to_string(),
        ids,
        present: true,
    })
}
```

Note: the `if v.is_null() && f.required` guard above the relation branch still
runs first. For M:N, `required` is rejected at schema-create (Task 2), so a M:N
field is never required; a `null` value (rather than `[]`) for M:N falls through
to `coerce_m2m`, which rejects non-arrays — acceptable (client should send `[]`).

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test -p ferrum-http entry`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/entry.rs
git commit -m "feat(http): body_to_binds emits m2m LinkPlan, splits from column binds"
```

---

## Task 10: transactional write of row + links

**Files:**
- Modify: `crates/http/src/routes/content.rs`

This task needs a live DB to verify fully (Task 13). Make the handler changes
and confirm the crate compiles; behavior is asserted in integration.

- [ ] **Step 1: Update the three `body_to_binds` call sites**

In `create`, `update`, the destructure becomes 3-tuple:

```rust
    let (binds_map, checks, links) = body_to_binds(&ct, body, true)?;
```

(`update` uses `let (mut binds_map, checks, links) = ...`.)

- [ ] **Step 2: Validate M:N target ids in `create` and `update`**

After `verify_relation_targets_exist(&state, &checks).await?;` in both `create`
and `update`, add link-target validation:

```rust
    verify_link_targets_exist(&state, &links).await?;
```

Add the helper near `verify_relation_targets_exist`:

```rust
/// Pre-check that every many-to-many target id exists, per field. Mirrors
/// `verify_relation_targets_exist` but for `LinkPlan`s. Returns 422
/// RelationTargetMissing naming the first field with any unresolved id.
async fn verify_link_targets_exist(
    state: &AppState,
    links: &[crate::entry::LinkPlan],
) -> Result<(), ApiError> {
    for plan in links {
        if plan.ids.is_empty() {
            continue;
        }
        let table = ferrum_sql::table_name(&plan.target)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        let sql = format!("SELECT id FROM {table} WHERE id = ANY($1)");
        let rows = sqlx::query(&sql).bind(&plan.ids).fetch_all(&state.pool).await.map_err(db)?;
        let mut found = std::collections::HashSet::new();
        for r in &rows {
            let id: Uuid = r.try_get("id").map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
            found.insert(id);
        }
        let missing: Vec<String> = plan
            .ids
            .iter()
            .filter(|id| !found.contains(id))
            .map(|id| id.to_string())
            .collect();
        if !missing.is_empty() {
            return Err(ApiError(Error::Validation(
                ValidationErrors::relation_target_missing(&plan.field, missing),
            )));
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Write row + links in one transaction (create)**

Replace the body of `create` from the `insert` call through the row fetch with a
transaction that also writes links:

```rust
    let (sql, binds) = ferrum_sql::insert(&ct, &binds_map)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let mut tx = state.pool.begin().await.map_err(db)?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_one(&mut *tx).await.map_err(|e| db_with_relation_context(e, &checks))?;
    let body = row_to_json(&ct, &row)?;
    let new_id = body
        .get("id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ApiError(Error::Internal(anyhow::anyhow!("insert returned no id"))))?;

    write_links(&mut tx, &ct.name, &links, new_id).await?;
    tx.commit().await.map_err(db)?;

    state.events.emit(Event::EntryCreated { content_type: ct.name.clone(), id: new_id }).await;
    Ok((StatusCode::CREATED, Json(body)))
```

Note: `bind_all` returns a `sqlx::query::Query`; it executes against
`&mut *tx` exactly like against `&state.pool`. The `Event` emit moves out of the
old `if let Some(id)` since `new_id` is now non-optional.

- [ ] **Step 4: Write link replace-set in `update`**

In `update`, after the row `fetch_optional` succeeds, wrap the update +
link replace-set in a transaction. Replace from the `update` SQL build through
the response with:

```rust
    let (sql, binds) = ferrum_sql::update(&ct, id, &binds_map)
        .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;

    let mut tx = state.pool.begin().await.map_err(db)?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| db_with_relation_context(e, &checks))?;
    let row = match row {
        Some(r) => r,
        None => {
            // Roll back implicitly by dropping tx.
            return Err(ApiError(Error::NotFound));
        }
    };
    write_links(&mut tx, &ct.name, &links, id).await?;
    tx.commit().await.map_err(db)?;

    state.events.emit(Event::EntryUpdated { content_type: ct.name.clone(), id }).await;
    Ok(Json(row_to_json(&ct, &row)?))
```

- [ ] **Step 5: Add the `write_links` helper**

Add to `crates/http/src/routes/content.rs`:

```rust
/// Apply each present LinkPlan as a replace-set inside the given transaction:
/// delete all existing links for the owner on that field, then insert the
/// supplied target ids. Absent fields are not in `links`, so their links are
/// untouched.
async fn write_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    owner_type: &str,
    links: &[crate::entry::LinkPlan],
    owner_id: Uuid,
) -> Result<(), ApiError> {
    for plan in links {
        if !plan.present {
            continue;
        }
        let (del_sql, _) = ferrum_sql::delete_links(owner_type, &plan.field, owner_id)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        sqlx::query(&del_sql).bind(owner_id).execute(&mut **tx).await.map_err(db)?;
        if plan.ids.is_empty() {
            continue;
        }
        let (ins_sql, _, _) =
            ferrum_sql::insert_links(owner_type, &plan.field, &plan.target, owner_id)
                .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        sqlx::query(&ins_sql)
            .bind(owner_id)
            .bind(&plan.ids)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
    }
    Ok(())
}
```

- [ ] **Step 6: Compile**

Run: `cargo build -p ferrum-http`
Expected: builds clean. (Behavior verified in Task 13.)

- [ ] **Step 7: Commit**

```bash
git add crates/http/src/routes/content.rs
git commit -m "feat(http): transactional row+link writes, m2m replace-set"
```

---

## Task 11: populate — Many + InverseOne

**Files:**
- Modify: `crates/http/src/populate.rs`

- [ ] **Step 1: Write the failing parse tests**

Add to the `tests` module in `crates/http/src/populate.rs`:

```rust
    fn ct_with_m2m() -> ContentType {
        ContentType {
            id: Uuid::new_v4(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "tags".into(),
                kind: FieldKind::Relation,
                required: false,
                unique: false,
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: json!({"target":"tag","cardinality":"many_to_many","inverse":"posts"}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn tag_ct() -> ContentType {
        ContentType {
            id: Uuid::new_v4(),
            name: "tag".into(),
            display_name: "Tag".into(),
            fields: vec![Field {
                name: "label".into(),
                kind: FieldKind::String,
                required: false, unique: false,
                default: serde_json::Value::Null, max_length: None, kind_meta: json!({}),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn parse_m2m_forward() {
        let reg = SchemaRegistry::new();
        reg.insert(tag_ct()).await;
        reg.insert(ct_with_m2m()).await;
        let post = reg.get("post").await.unwrap();
        let out = parse_populate(&post, &reg, "tags").await.unwrap();
        match &out[0] {
            PopulateField::Many { field_name, join_table: _, self_col, other_col, target } => {
                assert_eq!(field_name, "tags");
                assert_eq!(self_col, "post_id");
                assert_eq!(other_col, "tag_id");
                assert_eq!(target, "tag");
            }
            other => panic!("expected Many, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_m2m_inverse() {
        let reg = SchemaRegistry::new();
        reg.insert(tag_ct()).await;
        reg.insert(ct_with_m2m()).await;
        let tag = reg.get("tag").await.unwrap();
        let out = parse_populate(&tag, &reg, "posts").await.unwrap();
        match &out[0] {
            PopulateField::Many { field_name, self_col, other_col, target, .. } => {
                assert_eq!(field_name, "posts");
                // Inverse: parent is tag, children are posts.
                assert_eq!(self_col, "tag_id");
                assert_eq!(other_col, "post_id");
                assert_eq!(target, "post");
            }
            other => panic!("expected Many (inverse), got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ferrum-http parse_m2m_forward`
Expected: FAIL — `PopulateField::Many` undefined.

- [ ] **Step 3: Add the variants**

In `crates/http/src/populate.rs`, extend `PopulateField`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopulateField {
    Forward {
        field_name: String,
        target: String,
    },
    Inverse {
        field_name: String,
        source: String,
        fk_col: String,
    },
    /// one_to_one inverse: at most one child (FK is UNIQUE), returned as a
    /// single object or null rather than an array.
    InverseOne {
        field_name: String,
        source: String,
        fk_col: String,
    },
    /// many_to_many (forward or inverse). `self_col`/`other_col` are the join
    /// table's owner/target columns from the *current* type's perspective.
    Many {
        field_name: String,
        join_table: String,
        self_col: String,
        other_col: String,
        target: String,
    },
}
```

- [ ] **Step 4: Resolve M:N + 1:1 inverse in `parse_populate`**

In `parse_populate`, the forward-relation arm currently emits `Forward` for any
relation field. Branch on cardinality. Replace the forward block:

```rust
        if let Some(f) = ct.fields.iter().find(|f| f.name == name) {
            if f.kind == FieldKind::Relation {
                let meta = f.relation_meta().ok_or_else(|| {
                    Error::Validation(ValidationErrors::single(format!(
                        "unknown populate field `{name}`"
                    )))
                })?;
                if meta.cardinality == ferrum_core::Cardinality::ManyToMany {
                    let join_table = ferrum_sql::join_table_name(&ct.name, &f.name)
                        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                    out.push(PopulateField::Many {
                        field_name: name.to_string(),
                        join_table,
                        self_col: format!("{}_id", ct.name),
                        other_col: format!("{}_id", meta.target),
                        target: meta.target,
                    });
                } else {
                    out.push(PopulateField::Forward {
                        field_name: name.to_string(),
                        target: meta.target,
                    });
                }
                continue;
            }
        }
```

After the existing `inverse_lookup` (many_to_one) arm, add an M:N inverse arm
and a 1:1 inverse refinement. The `inverse_lookup` arm currently emits
`Inverse`. We must distinguish 1:1 inverse (→ `InverseOne`). Replace the
`inverse_lookup` block:

```rust
        if let Some((source, fk_col)) = registry.inverse_lookup(&ct.name, name).await {
            // Determine whether the source relation is one_to_one (→ single
            // object) or many_to_one (→ array).
            let is_one_to_one = registry
                .get(&source)
                .await
                .map(|src| {
                    src.fields.iter().any(|f| {
                        f.relation_meta().is_some_and(|m| {
                            m.target == ct.name
                                && m.inverse.as_deref() == Some(name)
                                && m.cardinality == ferrum_core::Cardinality::OneToOne
                        })
                    })
                })
                .unwrap_or(false);
            if is_one_to_one {
                out.push(PopulateField::InverseOne {
                    field_name: name.to_string(),
                    source,
                    fk_col,
                });
            } else {
                out.push(PopulateField::Inverse {
                    field_name: name.to_string(),
                    source,
                    fk_col,
                });
            }
            continue;
        }
        if let Some((owner, field, _target)) = registry.inverse_lookup_m2m(&ct.name, name).await {
            let join_table = ferrum_sql::join_table_name(&owner, &field)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            out.push(PopulateField::Many {
                field_name: name.to_string(),
                join_table,
                // Current type is the *target* of the M:N; its own id column in
                // the join table is `<ct.name>_id`, children are `<owner>_id`.
                self_col: format!("{}_id", ct.name),
                other_col: format!("{owner}_id"),
                target: owner,
            });
            continue;
        }
```

- [ ] **Step 5: Run parse tests to verify pass**

Run: `cargo test -p ferrum-http parse_m2m`
Expected: PASS.

- [ ] **Step 6: Implement `apply_many`**

Add to `crates/http/src/populate.rs`. It reuses `group_inverse_children`:

```rust
/// Hydrate a many-to-many field in-place. One batched SELECT joins the join
/// table to the target rows for all parents, then groups per parent with the
/// existing per-parent cap. Parents with no links get `[]`.
#[allow(clippy::too_many_arguments)]
pub async fn apply_many(
    pool: &PgPool,
    registry: &SchemaRegistry,
    rows: &mut [Map<String, Value>],
    field_name: &str,
    join_table: &str,
    self_col: &str,
    other_col: &str,
    target: &str,
) -> Result<(), Error> {
    let target_ct = registry.get(target).await.ok_or_else(|| {
        Error::Internal(anyhow::anyhow!("populate m2m target vanished: {target}"))
    })?;
    let mut parent_ids: Vec<Uuid> = Vec::with_capacity(rows.len());
    for r in rows.iter() {
        if let Some(Value::String(s)) = r.get("id") {
            if let Ok(u) = Uuid::parse_str(s) {
                parent_ids.push(u);
            }
        }
    }
    if parent_ids.is_empty() {
        for r in rows.iter_mut() {
            r.insert(field_name.into(), Value::Array(Vec::new()));
        }
        return Ok(());
    }
    let target_tbl = ferrum_sql::table_name(target)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let self_q = ferrum_sql::quote_ident(self_col)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    let other_q = ferrum_sql::quote_ident(other_col)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    // join_table is already a quoted identifier from join_table_name.
    let limit = (INVERSE_LIMIT_PER_PARENT + 1) * parent_ids.len();
    let sql = format!(
        "SELECT j.{self_q} AS __parent, t.* \
         FROM {join_table} j JOIN {target_tbl} t ON t.\"id\" = j.{other_q} \
         WHERE j.{self_q} = ANY($1) \
         ORDER BY j.{self_q}, t.\"id\" LIMIT {limit}"
    );
    let fetched = sqlx::query(&sql)
        .bind(&parent_ids)
        .fetch_all(pool)
        .await
        .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    let mut buckets: Vec<(Uuid, Map<String, Value>)> = Vec::with_capacity(fetched.len());
    for row in &fetched {
        let parent: Uuid = row
            .try_get("__parent")
            .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
        let map = match crate::entry::row_to_json(&target_ct, row)? {
            Value::Object(m) => m,
            _ => unreachable!("row_to_json returns an object"),
        };
        buckets.push((parent, map));
    }
    let grouped = group_inverse_children(&parent_ids, buckets, INVERSE_LIMIT_PER_PARENT);
    for r in rows.iter_mut() {
        let pid = r
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        match pid.and_then(|p| grouped.get(&p)) {
            Some(g) => {
                r.insert(field_name.into(), Value::Array(g.children.clone()));
                if g.truncated {
                    r.insert(format!("{field_name}_truncated"), Value::Bool(true));
                }
            }
            None => {
                r.insert(field_name.into(), Value::Array(Vec::new()));
            }
        }
    }
    Ok(())
}
```

Note: `row_to_json` reads `id/created_at/updated_at` plus declared fields by
name; the extra `__parent` column in the result set is ignored by it (it only
reads named columns), so no clash.

- [ ] **Step 7: Implement `apply_inverse_one`**

`InverseOne` reuses the same SELECT as inverse but returns a single object.
Add a thin wrapper that calls the existing batched inverse query path with a
cap of 1 and unwraps the single child:

```rust
/// Hydrate a one_to_one inverse: at most one child per parent (FK is UNIQUE).
/// Sets the field to the single child object, or `null` when none.
pub async fn apply_inverse_one(
    pool: &PgPool,
    registry: &SchemaRegistry,
    rows: &mut [Map<String, Value>],
    field_name: &str,
    source_table: &str,
    fk_col: &str,
) -> Result<(), Error> {
    // Reuse apply_inverse into a temp key, then collapse the array to a scalar.
    let tmp_key = format!("__one_{field_name}");
    apply_inverse(pool, registry, rows, &tmp_key, source_table, fk_col).await?;
    for r in rows.iter_mut() {
        let collapsed = match r.remove(&tmp_key) {
            Some(Value::Array(mut xs)) if !xs.is_empty() => xs.remove(0),
            _ => Value::Null,
        };
        // Drop any truncation marker apply_inverse may have added.
        r.remove(&format!("{tmp_key}_truncated"));
        r.insert(field_name.into(), collapsed);
    }
    Ok(())
}
```

- [ ] **Step 8: Run the http populate tests**

Run: `cargo test -p ferrum-http populate`
Expected: PASS (unit/parse tests; DB hydration covered in Task 13).

- [ ] **Step 9: Commit**

```bash
git add crates/http/src/populate.rs
git commit -m "feat(http): populate Many + InverseOne for m2m and 1:1"
```

---

## Task 12: wire new populate variants into the handler

**Files:**
- Modify: `crates/http/src/routes/content.rs`

- [ ] **Step 1: Dispatch the new variants in `apply_populate`**

In `crates/http/src/routes/content.rs`, extend the `match f` in `apply_populate`:

```rust
        match f {
            PopulateField::Forward { field_name, target } => {
                populate::apply_forward(&state.pool, registry, rows, &field_name, &target).await?;
            }
            PopulateField::Inverse { field_name, source, fk_col } => {
                populate::apply_inverse(&state.pool, registry, rows, &field_name, &source, &fk_col).await?;
            }
            PopulateField::InverseOne { field_name, source, fk_col } => {
                populate::apply_inverse_one(&state.pool, registry, rows, &field_name, &source, &fk_col).await?;
            }
            PopulateField::Many { field_name, join_table, self_col, other_col, target } => {
                populate::apply_many(
                    &state.pool, registry, rows, &field_name,
                    &join_table, &self_col, &other_col, &target,
                ).await?;
            }
        }
```

- [ ] **Step 2: Compile the whole workspace**

Run: `cargo build`
Expected: builds clean across all crates.

- [ ] **Step 3: Run all non-DB unit tests**

Run: `cargo test --lib`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/routes/content.rs
git commit -m "feat(http): dispatch InverseOne + Many populate variants"
```

---

## Task 13: integration tests (testcontainers)

**Files:**
- Create: `crates/bin/tests/relations_m2m.rs`

- [ ] **Step 1: Scaffold the test file with fixtures**

Create `crates/bin/tests/relations_m2m.rs`. Use the same `common::TestApp`
harness as `crates/bin/tests/relations.rs`:

```rust
//! Phase 2 relations close-out: one_to_one + many_to_many integration tests.
//! Boots a real Postgres per test via testcontainers and drives the axum
//! router in-process.

mod common;
use common::TestApp;
use serde_json::{json, Value};
use uuid::Uuid;

/// post.tags many_to_many → tag, inverse "posts".
async fn setup_post_tags(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "tag",
            "display_name": "Tag",
            "fields": [{"name": "label", "kind": "string"}]
        }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "post",
            "display_name": "Post",
            "fields": [
                {"name": "title", "kind": "string"},
                {"name": "tags", "kind": "relation",
                 "kind_meta": {"target": "tag", "cardinality": "many_to_many", "inverse": "posts"}}
            ]
        }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn create_tag(app: &TestApp, label: &str) -> Uuid {
    let resp = app
        .admin(app.client.post(app.url("/api/tag")))
        .json(&json!({"label": label}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}
```

- [ ] **Step 2: Test — create with links + forward populate**

```rust
#[tokio::test]
async fn m2m_create_and_populate_forward() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let t1 = create_tag(&app, "rust").await;
    let t2 = create_tag(&app, "web").await;

    let resp = app
        .admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "hello", "tags": [t1, t2]}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let post: Value = resp.json().await.unwrap();
    let post_id = post["id"].as_str().unwrap();

    // Unpopulated GET omits tags.
    let resp = app.admin(app.client.get(app.url(&format!("/api/post/{post_id}")))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("tags").is_none(), "tags should be omitted when not populated: {body}");

    // Populated GET returns the tag objects.
    let resp = app.admin(app.client.get(app.url(&format!("/api/post/{post_id}?populate=tags")))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    let tags = body["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    let labels: Vec<&str> = tags.iter().map(|t| t["label"].as_str().unwrap()).collect();
    assert!(labels.contains(&"rust") && labels.contains(&"web"));
}
```

- [ ] **Step 3: Test — inverse populate**

```rust
#[tokio::test]
async fn m2m_inverse_populate() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let t1 = create_tag(&app, "rust").await;

    for title in ["a", "b"] {
        let resp = app.admin(app.client.post(app.url("/api/post")))
            .json(&json!({"title": title, "tags": [t1]})).send().await.unwrap();
        assert_eq!(resp.status(), 201);
    }

    let resp = app.admin(app.client.get(app.url(&format!("/api/tag/{t1}?populate=posts")))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    let posts = body["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 2);
}
```

- [ ] **Step 4: Test — PATCH replace-set (add/remove/clear)**

```rust
#[tokio::test]
async fn m2m_patch_replace_set() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let t1 = create_tag(&app, "rust").await;
    let t2 = create_tag(&app, "web").await;
    let t3 = create_tag(&app, "db").await;

    let resp = app.admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "p", "tags": [t1, t2]})).send().await.unwrap();
    let post: Value = resp.json().await.unwrap();
    let id = post["id"].as_str().unwrap().to_string();

    // Replace {t1,t2} with {t2,t3}.
    let resp = app.admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({"title": "p", "tags": [t2, t3]})).send().await.unwrap();
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let resp = app.admin(app.client.get(app.url(&format!("/api/post/{id}?populate=tags")))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    let labels: Vec<&str> = body["tags"].as_array().unwrap().iter().map(|t| t["label"].as_str().unwrap()).collect();
    assert_eq!(labels.len(), 2);
    assert!(labels.contains(&"web") && labels.contains(&"db") && !labels.contains(&"rust"));

    // Clear with [].
    let resp = app.admin(app.client.put(app.url(&format!("/api/post/{id}"))))
        .json(&json!({"title": "p", "tags": []})).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let resp = app.admin(app.client.get(app.url(&format!("/api/post/{id}?populate=tags")))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert!(body["tags"].as_array().unwrap().is_empty());
}
```

- [ ] **Step 5: Test — bad target id → 422, link cascade on tag delete**

```rust
#[tokio::test]
async fn m2m_bad_target_id_rejected() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let ghost = Uuid::new_v4();
    let resp = app.admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "x", "tags": [ghost]})).send().await.unwrap();
    assert_eq!(resp.status(), 422, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn m2m_links_cascade_on_target_delete() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    let t1 = create_tag(&app, "rust").await;
    let resp = app.admin(app.client.post(app.url("/api/post")))
        .json(&json!({"title": "p", "tags": [t1]})).send().await.unwrap();
    let id = resp.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();

    // Deleting the tag cascades the join row; the post survives with no tags.
    let resp = app.admin(app.client.delete(app.url(&format!("/api/tag/{t1}")))).send().await.unwrap();
    assert_eq!(resp.status(), 204, "{}", resp.text().await.unwrap());

    let resp = app.admin(app.client.get(app.url(&format!("/api/post/{id}?populate=tags")))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert!(body["tags"].as_array().unwrap().is_empty());
}
```

- [ ] **Step 6: Test — one_to_one unique + inverse single object**

```rust
#[tokio::test]
async fn one_to_one_unique_and_inverse() {
    let app = TestApp::spawn().await;
    // profile (target) + user.profile one_to_one inverse "user".
    let resp = app.admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({"name":"profile","display_name":"Profile","fields":[{"name":"bio","kind":"string"}]}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let resp = app.admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({"name":"user","display_name":"User","fields":[
            {"name":"name","kind":"string"},
            {"name":"profile","kind":"relation","kind_meta":{"target":"profile","cardinality":"one_to_one","inverse":"user"}}
        ]}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    // Create a profile + two users; second user pointing at the same profile → 409.
    let resp = app.admin(app.client.post(app.url("/api/profile"))).json(&json!({"bio":"hi"})).send().await.unwrap();
    let prof_id = resp.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();

    let resp = app.admin(app.client.post(app.url("/api/user")))
        .json(&json!({"name":"a","profile":prof_id})).send().await.unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());

    let resp = app.admin(app.client.post(app.url("/api/user")))
        .json(&json!({"name":"b","profile":prof_id})).send().await.unwrap();
    assert_eq!(resp.status(), 409, "second user reusing profile must conflict: {}", resp.text().await.unwrap());

    // Inverse populate from profile → single object, not array.
    let resp = app.admin(app.client.get(app.url(&format!("/api/profile/{prof_id}?populate=user")))).send().await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert!(body["user"].is_object(), "1:1 inverse should be a single object: {body}");
    assert_eq!(body["user"]["name"].as_str().unwrap(), "a");
}
```

- [ ] **Step 7: Test — join table dropped on type/field delete**

```rust
#[tokio::test]
async fn m2m_join_table_dropped_with_type() {
    let app = TestApp::spawn().await;
    setup_post_tags(&app).await;
    // Dropping post should drop j_post_tags; recreating post must succeed
    // (no leftover table collision).
    let resp = app.admin(app.client.delete(app.url("/admin/content-types/post"))).send().await.unwrap();
    assert_eq!(resp.status(), 204, "{}", resp.text().await.unwrap());

    let resp = app.admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name":"post","display_name":"Post","fields":[
                {"name":"title","kind":"string"},
                {"name":"tags","kind":"relation","kind_meta":{"target":"tag","cardinality":"many_to_many"}}
            ]
        }))
        .send().await.unwrap();
    assert_eq!(resp.status(), 201, "recreate after drop must not collide: {}", resp.text().await.unwrap());
}
```

- [ ] **Step 8: Run the integration suite**

Run: `cargo test -p ferrum --test relations_m2m`
Expected: PASS (requires Docker for testcontainers).

- [ ] **Step 9: Run the full test suite to confirm no regressions**

Run: `cargo test`
Expected: PASS across all crates, including the existing `relations.rs` suite.

- [ ] **Step 10: Commit**

```bash
git add crates/bin/tests/relations_m2m.rs
git commit -m "test(bin): integration coverage for one_to_one + many_to_many"
```

---

## Final verification

- [ ] Run `cargo build` — clean.
- [ ] Run `cargo clippy --all-targets -- -D warnings` — clean (fix any lints).
- [ ] Run `cargo test` — all green, including existing `relations.rs`.
- [ ] Confirm the admin DELETE route path used in tests (`/admin/content-types/:name`) matches the actual admin router; if it differs, adjust the test URLs to the real path.
