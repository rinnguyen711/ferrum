# Draft & Publish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-content-type Draft & Publish: a system-managed nullable `published_at` column distinguishes draft (NULL) from published (set), with explicit publish/unpublish endpoints and UI.

**Architecture:** Model A — one row per entry. A type-level flag `options.draft_publish` (jsonb on `_content_types`) controls whether the `ct_<name>` table gets a `published_at TIMESTAMPTZ` system column. Reads filter by `?status=`; writes never touch `published_at` (only `/publish` + `/unpublish` do). Enabled by default on create; can be enabled on existing types via PATCH; disabling is rejected in v1.

**Tech Stack:** Rust (axum, sqlx, Postgres), React + TypeScript (Vite).

Spec: `docs/superpowers/specs/2026-06-04-draft-publish-design.md`.

**Conventions in this repo (read before starting):**
- Backend tests are inline `#[cfg(test)] mod tests` in each crate file; HTTP integration tests live in `crates/bin/tests/`.
- Run a single Rust test: `cargo test -p <crate> <test_name>`. Whole workspace: `cargo test`.
- `ContentType` is in `crates/core/src/content_type.rs`. `RawCt` (DB row) is in `crates/schema/src/registry.rs`.
- System columns are listed in `crates/core/src/system.rs` and are NOT members of `ct.fields`.
- Frontend typecheck: `cd ui && npx tsc --noEmit`.

---

## Task 1: Add `options` to ContentType core model

**Files:**
- Modify: `crates/core/src/content_type.rs`
- Modify: `crates/core/src/reserved.rs:5-9`

- [ ] **Step 1: Add reserved name test**

In `crates/core/src/reserved.rs`, add to the `tests` mod:

```rust
    #[test]
    fn published_at_is_reserved() {
        assert!(is_reserved("published_at"));
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p rustapi-core published_at_is_reserved`
Expected: FAIL (`published_at` not in list).

- [ ] **Step 3: Add to reserved list**

In `crates/core/src/reserved.rs`, add `"published_at"` to `RESERVED_FIELD_NAMES`:

```rust
pub const RESERVED_FIELD_NAMES: &[&str] = &[
    "id", "created_at", "updated_at", "published_at",
    "select", "from", "where", "table", "order", "group", "having",
    "user", "null", "true", "false", "default", "primary", "foreign", "index",
];
```

- [ ] **Step 4: Run, verify pass**

Run: `cargo test -p rustapi-core published_at_is_reserved`
Expected: PASS.

- [ ] **Step 5: Add `options` field + helper with test**

In `crates/core/src/content_type.rs`, add a test to the `tests` mod:

```rust
    #[test]
    fn draft_publish_defaults_and_reads() {
        use serde_json::json;
        let mut ct = ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields: vec![field("title")],
            options: json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        // Absent → false (existing pre-migration types read as off).
        assert!(!ct.draft_publish());
        ct.options = json!({ "draft_publish": true });
        assert!(ct.draft_publish());
    }
```

Note: the existing `tests` mod's `field()` helper and `Utc`/`Uuid` imports already exist; if `Utc` is not imported in the test mod, add `use chrono::Utc;` and `use uuid::Uuid;` there.

- [ ] **Step 6: Run, verify fail**

Run: `cargo test -p rustapi-core draft_publish_defaults_and_reads`
Expected: FAIL (no `options` field / no `draft_publish()`).

- [ ] **Step 7: Add field + helper + NewContentType default**

In `crates/core/src/content_type.rs`:

Add `options` to `ContentType` (after `fields`):

```rust
pub struct ContentType {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub options: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

Add `options` to `NewContentType`:

```rust
pub struct NewContentType {
    pub name: String,
    pub display_name: String,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub options: serde_json::Value,
}
```

Add an impl on `ContentType` (place after the struct):

```rust
impl ContentType {
    /// Whether Draft & Publish is enabled. Absent/invalid `options` → false.
    pub fn draft_publish(&self) -> bool {
        self.options
            .get("draft_publish")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}
```

Add a method on `NewContentType` that normalizes `options` so create defaults D&P **on** when the key is absent:

```rust
impl NewContentType {
    /// Resolve effective options for create: `draft_publish` defaults to true
    /// when the client omitted it. Returns a normalized jsonb object.
    pub fn resolved_options(&self) -> serde_json::Value {
        let dp = self
            .options
            .get("draft_publish")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        serde_json::json!({ "draft_publish": dp })
    }
}
```

- [ ] **Step 8: Fix existing constructors in tests**

Every place that constructs `ContentType { ... }` now needs `options`. These compile-fail until fixed. Add `options: serde_json::json!({})` (or `serde_json::Value::Null`) to each. Find them:

Run: `cargo build -p rustapi-core 2>&1 | grep -n "content_type.rs\|missing field .options"`

Fix each flagged `ContentType { ... }` literal in `crates/core/src/content_type.rs` tests by adding `options: serde_json::json!({}),` before `created_at`.

- [ ] **Step 9: Run core tests**

Run: `cargo test -p rustapi-core`
Expected: PASS (note: other crates won't build yet — that's expected; fix them in later tasks).

- [ ] **Step 10: Commit**

```bash
git add crates/core/src/content_type.rs crates/core/src/reserved.rs
git commit -m "feat(core): add ContentType.options + draft_publish helper, reserve published_at"
```

---

## Task 2: Emit `published_at` column in DDL

**Files:**
- Modify: `crates/sql/src/ddl.rs`

- [ ] **Step 1: Add test for create_table with D&P**

In `crates/sql/src/ddl.rs` `tests` mod, the helper `ct(fields)` builds a `ContentType`. It will need `options` after Task 1 — update the helper first:

```rust
    fn ct(fields: Vec<Field>) -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: "post".into(),
            display_name: "Post".into(),
            fields,
            options: json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
```

Then add tests:

```rust
    #[test]
    fn create_table_emits_published_at_when_draft_publish() {
        let mut c = ct(vec![field("title", FieldKind::String)]);
        c.options = json!({ "draft_publish": true });
        let sql = create_table(&c).unwrap();
        assert!(sql.contains("\"published_at\" TIMESTAMPTZ"), "got: {sql}");
    }

