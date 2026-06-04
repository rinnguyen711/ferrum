# Media Field Kind Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `Media` content-type field kind so content types can reference one or many Media Library assets, returning full asset metadata inline on read.

**Architecture:** A parallel "media" code path mirroring the existing relation machinery but pointed at the system table `_media_assets`. Single media is a nullable FK column (`ON DELETE SET NULL`); multiple media is an ordered join table `j_media_<ct>_<field>` (`ON DELETE CASCADE`). Reads always embed asset objects (no `?populate` gate); writes take bare asset id(s). Media fields are never `required`, so the single FK stays nullable and asset deletes never block.

**Tech Stack:** Rust (axum, sqlx, serde, Postgres), React/TypeScript (Vite). Spec: `docs/superpowers/specs/2026-06-04-media-field-kind-design.md`.

---

## File Structure

| File | Responsibility | Change |
|------|----------------|--------|
| `crates/core/src/field.rs` | `FieldKind::Media`, `MediaMeta`, validation, accessors | Modify |
| `crates/sql/src/ident.rs` | `media_join_table_name(ct, field)` | Modify |
| `crates/sql/src/ddl.rs` | media single column def, `create/drop_media_join_table` | Modify |
| `crates/sql/src/dml.rs` | `insert_media_links` / `delete_media_links` (ordered, w/ position) | Modify |
| `crates/schema/src/service.rs` | create/patch wiring for media join tables | Modify |
| `crates/http/src/entry.rs` | write coercion (`MediaCheck`, `MediaLinkPlan`), `decode_field` single | Modify |
| `crates/http/src/media_embed.rs` | always-on embed pass | Create |
| `crates/http/src/lib.rs` | register `media_embed` module | Modify |
| `crates/http/src/routes/content.rs` | run media checks + link plans, invoke embed pass | Modify |
| `ui/src/api/types.ts` | `"media"` kind, `MediaMeta`, `mediaMeta()` | Modify |
| `ui/src/builder/draftModel.ts` | `"media"` in KINDS, `mediaMultiple`, kind_meta build | Modify |
| `ui/src/builder/FieldConfigModal.tsx` | media config block + required guard | Modify |
| `ui/src/screens/media/AssetPicker.tsx` | browse/select assets modal | Create |
| `ui/src/screens/EntryEditor.tsx` | `MediaField` input + save passthrough | Modify |

Order is bottom-up: core types → sql DDL/DML → schema service → http write → http read/embed → http handler wiring → UI. Each task is independently testable and committed.

---

## Task 1: Core — `FieldKind::Media` + `MediaMeta` + validation

**Files:**
- Modify: `crates/core/src/field.rs`

- [ ] **Step 1: Add the `Media` enum variant**

In the `FieldKind` enum (after `Slug`), add:

```rust
    /// Phase 2.6: references one or many Media Library assets (`_media_assets`).
    /// Configuration lives in `Field.kind_meta`; see `MediaMeta`. Single media is
    /// a nullable FK column; multiple media lives in an ordered join table.
    Media,
```

- [ ] **Step 2: Make `BoundValue::from_json` reject `Media` as a scalar**

In `BoundValue::from_json`, alongside the existing `(FieldKind::Relation, _) => Err(CoerceError::TypeMismatch),` arm, add:

```rust
            (FieldKind::Media, _) => Err(CoerceError::TypeMismatch),
```

- [ ] **Step 3: Add `MediaMeta` + `from_value` (write the failing test first)**

Add a new test module at the end of the file:

```rust
#[cfg(test)]
mod media_meta_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_empty_defaults_single() {
        let m = MediaMeta::from_value(&json!({})).unwrap();
        assert!(!m.multiple);
    }

    #[test]
    fn parse_multiple_true() {
        let m = MediaMeta::from_value(&json!({"multiple": true})).unwrap();
        assert!(m.multiple);
    }

    #[test]
    fn reject_non_bool_multiple() {
        assert_eq!(
            MediaMeta::from_value(&json!({"multiple": "yes"})).unwrap_err(),
            FieldError::MediaMetaShape
        );
    }

    #[test]
    fn reject_extra_keys() {
        assert_eq!(
            MediaMeta::from_value(&json!({"multiple": true, "x": 1})).unwrap_err(),
            FieldError::MediaMetaShape
        );
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p rustapi-core media_meta_tests`
Expected: FAIL — `MediaMeta` not found / `MediaMetaShape` not found.

- [ ] **Step 5: Implement `MediaMeta` + the `MediaMetaShape` error variant**

Add the `MediaMetaShape` variant to `FieldError` (near `EnumMetaShape`):

```rust
    #[error("media kind_meta must be {{}} or {{multiple: bool}}")]
    MediaMetaShape,
    #[error("media field cannot be unique")]
    MediaFieldUniqueUnsupported,
    #[error("media field cannot have a default")]
    MediaFieldDefaultUnsupported,
    #[error("media field cannot be required")]
    MediaFieldRequiredUnsupported,
```