    #[test]
    fn create_table_omits_published_at_when_disabled() {
        let sql = create_table(&ct(vec![field("title", FieldKind::String)])).unwrap();
        assert!(!sql.contains("published_at"), "got: {sql}");
    }

    #[test]
    fn add_published_at_column_builds_alter() {
        let sql = add_published_at_column("post").unwrap();
        assert_eq!(
            sql,
            "ALTER TABLE \"ct_post\" ADD COLUMN \"published_at\" TIMESTAMPTZ"
        );
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p rustapi-sql create_table_emits_published_at_when_draft_publish add_published_at_column_builds_alter`
Expected: FAIL (no published_at logic, no `add_published_at_column`).

- [ ] **Step 3: Implement**

In `crates/sql/src/ddl.rs`, in `create_table`, after the loop that pushes field columns (after the `for f in &ct.fields { ... }` block, before `let body = cols.join`):

```rust
    if ct.draft_publish() {
        cols.push(r#""published_at" TIMESTAMPTZ"#.into());
    }
```

Add a new public function (place near `add_column`):

```rust
/// `ALTER TABLE ct_<name> ADD COLUMN "published_at" TIMESTAMPTZ` — used when
/// Draft & Publish is enabled on an existing type. Nullable: existing rows
/// become drafts (NULL).
pub fn add_published_at_column(ct_name: &str) -> Result<String, DdlError> {
    let table = table_name(ct_name)?;
    Ok(format!(
        "ALTER TABLE {table} ADD COLUMN \"published_at\" TIMESTAMPTZ"
    ))
}
```

Export it: in `crates/sql/src/lib.rs`, the `ddl` re-exports likely use `pub use ddl::*;` — confirm `add_published_at_column` is reachable as `rustapi_sql::add_published_at_column`. If lib.rs lists explicit names, add `add_published_at_column`.

- [ ] **Step 4: Run, verify pass**

Run: `cargo test -p rustapi-sql`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/ddl.rs crates/sql/src/lib.rs
git commit -m "feat(sql): emit published_at column for draft-publish types"
```

---

## Task 3: Publish/unpublish + status-filtered selects in DML

**Files:**
- Modify: `crates/sql/src/dml.rs`

- [ ] **Step 1: Add tests**

In `crates/sql/src/dml.rs` `tests` mod:

```rust
    #[test]
    fn publish_sets_published_at_now() {
        let id = Uuid::nil();
        let (sql, binds) = publish("post", id).unwrap();
        assert_eq!(
            sql,
            "UPDATE \"ct_post\" SET \"published_at\" = now(), \"updated_at\" = now() WHERE \"id\" = $1::uuid RETURNING *"
        );
        assert_eq!(binds.len(), 1);
    }

    #[test]
    fn unpublish_nulls_published_at() {
        let (sql, _) = unpublish("post", Uuid::nil()).unwrap();
        assert_eq!(
            sql,
            "UPDATE \"ct_post\" SET \"published_at\" = NULL, \"updated_at\" = now() WHERE \"id\" = $1::uuid RETURNING *"
        );
    }

    #[test]
    fn select_list_published_filter_appends_clause() {
        use crate::filter::Filter;
        let (sql, _) = select_list_status(
            "post",
            &Filter::default(),
            &Sort::default_created_at(),
            10,
            0,
            PublishFilter::Published,
        )
        .unwrap();
        assert!(sql.contains("\"published_at\" IS NOT NULL"), "got: {sql}");
    }

    #[test]
    fn select_list_draft_filter_appends_clause() {
        use crate::filter::Filter;
        let (sql, _) = select_list_status(
            "post",
            &Filter::default(),
            &Sort::default_created_at(),
            10,
            0,
            PublishFilter::Draft,
        )
        .unwrap();
        assert!(sql.contains("\"published_at\" IS NULL"), "got: {sql}");
    }

    #[test]
    fn select_list_all_filter_no_publish_clause() {
        use crate::filter::Filter;
        let (sql, _) = select_list_status(
            "post",
            &Filter::default(),
            &Sort::default_created_at(),
            10,
            0,
            PublishFilter::All,
        )
        .unwrap();
        assert!(!sql.contains("published_at"), "got: {sql}");
    }
```

Note: confirm `Filter` implements `Default`; if not, use the same way existing `select_list` tests build an empty filter (check the bottom of `dml.rs` tests for the pattern, e.g. `Filter { conditions: vec![] }`).

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p rustapi-sql publish_sets_published_at_now select_list_published_filter_appends_clause`
Expected: FAIL (functions/enum missing).

- [ ] **Step 3: Implement publish/unpublish**

In `crates/sql/src/dml.rs`, add after `delete`:

```rust
/// `UPDATE ct_<name> SET published_at = now(), updated_at = now() WHERE id=$1`
pub fn publish(ct_name: &str, id: Uuid) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let sql = format!(
        "UPDATE {table} SET \"published_at\" = now(), \"updated_at\" = now() WHERE \"id\" = $1::uuid RETURNING *"
    );
    Ok((sql, vec![BoundValue::Str(id.to_string())]))
}

/// `UPDATE ct_<name> SET published_at = NULL, updated_at = now() WHERE id=$1`
pub fn unpublish(ct_name: &str, id: Uuid) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let sql = format!(
        "UPDATE {table} SET \"published_at\" = NULL, \"updated_at\" = now() WHERE \"id\" = $1::uuid RETURNING *"
    );
    Ok((sql, vec![BoundValue::Str(id.to_string())]))
}
```

- [ ] **Step 4: Implement PublishFilter + status-aware select**

Add the enum (near the top, after `pub type SqlAndBinds`):

```rust
/// Which publish state to return from a list query. `All` adds no clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishFilter {
    Published,
    Draft,
    All,
}
```

Add `select_list_status` — a thin wrapper over the existing where/sort/paging logic in `select_list`. Refactor `select_list` to delegate so we stay DRY:

```rust
pub fn select_list_status(
    ct_name: &str,
    filter: &Filter,
    sort: &Sort,
    limit: i64,
    offset: i64,
    publish: PublishFilter,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let col = quote_ident(&sort.column)?;
    let dir = sort.dir.as_sql();

    let (mut where_sql, mut binds) = render_where(filter, 1)?;
    // Append publish-state predicate. render_where returns either "" or
    // " WHERE <conds>"; combine accordingly.
    let publish_pred = match publish {
        PublishFilter::Published => Some("\"published_at\" IS NOT NULL"),
        PublishFilter::Draft => Some("\"published_at\" IS NULL"),
        PublishFilter::All => None,
    };
    if let Some(pred) = publish_pred {
        if where_sql.is_empty() {
            where_sql = format!(" WHERE {pred}");
        } else {
            where_sql = format!("{where_sql} AND {pred}");
        }
    }

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

Then make the existing `select_list` delegate (replace its body):

```rust
pub fn select_list(
    ct_name: &str,
    filter: &Filter,
    sort: &Sort,
    limit: i64,
    offset: i64,
) -> Result<SqlAndBinds, DmlError> {
    select_list_status(ct_name, filter, sort, limit, offset, PublishFilter::All)
}
```

Add a matching `count_status` so list totals respect the filter:

```rust
/// `SELECT count(*) FROM ct_<name> [WHERE ...]` with publish-state predicate.
pub fn count_status(
    ct_name: &str,
    filter: &Filter,
    publish: PublishFilter,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let (mut where_sql, binds) = render_where(filter, 1)?;
    let publish_pred = match publish {
        PublishFilter::Published => Some("\"published_at\" IS NOT NULL"),
        PublishFilter::Draft => Some("\"published_at\" IS NULL"),
        PublishFilter::All => None,
    };
    if let Some(pred) = publish_pred {
        if where_sql.is_empty() {
            where_sql = format!(" WHERE {pred}");
        } else {
            where_sql = format!("{where_sql} AND {pred}");
        }
    }
    Ok((format!("SELECT count(*) FROM {table}{where_sql}"), binds))
}
```

Ensure `PublishFilter`, `publish`, `unpublish`, `select_list_status`, `count_status` are exported via `crates/sql/src/lib.rs` (`pub use dml::*` covers it; otherwise add names).

- [ ] **Step 5: Run, verify pass**

Run: `cargo test -p rustapi-sql`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/sql/src/dml.rs crates/sql/src/lib.rs
git commit -m "feat(sql): publish/unpublish + status-filtered list/count"
```

---

## Task 4: Migration + RawCt options round-trip

**Files:**
- Create: `crates/schema/migrations/0004_content_type_options.sql`
- Modify: `crates/schema/src/registry.rs:142-163`

- [ ] **Step 1: Write migration**

Create `crates/schema/migrations/0004_content_type_options.sql`:

```sql
ALTER TABLE _content_types
    ADD COLUMN IF NOT EXISTS options JSONB NOT NULL DEFAULT '{}'::jsonb;
```

- [ ] **Step 2: Add RawCt options field + round-trip test**

In `crates/schema/src/registry.rs`, update `RawCt`:

```rust
#[derive(sqlx::FromRow)]
struct RawCt {
    id: uuid::Uuid,
    name: String,
    display_name: String,
    fields: sqlx::types::Json<Vec<rustapi_core::Field>>,
    options: sqlx::types::Json<serde_json::Value>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}
```

Update `into_content_type`:

```rust
    fn into_content_type(self) -> ContentType {
        ContentType {
            id: self.id,
            name: self.name,
            display_name: self.display_name,
            fields: self.fields.0,
            options: self.options.0,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
```

Update the SELECT in `reload_from_db`:

```rust
        let rows = sqlx::query_as::<_, RawCt>(
            "SELECT id, name, display_name, fields, options, created_at, updated_at FROM _content_types",
        )
```

Also fix the registry `tests` mod `ct()` helper to include `options: serde_json::json!({})` before `created_at` (compile fix).

- [ ] **Step 3: Build the crate**

Run: `cargo build -p rustapi-schema`
Expected: compiles (the SELECT is a runtime query, not compile-checked).

- [ ] **Step 4: Commit**

```bash
git add crates/schema/migrations/0004_content_type_options.sql crates/schema/src/registry.rs
git commit -m "feat(schema): persist content type options column"
```

---

## Task 5: SchemaService create/patch handle options

**Files:**
- Modify: `crates/schema/src/service.rs:26-92` (create), `:94-212` (patch)

- [ ] **Step 1: Write integration-ish test (inline) for patch enable/disable rules**

The service needs a live DB, so the strongest tests live in `crates/bin/tests/`. Add a focused HTTP integration test in Task 8. Here, add a pure-logic guard with a unit test in `service.rs` for the disable-rejection decision. Add near the top of `service.rs`:

```rust
/// Decide the published_at DDL action when options change on PATCH.
/// Returns Ok(true) when the column must be added (enable), Ok(false) when
/// nothing changes, Err when an unsupported disable is requested.
pub(crate) fn published_at_transition(
    was_enabled: bool,
    now_enabled: bool,
) -> Result<bool, Error> {
    match (was_enabled, now_enabled) {
        (false, true) => Ok(true),
        (true, false) => Err(Error::Validation(
            rustapi_core::ValidationErrors::single(
                "disabling Draft & Publish is not supported",
            ),
        )),
        _ => Ok(false),
    }
}
```

Add test in the `tests` mod:

```rust
    #[test]
    fn published_at_transition_rules() {
        assert_eq!(published_at_transition(false, true).unwrap(), true);
        assert_eq!(published_at_transition(true, true).unwrap(), false);
        assert_eq!(published_at_transition(false, false).unwrap(), false);
        assert!(published_at_transition(true, false).is_err());
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p rustapi-schema published_at_transition_rules`
Expected: FAIL (function missing).

- [ ] **Step 3: Implement create options handling**

In `create` (`service.rs`), set resolved options on the constructed `ct`:

```rust
        let ct = ContentType {
            id,
            name: payload.name.clone(),
            display_name: payload.display_name.clone(),
            fields: payload.fields.clone(),
            options: payload.resolved_options(),
            created_at: now,
            updated_at: now,
        };
```

Update the INSERT to persist options:

```rust
        sqlx::query(
            "INSERT INTO _content_types (id, name, display_name, fields, options, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(ct.id)
        .bind(&ct.name)
        .bind(&ct.display_name)
        .bind(sqlx::types::Json(&ct.fields))
        .bind(sqlx::types::Json(&ct.options))
        .bind(ct.created_at)
        .bind(ct.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;
```

`create_table_sql` already reads `ct.draft_publish()` via Task 2, so the published_at column is emitted automatically.

- [ ] **Step 4: Implement patch options handling**

`PatchContentType` does not yet carry options. Add an optional field to it in `crates/core/src/content_type.rs` (find the `PatchContentType` struct — it has `display_name`, `add_fields`, `drop_fields`, `extend_enum_values`). Add:

```rust
    #[serde(default)]
    pub options: Option<serde_json::Value>,
```

In `patch` (`service.rs`), after computing `new_fields` and before building the UPDATE, resolve options and the transition:

```rust
        let was_enabled = existing.draft_publish();
        let new_options = match &payload.options {
            Some(o) => o.clone(),
            None => existing.options.clone(),
        };
        let now_enabled = new_options
            .get("draft_publish")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if published_at_transition(was_enabled, now_enabled)? {
            let sql = rustapi_sql::add_published_at_column(name)
                .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
            sqlx::query(&sql).execute(&mut *tx).await.map_err(map_db_err)?;
        }
```

Update the UPDATE statement to persist options:

```rust
        sqlx::query(
            "UPDATE _content_types SET display_name = $1, fields = $2, options = $3, updated_at = $4 WHERE name = $5",
        )
        .bind(&new_display)
        .bind(sqlx::types::Json(&new_fields))
        .bind(sqlx::types::Json(&new_options))
        .bind(now)
        .bind(name)
        .execute(&mut *tx)
        .await
        .map_err(map_db_err)?;
```

Update the returned `ContentType` literal to include `options: new_options`.

- [ ] **Step 5: Fix PatchContentType::validate + any constructors**

`PatchContentType::validate` in `content_type.rs` may pattern-match or not care about the new field — adding `Option` with `#[serde(default)]` keeps existing callers compiling. Build to confirm:

Run: `cargo build -p rustapi-core -p rustapi-schema 2>&1 | grep -n "missing field\|error\[" | head`
Fix any `PatchContentType { ... }` literals (in tests across the workspace) by adding `options: None,`.

- [ ] **Step 6: Run schema tests**

Run: `cargo test -p rustapi-schema published_at_transition_rules`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/content_type.rs crates/schema/src/service.rs
git commit -m "feat(schema): create/patch persist options, add published_at on enable, reject disable"
```

---

## Task 6: row_to_json emits published_at; body rejects it

**Files:**
- Modify: `crates/http/src/entry.rs:278-304` (row_to_json), `:64-71` (body_to_binds)

- [ ] **Step 1: Add tests**

In `crates/http/src/entry.rs` `tests` mod, add a body-strip test (mirror the existing `created_at` strip test around line 412):

```rust
    #[test]
    fn body_to_binds_strips_published_at() {
        let ct = /* build a ContentType with draft_publish on; reuse existing test ct builder */;
        let body = serde_json::json!({ "title": "x", "published_at": "2026-01-01T00:00:00Z" })
            .as_object().unwrap().clone();
        let (binds, ..) = body_to_binds(&ct, body, true).unwrap();
        assert!(!binds.contains_key("published_at"));
    }
```

Use the same ContentType-construction helper the surrounding tests use; set `options: serde_json::json!({"draft_publish": true})`. `published_at` is not in `ct.fields`, so the existing "unknown field" check would otherwise reject it — the strip must happen first (Step 3).

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p rustapi-http body_to_binds_strips_published_at`
Expected: FAIL (currently `published_at` triggers unknown-field error → `unwrap()` panics).

- [ ] **Step 3: Strip published_at in body_to_binds**

In `body_to_binds`, extend the system-strip loop:

```rust
    for sys in &["id", "created_at", "updated_at", "published_at"] {
        body.remove(*sys);
    }
```

- [ ] **Step 4: Emit published_at in row_to_json**

In `row_to_json`, after the `updated_at` insert and before the `for f in &ct.fields` loop:

```rust
    if ct.draft_publish() {
        let pa: Option<chrono::DateTime<chrono::Utc>> =
            row.try_get("published_at").map_err(decode)?;
        obj.insert(
            "published_at".into(),
            pa.map(|d| Value::String(d.to_rfc3339())).unwrap_or(Value::Null),
        );
    }
```

- [ ] **Step 5: Run, verify pass**

Run: `cargo test -p rustapi-http body_to_binds_strips_published_at`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/entry.rs
git commit -m "feat(http): serialize published_at, strip it from write bodies"
```

---

## Task 7: HTTP routes — publish/unpublish + status filter

**Files:**
- Modify: `crates/http/src/routes/content.rs` (router, list, get_one, new handlers)

- [ ] **Step 1: Add status param + publish routes (handlers)**

In `crates/http/src/routes/content.rs`:

Extend `ListParams` usage — add a `status` query param. `ListParams` is in `query.rs`; add a field there:

```rust
    #[serde(default)]
    pub status: Option<String>,
```

In `list`, map `status` → `PublishFilter` (default Published for D&P types, All otherwise) and use the status-aware SQL:

```rust
    use rustapi_sql::PublishFilter;
    let publish = if ct.draft_publish() {
        match params.status.as_deref() {
            Some("draft") => PublishFilter::Draft,
            Some("all") => PublishFilter::All,
            _ => PublishFilter::Published, // default + explicit "published"
        }
    } else {
        PublishFilter::All
    };
```

Replace the `select_list` call with `select_list_status(&ct.name, &filter, &opts.sort, opts.page_size as i64, offset, publish)` and the `count` call with `count_status(&ct.name, &filter, publish)`.

Note: capture `params.status` before `parse_list` consumes `params` (it takes `params` by value). Pull `let status = params.status.clone();` up near `let populate_param = params.populate.clone();`, then use `status.as_deref()` above.

Add routes:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/:type", get(list).post(create))
        .route("/api/:type/:id", get(get_one).put(update).delete(delete_one))
        .route("/api/:type/:id/publish", axum::routing::post(publish_entry))
        .route("/api/:type/:id/unpublish", axum::routing::post(unpublish_entry))
}
```

Add the two handlers (model them on `update` for authz + row_to_json):

```rust
async fn publish_entry(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    set_publish_state(state, ct_name, id, principal, true).await
}

async fn unpublish_entry(
    State(state): State<AppState>,
    Path((ct_name, id)): Path<(String, Uuid)>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    set_publish_state(state, ct_name, id, principal, false).await
}

async fn set_publish_state(
    state: AppState,
    ct_name: String,
    id: Uuid,
    principal: Principal,
    publish: bool,
) -> Result<Json<Value>, ApiError> {
    ensure(&state, &principal, Action::ContentWrite, &ct_name).await?;
    let ct = state.schemas.registry().get(&ct_name).await.ok_or(ApiError(Error::NotFound))?;
    if !ct.draft_publish() {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single(
                "Draft & Publish is not enabled for this content type",
            ),
        )));
    }
    let (sql, binds) = if publish {
        rustapi_sql::publish(&ct.name, id)
    } else {
        rustapi_sql::unpublish(&ct.name, id)
    }
    .map_err(|e| ApiError(Error::Internal(anyhow::anyhow!(e.to_string()))))?;
    let q = bind_all(sqlx::query(&sql), &binds);
    let row = q.fetch_optional(&state.pool).await.map_err(db)?;
    let row = row.ok_or(ApiError(Error::NotFound))?;
    state.events.emit(Event::EntryUpdated { content_type: ct.name.clone(), id }).await;
    Ok(Json(row_to_json(&ct, &row)?))
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p rustapi-http`
Expected: compiles. Fix any import (`PublishFilter`, `select_list_status`, `count_status`).

- [ ] **Step 3: Commit**

```bash
git add crates/http/src/routes/content.rs crates/http/src/query.rs
git commit -m "feat(http): publish/unpublish endpoints + status-filtered list"
```

---

## Task 8: HTTP integration tests

**Files:**
- Create: `crates/bin/tests/integration_draft_publish.rs`

- [ ] **Step 1: Write the test**

Model on existing integration tests in `crates/bin/tests/` (use the `common` test harness — check `crates/bin/tests/common/mod.rs` for the `TestApp` builder, auth header helper, and how a content type is created via the API).

```rust
mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn publish_flow_and_status_filter() {
    let app = TestApp::spawn().await;

    // Create a D&P content type.
    app.create_content_type(json!({
        "name": "note",
        "display_name": "Note",
        "options": { "draft_publish": true },
        "fields": [{ "name": "title", "kind": "string", "required": true,
                     "unique": false, "default": null, "kind_meta": {} }]
    })).await;

    // Create an entry → it is a draft (published_at null).
    let created = app.post_json("/api/note", json!({ "title": "hello" })).await;
    let id = created["id"].as_str().unwrap().to_string();
    assert!(created["published_at"].is_null());

    // Default list (status=published) excludes the draft.
    let listed = app.get_json("/api/note").await;
    assert_eq!(listed["meta"]["total"].as_i64().unwrap(), 0);

    // status=draft includes it.
    let drafts = app.get_json("/api/note?status=draft").await;
    assert_eq!(drafts["meta"]["total"].as_i64().unwrap(), 1);

    // Publish.
    let pubd = app.post_json(&format!("/api/note/{id}/publish"), json!({})).await;
    assert!(!pubd["published_at"].is_null());

    // Now default list includes it.
    let listed2 = app.get_json("/api/note").await;
    assert_eq!(listed2["meta"]["total"].as_i64().unwrap(), 1);

    // Unpublish → back to draft.
    let unpubd = app.post_json(&format!("/api/note/{id}/unpublish"), json!({})).await;
    assert!(unpubd["published_at"].is_null());

    // published_at in a write body is ignored, not an error.
    let updated = app.put_json(&format!("/api/note/{id}"),
        json!({ "title": "hi", "published_at": "2026-01-01T00:00:00Z" })).await;
    assert!(updated["published_at"].is_null());
}

#[tokio::test]
async fn publish_rejected_for_non_draft_publish_type() {
    let app = TestApp::spawn().await;
    app.create_content_type(json!({
        "name": "plain",
        "display_name": "Plain",
        "options": { "draft_publish": false },
        "fields": [{ "name": "title", "kind": "string", "required": true,
                     "unique": false, "default": null, "kind_meta": {} }]
    })).await;
    let created = app.post_json("/api/plain", json!({ "title": "x" })).await;
    let id = created["id"].as_str().unwrap();
    let status = app.post_status(&format!("/api/plain/{id}/publish"), json!({})).await;
    assert_eq!(status, 422);
}
```

Adapt method names (`post_json`, `get_json`, `put_json`, `post_status`, `create_content_type`) to whatever the existing harness exposes — read `common/mod.rs` and the neighboring `integration_*.rs` files first and match their exact helpers. If a `post_status` helper doesn't exist, assert on the response the way other tests assert error status.

- [ ] **Step 2: Run**

Run: `cargo test -p rustapi-bin --test integration_draft_publish`
Expected: PASS (requires the test DB the harness provisions).

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test`
Expected: PASS — confirms all earlier compile-fix steps across crates landed.

- [ ] **Step 4: Commit**

```bash
git add crates/bin/tests/integration_draft_publish.rs
git commit -m "test(http): draft-publish flow integration coverage"
```

---

## Task 9: Frontend API types + endpoints

**Files:**
- Modify: `ui/src/api/types.ts:30-43,58-63`
- Modify: `ui/src/api/endpoints.ts:52-89`

- [ ] **Step 1: Add options + draftPublish helper to types**

In `ui/src/api/types.ts`, add `options` to `ContentType` and `NewContentType`, and `published_at` to `Entry`:

```typescript
export interface ContentType {
  id: string;
  name: string;
  display_name: string;
  fields: Field[];
  options?: { draft_publish?: boolean };
  created_at: string;
  updated_at: string;
}
```

```typescript
export interface NewContentType {
  name: string;
  display_name: string;
  fields: Field[];
  options?: { draft_publish?: boolean };
}
```

```typescript
export type Entry = {
  id: string;
  created_at: string;
  updated_at: string;
  published_at?: string | null;
  [field: string]: unknown;
};
```

Add helper near `relationMeta`:

```typescript
export function draftPublishEnabled(ct: ContentType): boolean {
  return ct.options?.draft_publish === true;
}
```

Add `options` to `PatchContentType`:

```typescript
export interface PatchContentType {
  display_name?: string;
  add_fields: Field[];
  drop_fields: string[];
  extend_enum_values: EnumExtension[];
  options?: { draft_publish?: boolean };
}
```

- [ ] **Step 2: Add status param + publish endpoints**

In `ui/src/api/endpoints.ts`, extend `ListOpts` and `listEntries`:

```typescript
interface ListOpts {
  page?: number;
  pageSize?: number;
  sort?: string;
  populate?: string;
  status?: "published" | "draft" | "all";
}
```

In `listEntries`, after the `populate` line:

```typescript
  if (opts.status) q.set("status", opts.status);
```

Add publish endpoints (after `deleteEntry`):

```typescript
export function publishEntry(type: string, id: string): Promise<Entry> {
  return apiFetch<Entry>(
    `/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}/publish`,
    { method: "POST" },
  );
}

export function unpublishEntry(type: string, id: string): Promise<Entry> {
  return apiFetch<Entry>(
    `/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}/unpublish`,
    { method: "POST" },
  );
}
```

- [ ] **Step 3: Typecheck**

Run: `cd ui && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/api/types.ts ui/src/api/endpoints.ts
git commit -m "feat(ui): draft-publish api types + publish/unpublish/status endpoints"
```

---

## Task 10: Builder toggle for Draft & Publish

**Files:**
- Modify: `ui/src/builder/SchemaEditor.tsx`
- Read first: `ui/src/builder/draftModel.ts` (how the draft is shaped/diffed), `ui/src/builder/SchemaEditor.tsx` (settings UI region)

- [ ] **Step 1: Read the builder draft model**

Run: `sed -n '1,80p' ui/src/builder/draftModel.ts` and skim `SchemaEditor.tsx` for where type-level settings (display_name etc.) are edited and where `createContentType` / `patchContentType` are called.

- [ ] **Step 2: Add the toggle UI + state**

In `SchemaEditor.tsx`, add a boolean `draftPublish` to the editor state, defaulting to `true` for a new type or `draftPublishEnabled(ct)` for an existing one. Render a toggle in the type settings area:

```tsx
<label className="rs-toggle-row">
  <input
    type="checkbox"
    checked={draftPublish}
    disabled={!isNew && existingDraftPublish}
    onChange={(e) => setDraftPublish(e.target.checked)}
  />
  <span>Enable Draft &amp; Publish</span>
  {!isNew && existingDraftPublish && (
    <span className="rs-hint" title="Cannot be disabled in v1">locked on</span>
  )}
</label>
```

where `existingDraftPublish = !isNew && draftPublishEnabled(ct)`. Disabling an already-on type is blocked in the UI (matches backend rejection).

- [ ] **Step 3: Wire into create/patch payloads**

On create, include `options: { draft_publish: draftPublish }` in the `NewContentType` body. On patch, include `options: { draft_publish: draftPublish }` only when it changed from `existingDraftPublish` (avoid sending a disable that the server rejects — since the toggle is disabled when on, the only valid change is false→true).

- [ ] **Step 4: Typecheck**

Run: `cd ui && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/builder/SchemaEditor.tsx
git commit -m "feat(ui): Draft & Publish toggle in schema builder"
```

---

## Task 11: ContentList — Status column + Draft/Published tabs

**Files:**
- Modify: `ui/src/screens/ContentList.tsx`

- [ ] **Step 1: Compute D&P + drive status tabs**

In `ContentList.tsx`, after `const ct = schema.data;`, derive:

```tsx
const dp = ct ? draftPublishEnabled(ct) : false;
```

For D&P types, replace the existing `STATUS_TABS` (draft/review/published from a user enum) behavior with publish-state tabs. Add a publish tab set:

```tsx
const PUBLISH_TABS: [string, string][] = [
  ["all", "All"],
  ["published", "Published"],
  ["draft", "Draft"],
];
```

When `dp`, use a `publishFilter` state (default `"published"`) and pass it to `listEntries` as `status`:

```tsx
const [publishFilter, setPublishFilter] = useState<"published" | "draft" | "all">("published");
```

In the `useResource(() => listEntries(type, { ... }))` call, add `status: dp ? publishFilter : undefined` and include `publishFilter` in the dependency array.

Render the publish tabs (reuse the `.rs-cm-tabs` / `.rs-tab` markup that the status tabs use) when `dp`, switching `publishFilter`. The server already filters, so do not also client-filter by status for D&P types.

- [ ] **Step 2: Force-show locked Status column**

`published_at` is not in `ct.fields`, so add it to the column model built in this file (the `allColumns` array from the Fields feature):

```tsx
const allColumns: ColumnDef[] = [
  { key: "id", label: "ID" },
  ...(dp ? [{ key: "published_at", label: "Status" } as ColumnDef] : []),
  ...ct.fields.map((f) => ({ key: f.name, label: f.name })),
  { key: "updated", label: "Updated" },
];
```

Make `published_at` a locked column (always visible) when `dp`. The Fields menu already takes a single `lockedKey` (the title field). Extend it to also lock `published_at`: change `lockedKey` usage to a set/predicate. Minimal change — in `ContentList`, compute:

```tsx
const lockedKeys = new Set<string>();
if (titleField) lockedKeys.add(titleField);
if (dp) lockedKeys.add("published_at");
const colVisible = (key: string) => lockedKeys.has(key) || !hidden[key];
```

Update `FieldsMenu` prop from `lockedKey?: string` to `lockedKeys: Set<string>` and inside it use `lockedKeys.has(c.key)` instead of `c.key === lockedKey`. Update the menu's `shown` count accordingly.

- [ ] **Step 3: Render the Status cell**

In the row render, the column loop must handle the synthetic `published_at` column. In `renderCell` (or wherever columns map to cells), add a branch: when the column key is `published_at`, render a badge from the entry value:

```tsx
// in the cell-rendering switch keyed by column
if (key === "published_at") {
  const v = entry["published_at"];
  return v
    ? <span className="rs-status rs-status--ok">Published</span>
    : <span className="rs-status rs-status--muted">Draft</span>;
}
```

Match the existing `StatusBadge` class names (`rs-status rs-status--ok|muted`) used in `shell.tsx`.

Note: the current table renders ID, then `cols.map(f => ...)`, then Updated. The `published_at` column sits between ID and fields per `allColumns`. Render its header (`Status`) and cell in the same conditional positions, guarded by `colVisible("published_at")`.

- [ ] **Step 4: Typecheck + lint-build**

Run: `cd ui && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/screens/ContentList.tsx ui/src/screens/FieldsMenu.tsx
git commit -m "feat(ui): draft-publish status column + tabs in content list"
```

---

## Task 12: EntryEditor — Draft/Published tabs + Publish button

**Files:**
- Modify: `ui/src/screens/EntryEditor.tsx`

- [ ] **Step 1: Read the editor**

Run: `sed -n '20,145p' ui/src/screens/EntryEditor.tsx` to see state, `save`, and the action bar (`rs-editor-actions`, around line 116).

- [ ] **Step 2: Add publish state + button**

In `EntryEditor`, derive `dp` from the loaded `ct` via `draftPublishEnabled(ct)`. Track the entry's published state from the loaded entry (`existing.data?.published_at`), in local state so the button reflects publish/unpublish immediately:

```tsx
const [publishedAt, setPublishedAt] = useState<string | null>(null);
useEffect(() => {
  setPublishedAt((existing.data?.published_at as string | null) ?? null);
}, [existing.data]);
const isPublished = publishedAt != null;
```

In the `rs-editor-actions` bar, when `dp && !isNew`, add a Publish/Unpublish button beside Save:

```tsx
{dp && !isNew && (
  <button
    className={"rs-btn " + (isPublished ? "rs-btn--ghost" : "rs-btn--primary")}
    onClick={togglePublish}
    disabled={publishing}
  >
    {publishing ? "…" : isPublished ? "Unpublish" : "Publish"}
  </button>
)}
```

Add the handler:

```tsx
const [publishing, setPublishing] = useState(false);
const togglePublish = async () => {
  if (!ct) return;
  setPublishing(true);
  try {
    const updated = isPublished
      ? await unpublishEntry(ct.name, id!)
      : await publishEntry(ct.name, id!);
    setPublishedAt((updated.published_at as string | null) ?? null);
  } catch {
    setBanner("Publish action failed.");
  } finally {
    setPublishing(false);
  }
};
```

Import `publishEntry`, `unpublishEntry`, `draftPublishEnabled`. Use the existing `id` route param and `ct` exactly as the rest of the file does (match its variable names — adjust `id!` to however the file reads the id).

- [ ] **Step 3: Add Draft / Published status indicator**

Near the editor title (`rs-editor-titlewrap`), show the current state when `dp`:

```tsx
{dp && !isNew && (
  <span className={"rs-status " + (isPublished ? "rs-status--ok" : "rs-status--muted")}>
    {isPublished ? "Published" : "Draft"}
  </span>
)}
```

(Model A note: there is no separate editable draft copy — the indicator/tabs reflect this single record's live state. A full tab strip is optional; the status indicator + publish button satisfy the spec's intent for Model A. If a tab strip is desired, render two non-editing tabs "Draft"/"Published" that just set a read-only highlight — but do NOT fork the form data, since there is only one row.)

- [ ] **Step 4: Typecheck**

Run: `cd ui && npx tsc --noEmit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/screens/EntryEditor.tsx
git commit -m "feat(ui): publish/unpublish button + status in entry editor"
```

---

## Task 13: Manual verification

**Files:** none (verification only)

- [ ] **Step 1: Build + run backend, then UI, log in**

Run backend per repo norm; run `cd ui && npm run dev`. Log in (`admin@example.com` / `change-me-please` per README).

- [ ] **Step 2: Verify create-with-D&P**

Create a new content type with the toggle ON (default). Confirm via DB or API that its `ct_<name>` table has a `published_at` column and `_content_types.options` = `{"draft_publish": true}`.

- [ ] **Step 3: Verify list/editor behavior**

- New entries appear under Draft tab, not Published (default list).
- Status column shows Draft badge, locked in the Fields menu (cannot hide).
- Open entry → Publish button sets it Published; it now appears under Published tab and default list.
- Unpublish returns it to Draft.

- [ ] **Step 4: Verify enable-on-existing + disable-blocked**

- On an existing non-D&P type, toggle D&P on via builder → succeeds, existing rows become drafts.
- Confirm the builder disables turning D&P off for an already-on type.

- [ ] **Step 5: Verify non-D&P unchanged**

A type with D&P off shows no Status column/tabs, no Publish button; its list returns all rows; `?status=` is ignored.

---

## Self-review notes

- Spec coverage: options flag (T1,T4,T5), published_at system column (T1,T2,T6), default-on (T1,T5), enable-on-patch + disable-rejected (T5), reserved name (T1), status-filtered reads default published (T3,T7), publish/unpublish endpoints + body protection (T6,T7), builder toggle (T10), list status column+tabs (T11), editor publish UI (T12), tests (T2,T3,T5,T6,T8). All spec sections mapped.
- Model A caveat surfaced in T12: no forked draft form data; status indicator/tabs reflect the single row.
- Cross-task type consistency: `PublishFilter`, `select_list_status`, `count_status`, `add_published_at_column`, `publish`/`unpublish` (sql); `options`/`draft_publish()`/`resolved_options()`/`published_at_transition` (core/schema); `draftPublishEnabled`, `publishEntry`/`unpublishEntry`, `status` opt (ui). FieldsMenu prop changes from `lockedKey` to `lockedKeys: Set` (T11) — update the component built in the Fields feature.
```