Add the struct + parser (place after `EnumMeta`'s impl block):

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct MediaMeta {
    pub multiple: bool,
}

impl MediaMeta {
    pub fn from_value(v: &serde_json::Value) -> Result<Self, FieldError> {
        let obj = v.as_object().ok_or(FieldError::MediaMetaShape)?;
        for key in obj.keys() {
            if key != "multiple" {
                return Err(FieldError::MediaMetaShape);
            }
        }
        let multiple = match obj.get("multiple") {
            None => false,
            Some(serde_json::Value::Bool(b)) => *b,
            Some(_) => return Err(FieldError::MediaMetaShape),
        };
        Ok(Self { multiple })
    }
}
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p rustapi-core media_meta_tests`
Expected: PASS (4 tests).

- [ ] **Step 7: Add validation behavior (write failing tests first)**

Add to the existing `field_tests` module (or a new `media_field_tests` module at the end):

```rust
#[cfg(test)]
mod media_field_tests {
    use super::*;
    use serde_json::json;

    fn media(multiple: bool) -> Field {
        Field {
            name: "hero".into(),
            kind: FieldKind::Media,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: json!({"multiple": multiple}),
        }
    }

    #[test]
    fn single_media_ok() {
        assert!(media(false).validate().is_ok());
    }

    #[test]
    fn multi_media_ok() {
        assert!(media(true).validate().is_ok());
    }

    #[test]
    fn empty_kind_meta_ok() {
        let mut f = media(false);
        f.kind_meta = json!({});
        assert!(f.validate().is_ok());
    }

    #[test]
    fn rejects_unique() {
        let mut f = media(false);
        f.unique = true;
        assert_eq!(f.validate().unwrap_err(), FieldError::MediaFieldUniqueUnsupported);
    }

    #[test]
    fn rejects_default() {
        let mut f = media(false);
        f.default = json!("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(f.validate().unwrap_err(), FieldError::MediaFieldDefaultUnsupported);
    }

    #[test]
    fn rejects_required_single() {
        let mut f = media(false);
        f.required = true;
        assert_eq!(f.validate().unwrap_err(), FieldError::MediaFieldRequiredUnsupported);
    }

    #[test]
    fn rejects_required_multiple() {
        let mut f = media(true);
        f.required = true;
        assert_eq!(f.validate().unwrap_err(), FieldError::MediaFieldRequiredUnsupported);
    }

    #[test]
    fn physical_column_single_suffixes_id() {
        assert_eq!(media(false).physical_column(), "hero_id");
    }

    #[test]
    fn is_stored_column_matrix() {
        assert!(media(false).is_stored_column());
        assert!(!media(true).is_stored_column());
    }
}
```

- [ ] **Step 8: Run the tests to verify they fail**

Run: `cargo test -p rustapi-core media_field_tests`
Expected: FAIL — validation branch not present; `physical_column`/`is_stored_column` not yet media-aware.

- [ ] **Step 9: Implement the validation branch + helper updates**

In `Field::validate`, add a media branch before the primitive fallback (after the `Json` branch, mirroring the relation branch):

```rust
        if self.kind == FieldKind::Media {
            if self.unique {
                return Err(FieldError::MediaFieldUniqueUnsupported);
            }
            if !self.default.is_null() {
                return Err(FieldError::MediaFieldDefaultUnsupported);
            }
            if self.required {
                return Err(FieldError::MediaFieldRequiredUnsupported);
            }
            MediaMeta::from_value(&self.kind_meta)?;
            return Ok(());
        }
```

Add the `media_meta()` accessor near `relation_meta()`:

```rust
    pub fn media_meta(&self) -> Option<MediaMeta> {
        if self.kind == FieldKind::Media {
            MediaMeta::from_value(&self.kind_meta).ok()
        } else {
            None
        }
    }
```

Update `physical_column()` to suffix `_id` for single media:

```rust
    pub fn physical_column(&self) -> String {
        if self.kind == FieldKind::Relation {
            format!("{}_id", self.name)
        } else if self.kind == FieldKind::Media {
            format!("{}_id", self.name)
        } else {
            self.name.clone()
        }
    }
```

Update `is_stored_column()` so multiple media is not a row column:

```rust
    pub fn is_stored_column(&self) -> bool {
        if self.kind == FieldKind::Relation {
            return self
                .relation_meta()
                .map(|m| m.cardinality != Cardinality::ManyToMany)
                .unwrap_or(false);
        }
        if self.kind == FieldKind::Media {
            return self.media_meta().map(|m| !m.multiple).unwrap_or(true);
        }
        true
    }
```

- [ ] **Step 10: Run all core tests**

Run: `cargo test -p rustapi-core`
Expected: PASS (all, including the new media tests).

- [ ] **Step 11: Commit**

```bash
git add crates/core/src/field.rs
git commit -m "feat(core): add FieldKind::Media with MediaMeta and validation"
```

---

## Task 2: SQL — ident helper `media_join_table_name`

**Files:**
- Modify: `crates/sql/src/ident.rs`

- [ ] **Step 1: Write the failing test**

Add to the test module in `crates/sql/src/ident.rs`:

```rust
    #[test]
    fn media_join_table_name_builds() {
        assert_eq!(
            super::media_join_table_name("post", "gallery").unwrap(),
            "\"j_media_post_gallery\""
        );
    }

    #[test]
    fn media_join_table_name_rejects_bad_ident() {
        assert!(super::media_join_table_name("Bad", "gallery").is_err());
        assert!(super::media_join_table_name("post", "Bad").is_err());
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p rustapi-sql media_join_table_name`
Expected: FAIL — function not found.

- [ ] **Step 3: Implement `media_join_table_name`**

In `crates/sql/src/ident.rs`, mirror `join_table_name` (which builds `"j_<owner>_<field>"`). Add:

```rust
/// `"j_media_<ct>_<field>"` — the ordered join table for a multiple-media field.
/// The `j_media_` prefix keeps it distinct from relation join tables
/// (`j_<owner>_<field>`) so a relation and a media field of the same name never
/// collide. Both `ct` and `field` are validated as identifiers.
pub fn media_join_table_name(ct: &str, field: &str) -> Result<String, IdentError> {
    if !rustapi_core::reserved::is_valid_ident(ct) || !rustapi_core::reserved::is_valid_ident(field) {
        return Err(IdentError(format!("invalid identifier in media join table: {ct}.{field}")));
    }
    quote_ident(&format!("j_media_{ct}_{field}"))
}
```

> Note: match the existing `join_table_name` implementation's exact validation/quoting style in this file. If `join_table_name` validates differently (e.g. via a shared helper), reuse that same helper rather than the snippet above.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p rustapi-sql media_join_table_name`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/ident.rs
git commit -m "feat(sql): media_join_table_name ident helper"
```

---

## Task 3: SQL — media single column DDL

**Files:**
- Modify: `crates/sql/src/ddl.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/sql/src/ddl.rs`:

```rust
    #[test]
    fn create_table_emits_media_single_fk_set_null() {
        let mut f = field("hero", FieldKind::Media);
        f.kind_meta = json!({"multiple": false});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(
            sql.contains("\"hero_id\" uuid REFERENCES \"_media_assets\"(\"id\") ON DELETE SET NULL"),
            "got: {sql}"
        );
        // Always nullable: media is never required.
        assert!(!sql.contains("\"hero_id\" uuid NOT NULL"), "got: {sql}");
    }

    #[test]
    fn create_table_skips_multiple_media_column() {
        let mut f = field("gallery", FieldKind::Media);
        f.kind_meta = json!({"multiple": true});
        let sql = create_table(&ct(vec![f])).unwrap();
        assert!(!sql.contains("gallery"), "got: {sql}");
    }

    #[test]
    fn add_column_emits_media_single_fk() {
        let mut f = field("hero", FieldKind::Media);
        f.kind_meta = json!({"multiple": false});
        let sql = add_column("post", &f).unwrap();
        assert_eq!(
            sql,
            "ALTER TABLE \"ct_post\" ADD COLUMN \"hero_id\" uuid REFERENCES \"_media_assets\"(\"id\") ON DELETE SET NULL"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rustapi-sql media_single`
Expected: FAIL — media currently falls into the `sql_type` `_ => "TEXT"` arm, so the FK SQL is absent. (`create_table_skips_multiple_media_column` may already pass since `is_stored_column()` is false for multiple; that's fine.)

- [ ] **Step 3: Implement the media single column branch**

In `crates/sql/src/ddl.rs::column_def`, add a media branch before the `Enum` branch (after the `Relation` branch):

```rust
    if f.kind == FieldKind::Media {
        // Only single media reaches here; multiple media has no row column
        // (is_stored_column() == false) and is filtered out by the caller.
        let col = quote_ident(&f.physical_column())?;
        return Ok(format!(
            "{col} uuid REFERENCES \"_media_assets\"(\"id\") ON DELETE SET NULL"
        ));
    }
```

> The referenced table is the literal `"_media_assets"` — do NOT route it through `table_name()` (which produces `ct_<x>`). Media is never `required`, so no `NOT NULL`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rustapi-sql media_single`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/ddl.rs
git commit -m "feat(sql): media single FK column DDL with ON DELETE SET NULL"
```

---

## Task 4: SQL — media join table DDL

**Files:**
- Modify: `crates/sql/src/ddl.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/sql/src/ddl.rs`:

```rust
    #[test]
    fn create_media_join_table_emits_ordered_table_and_index() {
        let (create, index) = create_media_join_table("post", "gallery").unwrap();
        assert_eq!(
            create,
            "CREATE TABLE \"j_media_post_gallery\" (\
\"post_id\" uuid NOT NULL REFERENCES \"ct_post\"(\"id\") ON DELETE CASCADE, \
\"asset_id\" uuid NOT NULL REFERENCES \"_media_assets\"(\"id\") ON DELETE CASCADE, \
\"position\" int NOT NULL, \
PRIMARY KEY (\"post_id\", \"asset_id\"))"
        );
        assert_eq!(
            index,
            "CREATE INDEX ON \"j_media_post_gallery\" (\"post_id\", \"position\")"
        );
    }

    #[test]
    fn drop_media_join_table_works() {
        assert_eq!(
            drop_media_join_table("post", "gallery").unwrap(),
            "DROP TABLE \"j_media_post_gallery\""
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rustapi-sql media_join`
Expected: FAIL — functions not found.

- [ ] **Step 3: Implement `create_media_join_table` + `drop_media_join_table`**

In `crates/sql/src/ddl.rs`, after `drop_join_table`, add (mirroring `create_join_table`):

```rust
/// Build the `CREATE TABLE` + `CREATE INDEX` statements for an ordered
/// multiple-media join table `j_media_<ct>_<field>`. Returns
/// `(create_table_sql, create_index_sql)`. The owner FK is `<ct>_id` (cascades
/// when the entry is deleted); `asset_id` references `_media_assets` and
/// cascades when the asset is deleted. `position` orders the gallery.
pub fn create_media_join_table(ct: &str, field: &str) -> Result<(String, String), DdlError> {
    let jt = crate::ident::media_join_table_name(ct, field)?;
    let owner_tbl = table_name(ct)?;
    let owner_col = quote_ident(&format!("{ct}_id"))?;
    let create = format!(
        "CREATE TABLE {jt} (\
{owner_col} uuid NOT NULL REFERENCES {owner_tbl}(\"id\") ON DELETE CASCADE, \
\"asset_id\" uuid NOT NULL REFERENCES \"_media_assets\"(\"id\") ON DELETE CASCADE, \
\"position\" int NOT NULL, \
PRIMARY KEY ({owner_col}, \"asset_id\"))"
    );
    let index = format!("CREATE INDEX ON {jt} ({owner_col}, \"position\")");
    Ok((create, index))
}

/// `DROP TABLE <media join table for ct.field>`.
pub fn drop_media_join_table(ct: &str, field: &str) -> Result<String, DdlError> {
    let jt = crate::ident::media_join_table_name(ct, field)?;
    Ok(format!("DROP TABLE {jt}"))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rustapi-sql media_join`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/ddl.rs
git commit -m "feat(sql): ordered media join table DDL"
```

---

## Task 5: SQL — media link DML (ordered insert/delete)

**Files:**
- Modify: `crates/sql/src/dml.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/sql/src/dml.rs`:

```rust
    #[test]
    fn insert_media_links_emits_positioned_unnest() {
        let id = Uuid::nil();
        let (sql, owner) = super::insert_media_links("post", "gallery", id).unwrap();
        assert_eq!(owner, id);
        assert_eq!(
            sql,
            "INSERT INTO \"j_media_post_gallery\" (\"post_id\", \"asset_id\", \"position\") \
SELECT $1::uuid, x.asset, x.ord::int FROM UNNEST($2::uuid[]) WITH ORDINALITY AS x(asset, ord)"
        );
    }

    #[test]
    fn delete_media_links_clears_owner() {
        let id = Uuid::nil();
        let (sql, owner) = super::delete_media_links("post", "gallery", id).unwrap();
        assert_eq!(owner, id);
        assert_eq!(sql, "DELETE FROM \"j_media_post_gallery\" WHERE \"post_id\" = $1::uuid");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rustapi-sql media_links`
Expected: FAIL — functions not found.

- [ ] **Step 3: Implement `insert_media_links` + `delete_media_links`**

In `crates/sql/src/dml.rs`, after `delete_links`, add (mirroring `insert_links`/`delete_links` but using `media_join_table_name` and `WITH ORDINALITY` so array order becomes `position`):

```rust
/// `INSERT INTO j_media_<ct>_<field> (<ct>_id, asset_id, position)` — replace-set
/// insert of a gallery in array order. `position` comes from `WITH ORDINALITY`
/// (1-based). Caller binds `$1` = owner id, `$2` = `uuid[]` of asset ids in order.
pub fn insert_media_links(ct: &str, field: &str, owner_id: Uuid) -> Result<(String, Uuid), DmlError> {
    let jt = crate::ident::media_join_table_name(ct, field)?;
    let owner_col = quote_ident(&format!("{ct}_id"))?;
    let sql = format!(
        "INSERT INTO {jt} ({owner_col}, \"asset_id\", \"position\") \
SELECT $1::uuid, x.asset, x.ord::int FROM UNNEST($2::uuid[]) WITH ORDINALITY AS x(asset, ord)"
    );
    Ok((sql, owner_id))
}

/// `DELETE FROM j_media_<ct>_<field> WHERE <ct>_id = $1::uuid` — clears a gallery
/// ahead of a replace-set re-insert. Caller binds `$1` = owner id.
pub fn delete_media_links(ct: &str, field: &str, owner_id: Uuid) -> Result<(String, Uuid), DmlError> {
    let jt = crate::ident::media_join_table_name(ct, field)?;
    let owner_col = quote_ident(&format!("{ct}_id"))?;
    let sql = format!("DELETE FROM {jt} WHERE {owner_col} = $1::uuid");
    Ok((sql, owner_id))
}
```

> Confirm `crate::ident::media_join_table_name` is reachable from `dml.rs` (the file already calls `join_table_name`); import path mirrors that existing usage.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rustapi-sql media_links`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/dml.rs
git commit -m "feat(sql): ordered media link insert/delete DML"
```

---

## Task 6: Schema service — wire media join tables into create/patch

**Files:**
- Modify: `crates/schema/src/service.rs`

This task has DB-backed integration tests. Follow the existing test style in `service.rs` (the `#[sqlx::test]` cases near the bottom, e.g. `create_rejects_self_referential_m2m`). If the suite requires a database, these run under the project's standard `cargo test` DB harness.

- [ ] **Step 1: Add a helper to create a media join table inside a txn**

Near `exec_create_join_table` (around line 340), add a sibling:

```rust
async fn exec_create_media_join_table(
    tx: &mut Transaction<'_, Postgres>,
    ct: &str,
    field: &str,
) -> Result<(), Error> {
    let (jt, idx) = rustapi_sql::create_media_join_table(ct, field)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    sqlx::query(&jt).execute(&mut **tx).await.map_err(map_db_err)?;
    sqlx::query(&idx).execute(&mut **tx).await.map_err(map_db_err)?;
    Ok(())
}
```

> Match `exec_create_join_table`'s exact signature/error handling in this file; the snippet above assumes it returns `Result<(), Error>` and takes `&mut Transaction`. Adjust to mirror the real one.

- [ ] **Step 2: Wire create-type (the `create` method)**

In the create-type join-table loop (currently lines ~74-80), extend it to also create media join tables. After the existing relation-m2m block inside `for f in &ct.fields { ... }`, add:

```rust
            if let Some(m) = f.media_meta() {
                if m.multiple {
                    exec_create_media_join_table(&mut tx, &ct.name, &f.name).await?;
                }
            }
```

- [ ] **Step 3: Wire patch add-fields**

In `patch`, the add-fields loop (lines ~128-138). The current loop does `if let Some(meta) = f.relation_meta() { if m2m { create join; continue } } add_column`. Add a media check before the `add_column` fallthrough:

```rust
        for f in &payload.add_fields {
            if let Some(meta) = f.relation_meta() {
                if meta.cardinality == Cardinality::ManyToMany {
                    exec_create_join_table(&mut tx, name, &f.name, &meta.target).await?;
                    continue;
                }
            }
            if let Some(m) = f.media_meta() {
                if m.multiple {
                    exec_create_media_join_table(&mut tx, name, &f.name).await?;
                    continue;
                }
            }
            let sql = rustapi_sql::add_column(name, f)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }
```

- [ ] **Step 4: Wire patch drop-fields**

In `patch`, the drop loop (lines ~111-127). Currently it branches on `is_m2m` (drop join table) vs else (drop column). Add a media-multiple branch. Replace the loop body with:

```rust
        for drop_name in &payload.drop_fields {
            let dropped = existing.fields.iter().find(|f| &f.name == drop_name);
            let is_m2m = dropped
                .and_then(|f| f.relation_meta())
                .map(|m| m.cardinality == Cardinality::ManyToMany)
                .unwrap_or(false);
            let is_multi_media = dropped
                .and_then(|f| f.media_meta())
                .map(|m| m.multiple)
                .unwrap_or(false);
            if is_m2m {
                let sql = rustapi_sql::drop_join_table(name, drop_name)
                    .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
            } else if is_multi_media {
                let sql = rustapi_sql::drop_media_join_table(name, drop_name)
                    .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
            } else {
                let sql = rustapi_sql::drop_column(name, drop_name)
                    .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
                sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
            }
        }
```

- [ ] **Step 5: Add an integration test**

Add near the other `#[sqlx::test]` cases (match their exact attribute + signature style — e.g. whether they take a `PgPool` argument):

```rust
    #[sqlx::test]
    async fn create_type_with_multi_media_makes_join_table(pool: PgPool) {
        let svc = service_for(&pool).await; // use whatever constructor the sibling tests use
        let ct = svc.create(NewContentType {
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![Field {
                name: "gallery".into(),
                kind: FieldKind::Media,
                required: false,
                unique: false,
                default: serde_json::Value::Null,
                max_length: None,
                kind_meta: serde_json::json!({"multiple": true}),
            }],
        }).await.unwrap();
        assert_eq!(ct.name, "post");
        // The join table exists.
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'j_media_post_gallery')"
        ).fetch_one(&pool).await.unwrap();
        assert!(exists.0);
    }
```

> Adapt `service_for` / construction to match the existing sibling tests in this file (they already build a `SchemaService` against `pool`). If they use a free helper, reuse it; do not invent a new constructor.

- [ ] **Step 6: Run schema tests**

Run: `cargo test -p rustapi-schema`
Expected: PASS (including the new media join-table test). If the DB harness is unavailable in this environment, at minimum `cargo build -p rustapi-schema` must succeed; note the DB test as deferred to CI.

- [ ] **Step 7: Commit**

```bash
git add crates/schema/src/service.rs
git commit -m "feat(schema): create/drop media join tables on type create and patch"
```

---

## Task 7: HTTP write — `MediaCheck` + `MediaLinkPlan` coercion in `entry.rs`

**Files:**
- Modify: `crates/http/src/entry.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/http/src/entry.rs`:

```rust
    fn ct_with_media() -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![
                Field {
                    name: "hero".into(),
                    kind: FieldKind::Media,
                    required: false, unique: false,
                    default: json!(null), max_length: None,
                    kind_meta: json!({"multiple": false}),
                },
                Field {
                    name: "gallery".into(),
                    kind: FieldKind::Media,
                    required: false, unique: false,
                    default: json!(null), max_length: None,
                    kind_meta: json!({"multiple": true}),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn media_single_uuid_coerces_and_registers_check() {
        let id = Uuid::new_v4();
        let body = serde_json::from_value::<Value>(json!({"hero": id.to_string()}))
            .unwrap().as_object().unwrap().clone();
        let (out, _checks, _links, media_checks, media_links) =
            body_to_binds(&ct_with_media(), body, true).unwrap();
        assert_eq!(out.get("hero").unwrap(), &BoundValue::Uuid(id));
        assert_eq!(media_checks.len(), 1);
        assert_eq!(media_checks[0].field, "hero");
        assert_eq!(media_checks[0].id, id);
        assert!(media_links.is_empty());
    }

    #[test]
    fn media_single_null_writes_typed_null_no_check() {
        let body = serde_json::from_value::<Value>(json!({"hero": Value::Null}))
            .unwrap().as_object().unwrap().clone();
        let (out, _c, _l, media_checks, _ml) =
            body_to_binds(&ct_with_media(), body, true).unwrap();
        assert_eq!(out.get("hero").unwrap(), &BoundValue::Null(FieldKind::Uuid));
        assert!(media_checks.is_empty());
    }

    #[test]
    fn media_single_bad_uuid_rejected() {
        let body = serde_json::from_value::<Value>(json!({"hero": "nope"}))
            .unwrap().as_object().unwrap().clone();
        assert!(matches!(body_to_binds(&ct_with_media(), body, true), Err(Error::Validation(_))));
    }

    #[test]
    fn media_multi_array_becomes_ordered_link_plan() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let body = serde_json::from_value::<Value>(json!({"gallery": [a.to_string(), b.to_string()]}))
            .unwrap().as_object().unwrap().clone();
        let (out, _c, _l, _mc, media_links) =
            body_to_binds(&ct_with_media(), body, true).unwrap();
        assert!(!out.contains_key("gallery"));
        assert_eq!(media_links.len(), 1);
        assert_eq!(media_links[0].field, "gallery");
        assert_eq!(media_links[0].ids, vec![a, b]);
        assert!(media_links[0].present);
    }

    #[test]
    fn media_multi_empty_array_is_clear() {
        let body = serde_json::from_value::<Value>(json!({"gallery": []}))
            .unwrap().as_object().unwrap().clone();
        let (_o, _c, _l, _mc, media_links) =
            body_to_binds(&ct_with_media(), body, true).unwrap();
        assert_eq!(media_links.len(), 1);
        assert!(media_links[0].ids.is_empty());
        assert!(media_links[0].present);
    }

    #[test]
    fn media_multi_dedups_preserving_order() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let body = serde_json::from_value::<Value>(json!({
            "gallery": [a.to_string(), b.to_string(), a.to_string()]
        })).unwrap().as_object().unwrap().clone();
        let (_o, _c, _l, _mc, media_links) =
            body_to_binds(&ct_with_media(), body, true).unwrap();
        assert_eq!(media_links[0].ids, vec![a, b]);
    }
```

> These tests assume `body_to_binds` now returns a 5-tuple. The existing relation/m2m tests destructure a 3-tuple `(out, checks, links)` and must be updated to ignore the two new fields (`, _media_checks, _media_links`). Update every existing `body_to_binds(...)` call site in this test module accordingly.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rustapi-http --lib entry`
Expected: FAIL — `MediaCheck`/`MediaLinkPlan` undefined; tuple arity mismatch.

- [ ] **Step 3: Add the new types + extend `BodyBinds`**

In `crates/http/src/entry.rs`, after `LinkPlan`, add:

```rust
/// One pending existence check for a single-media field value. Target is always
/// `_media_assets`, so (unlike RelationCheck) there is no `target` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaCheck {
    pub field: String,
    pub id: Uuid,
}

/// A pending ordered replace-set for a multiple-media field. `ids` is in gallery
/// order (deduped); empty means "clear all". `present` is always true when emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLinkPlan {
    pub field: String,
    pub ids: Vec<Uuid>,
    pub present: bool,
}
```

Extend the `BodyBinds` alias:

```rust
pub type BodyBinds = (
    BTreeMap<String, BoundValue>,
    Vec<RelationCheck>,
    Vec<LinkPlan>,
    Vec<MediaCheck>,
    Vec<MediaLinkPlan>,
);
```

- [ ] **Step 4: Implement the media branch in `body_to_binds`**

In `body_to_binds`, add `let mut media_checks: Vec<MediaCheck> = Vec::new();` and `let mut media_links: Vec<MediaLinkPlan> = Vec::new();` next to the existing `checks`/`links` declarations. Inside the `Some(v) => { ... }` arm, add a media branch before the relation branch (or right after it):

```rust
                if f.kind == FieldKind::Media {
                    let meta = f.media_meta().ok_or_else(|| {
                        Error::Validation(ValidationErrors::field(&f.name, "missing media kind_meta"))
                    })?;
                    if meta.multiple {
                        media_links.push(coerce_media_multi(f, v)?);
                    } else {
                        coerce_media_single(f, v, &mut out, &mut media_checks)?;
                    }
                    continue;
                }
```

Add the two helper functions near `coerce_relation` / `coerce_m2m`:

```rust
/// Coerce a single-media value (uuid string | null). Pushes a typed Uuid bind
/// under `f.name` (DML maps to `<name>_id`) and registers an existence check.
fn coerce_media_single(
    f: &Field,
    v: &Value,
    out: &mut BTreeMap<String, BoundValue>,
    checks: &mut Vec<MediaCheck>,
) -> Result<(), Error> {
    match v {
        Value::Null => {
            out.insert(f.name.clone(), BoundValue::Null(FieldKind::Uuid));
        }
        Value::String(s) => {
            let id = Uuid::parse_str(s).map_err(|_| {
                Error::Validation(ValidationErrors::field(&f.name, "invalid uuid"))
            })?;
            out.insert(f.name.clone(), BoundValue::Uuid(id));
            checks.push(MediaCheck { field: f.name.clone(), id });
        }
        _ => {
            return Err(Error::Validation(ValidationErrors::field(
                &f.name,
                "media value must be a uuid string or null",
            )));
        }
    }
    Ok(())
}

/// Parse a multiple-media value (array of uuid strings, order preserved, deduped).
/// Empty array is a valid "clear all".
fn coerce_media_multi(f: &Field, v: &Value) -> Result<MediaLinkPlan, Error> {
    let arr = v.as_array().ok_or_else(|| {
        Error::Validation(ValidationErrors::field(
            &f.name,
            "multiple media value must be an array of uuid strings",
        ))
    })?;
    let mut ids = Vec::with_capacity(arr.len());
    let mut seen = std::collections::HashSet::new();
    for item in arr {
        let s = item.as_str().ok_or_else(|| {
            Error::Validation(ValidationErrors::field(&f.name, "media ids must be strings"))
        })?;
        let id = Uuid::parse_str(s)
            .map_err(|_| Error::Validation(ValidationErrors::field(&f.name, "invalid uuid")))?;
        if seen.insert(id) {
            ids.push(id);
        }
    }
    Ok(MediaLinkPlan { field: f.name.clone(), ids, present: true })
}
```

Update the final `Ok((out, checks, links))` to `Ok((out, checks, links, media_checks, media_links))`.

- [ ] **Step 5: Add `decode_field` support for single media**

In `decode_field`, the `FieldKind::Media` currently falls into `_ => Ok(Value::Null)`. Add an explicit arm (place near the `Relation` arm) so single media decodes its `<name>_id` column to a bare uuid string (the embed pass overwrites it later):

```rust
        FieldKind::Media => {
            // Single media only reaches here (decode_field is only called for
            // stored columns). Surface the raw asset uuid; the media embed pass
            // replaces it with the asset object.
            let col = f.physical_column();
            let v: Option<Uuid> = row.try_get(col.as_str()).map_err(decode)?;
            Ok(v.map(|u| Value::String(u.to_string())).unwrap_or(Value::Null))
        }
```

- [ ] **Step 6: Update the two existing relation/m2m test call sites**

Every existing `body_to_binds(...)` call in this test module destructures `(out, checks, links)` or `(out, _checks, _links)`. Add two trailing ignores to each, e.g. `let (out, checks, _links, _mc, _ml) = ...`.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p rustapi-http --lib entry`
Expected: PASS. (Compilation will also fail in `routes/content.rs` until Task 9 — to keep this task green in isolation, run only the `--lib entry` unit tests here; the crate-wide build is fixed in Task 9.)

> If the crate does not compile because `content.rs` still destructures the old 3-tuple, that is expected and resolved in Task 9. If your harness requires a compiling crate to run any test, do Task 9's call-site edits together with this task before running, and commit both — but prefer keeping them separate if the harness allows `--lib` module tests to compile independently.

- [ ] **Step 8: Commit**

```bash
git add crates/http/src/entry.rs
git commit -m "feat(http): media write coercion (MediaCheck, MediaLinkPlan) and decode"
```

---

## Task 8: HTTP read — always-on media embed pass

**Files:**
- Create: `crates/http/src/media_embed.rs`
- Modify: `crates/http/src/lib.rs`

- [ ] **Step 1: Register the module**

In `crates/http/src/lib.rs`, add alongside the other `mod` declarations (e.g. near `mod populate;`):

```rust
mod media_embed;
```

- [ ] **Step 2: Write the module with a unit-testable grouping helper + failing test**

Create `crates/http/src/media_embed.rs`. Start with a pure helper that orders gallery asset ids per parent (unit-testable without a DB), plus the embed entry point that the handler calls:

```rust
//! Always-on media embed pass. Runs after `row_to_json` on every entry read
//! (single GET and list), replacing bare media ids with full asset objects.
//! Not gated by `?populate`. Single media -> object or null; multiple media ->
//! ordered array of asset objects.

use rustapi_core::{ContentType, Error, FieldKind};
use serde_json::{Map, Value};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

/// Group ordered (parent, asset_id) join rows into per-parent id lists, preserving
/// the order they arrive in (caller SELECTs `ORDER BY <ct>_id, position`).
pub fn group_gallery_ids(
    parents: &[Uuid],
    fetched: Vec<(Uuid, Uuid)>,
) -> HashMap<Uuid, Vec<Uuid>> {
    let mut out: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for p in parents {
        out.insert(*p, Vec::new());
    }
    for (p, asset) in fetched {
        out.entry(p).or_default().push(asset);
    }
    out
}
```

Add the failing test at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_gallery_preserves_order_and_seeds_empty() {
        let p1 = Uuid::new_v4();
        let p2 = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let grouped = group_gallery_ids(&[p1, p2], vec![(p1, a), (p1, b)]);
        assert_eq!(grouped.get(&p1).unwrap(), &vec![a, b]);
        assert!(grouped.get(&p2).unwrap().is_empty());
    }
}
```

- [ ] **Step 3: Run the test to verify it fails, then passes**

Run: `cargo test -p rustapi-http --lib media_embed`
Expected: FAIL first (module/file new — actually it should pass once the file compiles; if it compiles and passes, that confirms the helper). If the crate doesn't compile yet because `apply_media_embed` is referenced from `content.rs`, defer that wiring to Task 9 and only verify this module's own test compiles via `cargo test -p rustapi-http --lib media_embed`.

- [ ] **Step 4: Implement the asset-fetch + embed entry point**

Add to `media_embed.rs` the function the handler will call. It mirrors `populate::apply_forward` (batched id collection + one SELECT) but targets `_media_assets` and runs for every media field unconditionally:

```rust
/// Build an `AssetView`-shaped JSON object from an `_media_assets` row. Mirrors
/// the field set of `routes::media::AssetView`.
fn asset_row_to_json(row: &sqlx::postgres::PgRow) -> Result<(Uuid, Value), Error> {
    use chrono::{DateTime, Utc};
    let id: Uuid = row.try_get("id").map_err(internal)?;
    let folder_id: Option<Uuid> = row.try_get("folder_id").map_err(internal)?;
    let file_name: String = row.try_get("file_name").map_err(internal)?;
    let alt_text: Option<String> = row.try_get("alt_text").map_err(internal)?;
    let caption: Option<String> = row.try_get("caption").map_err(internal)?;
    let mime_type: String = row.try_get("mime_type").map_err(internal)?;
    let size_bytes: i64 = row.try_get("size_bytes").map_err(internal)?;
    let width: Option<i32> = row.try_get("width").map_err(internal)?;
    let height: Option<i32> = row.try_get("height").map_err(internal)?;
    let original_filename: String = row.try_get("original_filename").map_err(internal)?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(internal)?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at").map_err(internal)?;
    let mut m = Map::new();
    m.insert("id".into(), Value::String(id.to_string()));
    m.insert("folder_id".into(), folder_id.map(|u| Value::String(u.to_string())).unwrap_or(Value::Null));
    m.insert("file_name".into(), Value::String(file_name));
    m.insert("alt_text".into(), alt_text.map(Value::String).unwrap_or(Value::Null));
    m.insert("caption".into(), caption.map(Value::String).unwrap_or(Value::Null));
    m.insert("mime_type".into(), Value::String(mime_type));
    m.insert("size_bytes".into(), Value::Number(size_bytes.into()));
    m.insert("width".into(), width.map(|n| Value::Number(n.into())).unwrap_or(Value::Null));
    m.insert("height".into(), height.map(|n| Value::Number(n.into())).unwrap_or(Value::Null));
    m.insert("original_filename".into(), Value::String(original_filename));
    m.insert("created_at".into(), Value::String(created_at.to_rfc3339()));
    m.insert("updated_at".into(), Value::String(updated_at.to_rfc3339()));
    Ok((id, Value::Object(m)))
}

fn internal(e: impl Into<anyhow::Error>) -> Error {
    Error::Internal(e.into())
}

/// Embed all media fields on `rows` in place. For each content-type media field:
/// single -> replace the bare id with the asset object (or null); multiple ->
/// insert an ordered array of asset objects. One batched `_media_assets` SELECT
/// covers all referenced ids across single + multiple fields.
pub async fn apply_media_embed(
    pool: &PgPool,
    ct: &ContentType,
    rows: &mut [Map<String, Value>],
) -> Result<(), Error> {
    // Gather media fields.
    let media_fields: Vec<&rustapi_core::Field> =
        ct.fields.iter().filter(|f| f.kind == FieldKind::Media).collect();
    if media_fields.is_empty() {
        return Ok(());
    }

    // Parent ids for gallery lookups.
    let parent_ids: Vec<Uuid> = rows
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()))
        .collect();

    // Per-multiple-field gallery id lists, keyed by field name.
    let mut galleries: HashMap<String, HashMap<Uuid, Vec<Uuid>>> = HashMap::new();
    let mut all_asset_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    for f in &media_fields {
        let multiple = f.media_meta().map(|m| m.multiple).unwrap_or(false);
        if multiple {
            if parent_ids.is_empty() {
                galleries.insert(f.name.clone(), HashMap::new());
                continue;
            }
            let jt = rustapi_sql::media_join_table_name(&ct.name, &f.name)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            let owner_col = format!("{}_id", ct.name);
            let owner_q = rustapi_sql::quote_ident(&owner_col)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            let sql = format!(
                "SELECT {owner_q} AS parent, \"asset_id\" FROM {jt} WHERE {owner_q} = ANY($1) \
                 ORDER BY {owner_q}, \"position\""
            );
            let fetched = sqlx::query(&sql).bind(&parent_ids).fetch_all(pool).await.map_err(internal)?;
            let mut pairs: Vec<(Uuid, Uuid)> = Vec::with_capacity(fetched.len());
            for row in &fetched {
                let parent: Uuid = row.try_get("parent").map_err(internal)?;
                let asset: Uuid = row.try_get("asset_id").map_err(internal)?;
                all_asset_ids.insert(asset);
                pairs.push((parent, asset));
            }
            galleries.insert(f.name.clone(), group_gallery_ids(&parent_ids, pairs));
        } else {
            for r in rows.iter() {
                if let Some(Value::String(s)) = r.get(&f.name) {
                    if let Ok(u) = Uuid::parse_str(s) {
                        all_asset_ids.insert(u);
                    }
                }
            }
        }
    }

    // One batched asset fetch.
    let mut by_id: HashMap<Uuid, Value> = HashMap::new();
    if !all_asset_ids.is_empty() {
        let ids: Vec<Uuid> = all_asset_ids.into_iter().collect();
        let fetched = sqlx::query("SELECT * FROM \"_media_assets\" WHERE id = ANY($1)")
            .bind(&ids).fetch_all(pool).await.map_err(internal)?;
        for row in &fetched {
            let (id, obj) = asset_row_to_json(row)?;
            by_id.insert(id, obj);
        }
    }

    // Rewrite each row.
    for r in rows.iter_mut() {
        let pid = r.get("id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok());
        for f in &media_fields {
            let multiple = f.media_meta().map(|m| m.multiple).unwrap_or(false);
            if multiple {
                let list = pid
                    .and_then(|p| galleries.get(&f.name).and_then(|g| g.get(&p)))
                    .cloned()
                    .unwrap_or_default();
                let arr: Vec<Value> = list
                    .iter()
                    .filter_map(|id| by_id.get(id).cloned())
                    .collect();
                r.insert(f.name.clone(), Value::Array(arr));
            } else {
                let resolved = match r.get(&f.name) {
                    Some(Value::String(s)) => Uuid::parse_str(s)
                        .ok()
                        .and_then(|u| by_id.get(&u).cloned()),
                    _ => None,
                };
                r.insert(f.name.clone(), resolved.unwrap_or(Value::Null));
            }
        }
    }

    Ok(())
}
```

> Ensure `rustapi_sql::quote_ident` and `rustapi_sql::media_join_table_name` are `pub` (Tasks 2 confirms the latter; `quote_ident` is already used in `populate.rs`). Match `populate.rs`'s error-mapping idioms.

- [ ] **Step 5: Build the crate**

Run: `cargo build -p rustapi-http`
Expected: SUCCESS (module compiles; not yet called — call site lands in Task 9). If `apply_media_embed` triggers a dead-code warning, that's acceptable for this task; it's wired next.

- [ ] **Step 6: Run the module test**

Run: `cargo test -p rustapi-http --lib media_embed`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/http/src/media_embed.rs crates/http/src/lib.rs
git commit -m "feat(http): always-on media embed pass"
```

---

## Task 9: HTTP handler — wire media checks, link plans, and embed into `content.rs`

**Files:**
- Modify: `crates/http/src/routes/content.rs`

This is the integration task. It updates the `create`, `update`, `list`, and `get_one` handlers and adds two helpers. Prefer DB-backed integration tests in the existing http test style; if the DB harness is unavailable, a compiling crate + the unit tests from Tasks 7-8 are the gate, and the round-trip test is deferred to CI.

- [ ] **Step 1: Update `body_to_binds` destructuring in `create`**

In `create` (line ~103), change:

```rust
    let (binds_map, checks, links) = body_to_binds(&ct, body, true)?;
    verify_relation_targets_exist(&state, &checks).await?;
    verify_link_targets_exist(&state, &links).await?;
```

to:

```rust
    let (binds_map, checks, links, media_checks, media_links) = body_to_binds(&ct, body, true)?;
    verify_relation_targets_exist(&state, &checks).await?;
    verify_link_targets_exist(&state, &links).await?;
    verify_media_targets_exist(&state, &media_checks).await?;
    verify_media_link_targets_exist(&state, &media_links).await?;
```

After `write_links(&mut tx, &ct.name, &links, new_id).await?;` add:

```rust
    write_media_links(&mut tx, &ct.name, &media_links, new_id).await?;
```

- [ ] **Step 2: Update `update` the same way**

In `update` (line ~162), change the destructure to the 5-tuple and add the two verify calls (mirroring Step 1). After `write_links(&mut tx, &ct.name, &links, id).await?;` add:

```rust
    write_media_links(&mut tx, &ct.name, &media_links, id).await?;
```

The PUT full-replace null-fill loop (lines ~173-187) already skips non-stored columns via `!f.is_stored_column()`, so multiple-media (not stored) is correctly skipped. Single media IS stored; when absent from the body it should be nulled like a relation. Update the `null_kind` computation to treat media like relation:

```rust
            let null_kind = if f.kind == rustapi_core::FieldKind::Relation
                || f.kind == rustapi_core::FieldKind::Media
            {
                rustapi_core::FieldKind::Uuid
            } else {
                f.kind
            };
```

- [ ] **Step 3: Add the media verify + write helpers**

After `write_links` (line ~475), add:

```rust
/// Existence pre-check for single-media ids. All target `_media_assets`, so one
/// batched SELECT covers every check. Returns 422 naming the first field with a
/// missing id (payload order).
async fn verify_media_targets_exist(
    state: &AppState,
    checks: &[crate::entry::MediaCheck],
) -> Result<(), ApiError> {
    if checks.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = checks.iter().map(|c| c.id).collect();
    let rows = sqlx::query("SELECT id FROM \"_media_assets\" WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_all(&state.pool)
        .await
        .map_err(db)?;
    let mut found = std::collections::HashSet::new();
    for r in &rows {
        let id: Uuid = r.try_get("id").map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
        found.insert(id);
    }
    let mut current_field: Option<&str> = None;
    let mut missing: Vec<String> = Vec::new();
    for c in checks {
        if !found.contains(&c.id) {
            match current_field {
                None => { current_field = Some(&c.field); missing.push(c.id.to_string()); }
                Some(name) if name == c.field => missing.push(c.id.to_string()),
                Some(_) => break,
            }
        }
    }
    if let Some(field) = current_field {
        return Err(ApiError(Error::Validation(
            ValidationErrors::relation_target_missing(field, missing),
        )));
    }
    Ok(())
}

/// Existence pre-check for multiple-media ids, per field. One batched SELECT per
/// field against `_media_assets`.
async fn verify_media_link_targets_exist(
    state: &AppState,
    links: &[crate::entry::MediaLinkPlan],
) -> Result<(), ApiError> {
    for plan in links {
        if plan.ids.is_empty() {
            continue;
        }
        let rows = sqlx::query("SELECT id FROM \"_media_assets\" WHERE id = ANY($1)")
            .bind(&plan.ids)
            .fetch_all(&state.pool)
            .await
            .map_err(db)?;
        let mut found = std::collections::HashSet::new();
        for r in &rows {
            let id: Uuid = r.try_get("id").map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e))))?;
            found.insert(id);
        }
        let missing: Vec<String> = plan.ids.iter()
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

/// Apply each multiple-media replace-set inside the txn: clear the gallery, then
/// insert the supplied asset ids in order (position = array order via ORDINALITY).
async fn write_media_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    owner_type: &str,
    links: &[crate::entry::MediaLinkPlan],
    owner_id: Uuid,
) -> Result<(), ApiError> {
    for plan in links {
        if !plan.present {
            continue;
        }
        let (del_sql, _) = rustapi_sql::delete_media_links(owner_type, &plan.field, owner_id)
            .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
        sqlx::query(&del_sql).bind(owner_id).execute(&mut **tx).await.map_err(db)?;
        if plan.ids.is_empty() {
            continue;
        }
        let (ins_sql, _) = rustapi_sql::insert_media_links(owner_type, &plan.field, owner_id)
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

- [ ] **Step 4: Invoke the embed pass on every read**

In `list` (after the `if let Some(raw) = populate_param ...` block, ~line 76), add an unconditional embed call:

```rust
    crate::media_embed::apply_media_embed(&state.pool, &ct, &mut maps).await.map_err(ApiError)?;
```

> Place it AFTER `apply_populate` so populate's relation hydration and media embed don't fight; media fields are independent of relation fields, so order between them is immaterial, but keep embed last for clarity.

In `get_one`, the single-row path currently only runs populate when `?populate` is set. Always run embed. After the populate `if let` block (~line 150), add:

```rust
    {
        let mut one = vec![std::mem::take(&mut map)];
        crate::media_embed::apply_media_embed(&state.pool, &ct, &mut one).await.map_err(ApiError)?;
        map = one.pop().unwrap_or_default();
    }
```

> `create` and `update` return `row_to_json(...)` directly (bare media ids, no embed) — that matches relation's behavior (POST/PUT responses are not populated). Leave them as bare ids; the client re-fetches or the next GET embeds. If the spec's "return asset metadata" must also apply to the write response, additionally embed there; per the spec, embed is a read concern, so this plan embeds on GET/list only.

- [ ] **Step 5: Confirm `ValidationErrors::relation_target_missing` is the right error**

The media verify helpers reuse `relation_target_missing` for a consistent 422 shape. This is intentional — the field name is what the client needs. No new error variant required.

- [ ] **Step 6: Build + run the http crate tests**

Run: `cargo build -p rustapi-http && cargo test -p rustapi-http`
Expected: SUCCESS + PASS (unit tests from Tasks 7-8 plus any existing http tests). DB round-trip tests (below) require the DB harness.

- [ ] **Step 7: Add a DB round-trip integration test (http test style)**

In the http integration test module that exercises entry CRUD against a live pool (match the existing pattern — find a test that creates a content type then POSTs an entry), add a media round-trip. Pseudocode shape to adapt:

```rust
    #[sqlx::test]
    async fn media_single_and_multi_round_trip(pool: PgPool) {
        // 1. Set up app/state + an authed principal exactly like sibling tests.
        // 2. Upload two assets via the media routes (or insert _media_assets rows directly).
        // 3. Create a content type "post" with fields:
        //      hero    media {multiple:false}
        //      gallery media {multiple:true}
        // 4. POST /content/post { "hero": <a1>, "gallery": [<a2>, <a1>] }
        // 5. GET  /content/post/<id>  (no ?populate)
        //    assert body["hero"] is an object with "id" == a1 and "mime_type" present.
        //    assert body["gallery"] is an array of two objects, ids == [a2, a1] in order.
        // 6. DELETE the asset a1 via the media route.
        // 7. GET the entry again:
        //    assert body["hero"] is null (SET NULL).
        //    assert body["gallery"] is a one-element array [a2] (cascade dropped a1).
    }
```

> Implement the steps concretely using the helpers the existing http tests already provide (router builder, auth header, JSON request helpers). If those helpers don't exist for media, insert `_media_assets` rows directly with `sqlx::query` to seed assets.

- [ ] **Step 8: Run the integration test**

Run: `cargo test -p rustapi-http media_single_and_multi_round_trip`
Expected: PASS (with DB). If no DB harness, mark deferred to CI and ensure the crate builds.

- [ ] **Step 9: Commit**

```bash
git add crates/http/src/routes/content.rs
git commit -m "feat(http): wire media checks, ordered link plans, and embed into entry CRUD"
```

---

## Task 10: UI types — `media` kind + `MediaMeta`

**Files:**
- Modify: `ui/src/api/types.ts`

- [ ] **Step 1: Add `"media"` to the `FieldKind` union**

In the `FieldKind` union, add `| "media"` (after `| "slug"`).

- [ ] **Step 2: Add `MediaMeta` + accessor**

After the `EnumMeta` / `enumValues` block, add:

```ts
// Media kind_meta shape (when kind === "media").
export interface MediaMeta {
  multiple: boolean;
}

export function mediaMeta(f: Field): MediaMeta | null {
  if (f.kind !== "media") return null;
  const m = f.kind_meta as Partial<MediaMeta>;
  return { multiple: m.multiple === true };
}
```

- [ ] **Step 3: Typecheck**

Run: `cd ui && npx tsc --noEmit`
Expected: SUCCESS (no type errors introduced).

- [ ] **Step 4: Commit**

```bash
git add ui/src/api/types.ts
git commit -m "feat(ui): media FieldKind and mediaMeta accessor"
```

---

## Task 11: UI builder model — media draft field

**Files:**
- Modify: `ui/src/builder/draftModel.ts`

- [ ] **Step 1: Add `"media"` to `KINDS`**

Change the `KINDS` array to include `"media"`:

```ts
export const KINDS: FieldKind[] = [
  "string", "text", "integer", "float", "boolean", "datetime",
  "relation", "media", "enum", "json", "email", "url", "slug",
];
```

- [ ] **Step 2: Add `mediaMultiple` to `DraftField`**

In the `DraftField` interface, add (after `cardinality`):

```ts
  mediaMultiple: boolean;        // kind === "media"
```

- [ ] **Step 3: Seed it in `blankField` and `seedFromContentType`**

In `blankField()` add `mediaMultiple: false,`.

In `seedFromContentType`, import `mediaMeta`:

```ts
import { enumValues, relationMeta, mediaMeta } from "../api/types";
```

and in the field map add `mediaMultiple: mediaMeta(f)?.multiple ?? false,`.

- [ ] **Step 4: Build `kind_meta` for media in `draftFieldToField`**

In `draftFieldToField`, extend the meta branch:

```ts
  if (d.kind === "relation") {
    kind_meta = {
      target: d.target,
      cardinality: d.cardinality,
      ...(d.inverse ? { inverse: d.inverse } : {}),
    };
  } else if (d.kind === "enum") {
    kind_meta = { values: d.enumValues };
  } else if (d.kind === "media") {
    kind_meta = { multiple: d.mediaMultiple };
  }
```

- [ ] **Step 5: Typecheck**

Run: `cd ui && npx tsc --noEmit`
Expected: SUCCESS.

- [ ] **Step 6: Commit**

```bash
git add ui/src/builder/draftModel.ts
git commit -m "feat(ui): media draft field model and kind_meta build"
```

---

## Task 12: UI builder modal — media config block + required guard

**Files:**
- Modify: `ui/src/builder/FieldConfigModal.tsx`

- [ ] **Step 1: Generalize the required-blocked guard**

Change:

```tsx
  const m2mRequiredBlocked = field.kind === "relation" && field.cardinality === "many_to_many";
```

to also cover all media fields (media is never required):

```tsx
  const requiredBlocked =
    (field.kind === "relation" && field.cardinality === "many_to_many") ||
    field.kind === "media";
```

Then replace every later `m2mRequiredBlocked` reference with `requiredBlocked`, and in `save()`:

```tsx
    if (requiredBlocked) out.required = false;
```

In the advanced-tab "Required field" hint text, update the message to reflect both cases:

```tsx
                  <span>
                    {requiredBlocked
                      ? (field.kind === "media"
                          ? "Media fields cannot be required."
                          : "Many-to-many relations cannot be required.")
                      : "The entry can't be saved while this is empty."}
                  </span>
```

and the Toggle:

```tsx
                <Toggle
                  on={field.required && !requiredBlocked}
                  disabled={locked || requiredBlocked}
                  onChange={(v) => set({ required: v })}
                />
```

- [ ] **Step 2: Add the media config block (basic tab)**

After the `field.kind === "enum"` block in the basic tab, add:

```tsx
              {field.kind === "media" && (
                <div className="rs-field">
                  <div className="rs-field-label"><label>Selection</label></div>
                  <div className="rs-setting-row">
                    <div className="rs-setting-meta">
                      <strong>Allow multiple assets</strong>
                      <span>Pick a gallery of assets instead of a single one.</span>
                    </div>
                    <Toggle
                      on={field.mediaMultiple}
                      disabled={locked}
                      onChange={(v) => set({ mediaMultiple: v })}
                    />
                  </div>
                </div>
              )}
```

- [ ] **Step 3: Use the image icon for media field type**

Change the icon selection line:

```tsx
  const I = Icons[field.kind === "relation" ? "relation" : field.kind === "media" ? "image" : "type"];
```

> Confirm `Icons.image` exists (it's used in `MediaLibrary.tsx`). If the `Icons` map's index signature complains, fall back to a small `switch` or cast as the existing code does.

- [ ] **Step 4: Typecheck**

Run: `cd ui && npx tsc --noEmit`
Expected: SUCCESS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/builder/FieldConfigModal.tsx
git commit -m "feat(ui): media field config block and required guard in builder"
```

---

## Task 13: UI — AssetPicker modal

**Files:**
- Create: `ui/src/screens/media/AssetPicker.tsx`

- [ ] **Step 1: Implement the picker**

Create `ui/src/screens/media/AssetPicker.tsx`. It browses folders/assets (reusing `listFolders`/`listAssets` and `AssetThumb`) and returns the chosen asset(s). Single mode selects-and-closes; multiple mode multi-selects in click order then confirms.

```tsx
import { useCallback, useEffect, useMemo, useState } from "react";
import { Icons } from "../../components/icons";
import { listFolders, listAssets } from "../../api/endpoints";
import type { MediaFolder, MediaAsset } from "../../api/types";
import { AssetThumb } from "./AssetThumb";
import { Checkbox } from "./Checkbox";

export function AssetPicker({
  multiple,
  onClose,
  onPick,
}: {
  multiple: boolean;
  onClose: () => void;
  onPick: (assets: MediaAsset[]) => void;
}) {
  const [folders, setFolders] = useState<MediaFolder[]>([]);
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [cur, setCur] = useState<string | null>(null);
  const [picked, setPicked] = useState<MediaAsset[]>([]);

  useEffect(() => { listFolders({ all: true }).then(setFolders).catch(() => {}); }, []);
  const reload = useCallback((folderId: string | null) => {
    listAssets(folderId).then(setAssets).catch(() => {});
  }, []);
  useEffect(() => { reload(cur); }, [cur, reload]);

  const path = useMemo(() => {
    const chain: MediaFolder[] = [];
    let id: string | null = cur;
    const byId = new Map(folders.map((f) => [f.id, f]));
    while (id != null) {
      const f = byId.get(id);
      if (!f) break;
      chain.unshift(f);
      id = f.parent_id;
    }
    return chain;
  }, [cur, folders]);

  const subFolders = folders.filter((f) => f.parent_id === cur);

  const pickedIds = new Set(picked.map((a) => a.id));
  const toggle = (a: MediaAsset) => {
    if (!multiple) { onPick([a]); return; }
    setPicked((p) => (pickedIds.has(a.id) ? p.filter((x) => x.id !== a.id) : [...p, a]));
  };

  return (
    <div className="rs-modal-backdrop" onClick={onClose}>
      <div className="rs-modal rs-modal--wide" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <div className="rs-modal-head">
          <div className="rs-modal-icon"><Icons.image size={18} /></div>
          <div className="rs-modal-titles">
            <span className="rs-modal-eyebrow">Media Library</span>
            <h2>{multiple ? "Select assets" : "Select an asset"}</h2>
          </div>
          <button className="rs-modal-x" onClick={onClose}><Icons.x size={18} /></button>
        </div>

        <div className="rs-media-bc">
          {path.length === 0
            ? <span className="rs-media-bc-here">Media Library</span>
            : <button onClick={() => setCur(null)} type="button">Media Library</button>}
          {path.map((f, i) => (
            <span key={f.id} style={{ display: "contents" }}>
              <span className="rs-media-bc-sep">/</span>
              {i === path.length - 1
                ? <span className="rs-media-bc-here">{f.name}</span>
                : <button onClick={() => setCur(f.id)} type="button">{f.name}</button>}
            </span>
          ))}
        </div>

        <div className="rs-modal-body">
          {subFolders.length > 0 && (
            <div className="rs-folder-grid">
              {subFolders.map((f) => (
                <div key={f.id} className="rs-folder-card" onClick={() => setCur(f.id)}>
                  <span className="rs-folder-ico"><Icons.folder size={22} /></span>
                  <span className="rs-folder-meta"><strong title={f.name}>{f.name}</strong></span>
                </div>
              ))}
            </div>
          )}
          {assets.length === 0 ? (
            <div className="rs-media-empty"><p>No assets in this folder.</p></div>
          ) : (
            <div className="rs-media-grid">
              {assets.map((m) => {
                const sel = pickedIds.has(m.id);
                return (
                  <div className={"rs-media-card" + (sel ? " is-selected" : "")} key={m.id} onClick={() => toggle(m)}>
                    {multiple && (
                      <div className="rs-media-check" onClick={(e) => { e.stopPropagation(); toggle(m); }}>
                        <Checkbox checked={sel} onChange={() => toggle(m)} />
                      </div>
                    )}
                    <AssetThumb asset={m} />
                    <div className="rs-media-card-meta">
                      <span className="rs-media-card-text"><strong title={m.file_name}>{m.file_name}</strong></span>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {multiple && (
          <div className="rs-modal-foot">
            <button className="rs-btn rs-btn--ghost" onClick={onClose}>Cancel</button>
            <div className="rs-spacer" />
            <button className="rs-btn rs-btn--primary" disabled={picked.length === 0} onClick={() => onPick(picked)}>
              <Icons.check size={15} /> Add {picked.length || ""} asset{picked.length === 1 ? "" : "s"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
```

> Reuse the exact class names from `MediaLibrary.tsx` so styling is consistent. Confirm `Checkbox` is exported from `./Checkbox` (it is, used by MediaLibrary).

- [ ] **Step 2: Typecheck**

Run: `cd ui && npx tsc --noEmit`
Expected: SUCCESS.

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/media/AssetPicker.tsx
git commit -m "feat(ui): AssetPicker modal for browsing and selecting media"
```

---

## Task 14: UI — `MediaField` input in the entry editor

**Files:**
- Modify: `ui/src/screens/EntryEditor.tsx`

- [ ] **Step 1: Import the picker, types, and asset endpoints**

Add imports at the top of `EntryEditor.tsx`:

```tsx
import { mediaMeta } from "../api/types";
import type { MediaAsset } from "../api/types";
import { AssetPicker } from "./media/AssetPicker";
import { AssetThumb } from "./media/AssetThumb";
import { listAssets } from "../api/endpoints";
```

- [ ] **Step 2: Route the `media` kind in `FieldInput`**

In the `FieldInput` switch, add before `default:`:

```tsx
    case "media":
      return <MediaField field={field} value={value} onChange={onChange} />;
```

- [ ] **Step 3: Implement `MediaField`**

Add at the bottom of the file. It normalizes the seeded value (which, from the embed read shape, is an asset object or array of objects) into the current asset list, renders thumbnails, opens the picker, and emits bare id(s) via `onChange`.

```tsx
function MediaField({
  field,
  value,
  onChange,
}: {
  field: Field;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const multiple = mediaMeta(field)?.multiple ?? false;
  const [open, setOpen] = useState(false);
  const [assets, setAssets] = useState<MediaAsset[]>([]);

  // Seed from the embedded read shape: object | array<object> | id | id[] | "".
  useEffect(() => {
    let cancelled = false;
    const seed = async () => {
      if (value === "" || value == null) { setAssets([]); return; }
      const items = Array.isArray(value) ? value : [value];
      const objects = items.filter((x): x is MediaAsset => typeof x === "object" && x !== null && "id" in (x as object));
      if (objects.length === items.length) {
        setAssets(objects);
        return;
      }
      // Some/all are bare ids — resolve by scanning the library (no by-id list
      // endpoint exists; this path is rare since reads embed objects).
      const ids = items.map((x) => (typeof x === "string" ? x : (x as MediaAsset)?.id)).filter(Boolean) as string[];
      try {
        const all = await listAssets(null);
        if (cancelled) return;
        const byId = new Map(all.map((a) => [a.id, a]));
        setAssets(ids.map((id) => byId.get(id)).filter((a): a is MediaAsset => !!a));
      } catch {
        if (!cancelled) setAssets([]);
      }
    };
    seed();
    return () => { cancelled = true; };
  }, [value]);

  const emit = (next: MediaAsset[]) => {
    setAssets(next);
    onChange(multiple ? next.map((a) => a.id) : (next[0]?.id ?? null));
  };

  const onPick = (picked: MediaAsset[]) => {
    setOpen(false);
    if (multiple) {
      const existing = new Set(assets.map((a) => a.id));
      emit([...assets, ...picked.filter((p) => !existing.has(p.id))]);
    } else {
      emit(picked.slice(0, 1));
    }
  };

  const remove = (id: string) => emit(assets.filter((a) => a.id !== id));
  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= assets.length) return;
    const next = assets.slice();
    [next[i], next[j]] = [next[j], next[i]];
    emit(next);
  };

  return (
    <div className="rs-media-field">
      {assets.length === 0 ? (
        <div className="rs-media-field-empty">No asset selected.</div>
      ) : (
        <div className="rs-media-field-strip">
          {assets.map((a, i) => (
            <div className="rs-media-field-item" key={a.id}>
              <AssetThumb asset={a} />
              <span className="rs-media-field-name" title={a.file_name}>{a.file_name}</span>
              <div className="rs-media-field-actions">
                {multiple && (
                  <>
                    <button type="button" className="rs-link-btn" disabled={i === 0} onClick={() => move(i, -1)}>↑</button>
                    <button type="button" className="rs-link-btn" disabled={i === assets.length - 1} onClick={() => move(i, 1)}>↓</button>
                  </>
                )}
                <button type="button" className="rs-link-btn rs-danger" onClick={() => remove(a.id)}>Remove</button>
              </div>
            </div>
          ))}
        </div>
      )}
      <button type="button" className="rs-btn rs-btn--ghost" onClick={() => setOpen(true)}>
        <Icons.image size={15} /> {multiple ? "Add assets" : assets.length ? "Replace asset" : "Choose asset"}
      </button>
      {open && <AssetPicker multiple={multiple} onClose={() => setOpen(false)} onPick={onPick} />}
    </div>
  );
}
```

- [ ] **Step 4: Fix the save body-builder so media values pass through**

In `save()`, the loop skips empty strings and coerces numbers/json. Media values are already ids (string | null) or id arrays, set via `onChange`. Add a media passthrough before the generic `else`:

```tsx
    for (const f of ct.fields) {
      const v = form[f.name];
      if (f.kind === "media") {
        // v is an id string, an id array, or null — send as-is (skip empty single).
        if (Array.isArray(v)) { body[f.name] = v; }
        else if (v == null || v === "") { /* omit single media when unset */ }
        else { body[f.name] = v; }
        continue;
      }
      if (v === "" || v === undefined) continue;
      if (f.kind === "integer" || f.kind === "float") {
        body[f.name] = Number(v);
      } else if (f.kind === "json") {
        // ...unchanged...
      } else {
        body[f.name] = v;
      }
    }
```

> For multiple media, always send the array (even empty `[]`) so a cleared gallery persists. For single media, omit when unset (matches how other optional fields behave) — or send `null` if you want an explicit clear on update. Sending `[]` for multi and omitting empty single is the chosen behavior.

- [ ] **Step 5: Seed form value correctly for media on load**

The form seed (`useEffect` at line ~35) sets `seed[f.name] = existing.data ? existing.data[f.name] ?? "" : ""`. For media this puts the embedded object/array into the form, which `MediaField` handles. No change needed — `MediaField`'s effect normalizes it. Confirm by inspection.

- [ ] **Step 6: Typecheck + build**

Run: `cd ui && npx tsc --noEmit && npm run build`
Expected: SUCCESS.

- [ ] **Step 7: Commit**

```bash
git add ui/src/screens/EntryEditor.tsx
git commit -m "feat(ui): MediaField asset picker input in entry editor"
```

---

## Task 15: Full verification + manual smoke test

**Files:** none (verification only)

- [ ] **Step 1: Run the full Rust test suite**

Run: `cargo test`
Expected: PASS across `rustapi-core`, `rustapi-sql`, `rustapi-schema`, `rustapi-http`. Note any DB-gated tests skipped if no DB.

- [ ] **Step 2: Run the UI typecheck + build**

Run: `cd ui && npx tsc --noEmit && npm run build`
Expected: SUCCESS.

- [ ] **Step 3: Manual smoke (against a running instance, optional but recommended)**

Using the `/run` flow or the project's dev command:
1. Create a content type with a `hero` media (single) and a `gallery` media (multiple) field — confirm the builder shows the "Allow multiple assets" toggle and hides "Required".
2. Upload a couple of assets in the Media Library.
3. Create an entry, open the asset picker for `hero` (single → selects + closes) and `gallery` (multiple → checkbox select + Add), confirm thumbnails render. Save.
4. Reopen the entry — thumbnails persist (read embeds objects).
5. In the Media Library, delete an asset referenced by both fields. Reopen the entry: `hero` is empty (SET NULL), the gallery dropped that item (cascade).

- [ ] **Step 4: Final commit (if any verification fixups were needed)**

```bash
git add -A
git commit -m "chore(media-field): verification fixups"
```

---

## Notes for the Implementer

- **Tuple arity break:** Task 7 changes `body_to_binds` to a 5-tuple. The compiler will flag every call site (`entry.rs` tests and `content.rs`). Task 9 fixes `content.rs`. If your harness can't run `--lib` module tests on a non-compiling crate, do Tasks 7 and 9's call-site edits together before running tests, but keep the commits separate per task.
- **`_media_assets` is a fixed table name** — never route it through `table_name()`. Quote it as the literal `"_media_assets"`.
- **Media is never required** — the single FK column is always nullable; this is what lets `ON DELETE SET NULL` work without blocking asset deletes.
- **Reuse, don't duplicate UI:** AssetPicker and MediaField reuse `AssetThumb`, `Checkbox`, the `rs-media-*` / `rs-folder-*` / `rs-modal-*` classes, and `listFolders`/`listAssets` — no new endpoints, no new CSS required beyond optional `rs-media-field*` polish.
- **Write responses stay bare:** POST/PUT return bare ids (like relation); embed runs on GET/list only. The UI seeds from GET, so editors show thumbnails.
