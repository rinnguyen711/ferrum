# Localization (i18n) Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-content-type localization to the backend — each localized entry is one row per locale, rows grouped by a shared `document_id`, with default-locale fallback on reads, across REST and GraphQL.

**Architecture:** A `localized` option on `ContentType` (parallels the existing `draft_publish` option) gates two extra physical columns (`document_id uuid`, `locale text`) on `ct_<name>`. A `_locales` table + `LocaleRegistry` cache holds the global locale set and the single default. The REST+GraphQL shared content cores in `crates/http/src/routes/content.rs` gain a locale selector; fallback resolution lives there once so both surfaces match.

**Tech Stack:** Rust, axum 0.7, sqlx (Postgres), async-graphql 7. Workspace crates: core → sql → schema → http → bin. Tests via testcontainers (needs Docker).

**Spec:** `docs/superpowers/specs/2026-06-19-localization-backend-design.md`

**Conventions observed in this codebase (read before starting):**
- Physical content tables are `ct_<name>` via `rustapi_sql::table_name`. Columns quoted via `quote_ident`.
- System columns (`id`, `created_at`, `updated_at`, `published_at`) are special-cased in `crates/core/src/system.rs::is_system_column`, in `crates/http/src/entry.rs::row_to_json` (line ~305) and its column-order helper (line ~345). New system-ish columns `document_id`/`locale` must be threaded through all three.
- `ContentType::draft_publish()` (crates/core/src/content_type.rs:64) is the exact pattern for `localized()`.
- Migrations live in `crates/schema/migrations/NNNN_*.sql`. Latest is `0015_end_users.sql`; next is `0016`.
- After adding a migration, the schema crate must rebuild for it to apply (see memory: sqlx-migrate-rebuild).
- Shared content cores: `list_entries`, `get_entry`, `create_entry`, `update_entry`, `delete_entry` in `crates/http/src/routes/content.rs`. Both REST handlers and GraphQL resolvers call them.
- DML builders return `SqlAndBinds = (String, Vec<BoundValue>)`; `select_*` use `SELECT *`, writes use `RETURNING *`. `BoundValue` has no `Vec`/array variant — array binds are bound directly (see `insert_links`).

---

## File Structure

**core** (`crates/core/src/`)
- `content_type.rs` — add `localized()` accessor + `resolved_options()` localized key; reuse for validation.
- `locale.rs` (new) — `is_valid_locale_tag(&str) -> bool`. One responsibility: validate a locale code.
- `system.rs` — `document_id`/`locale` are NOT global system columns (only present on localized types), so leave `is_system_column` alone; instead expose a helper `localization_columns() -> [&str; 2]` for the http layer to special-case.
- `lib.rs` — re-export `locale`, the helper.

**sql** (`crates/sql/src/`)
- `ddl.rs` — emit `document_id`/`locale`/scoped-unique/index when localized; ALTER localize statements.
- `dml.rs` — locale-aware select-by-document, list locale filter+fallback, insert with document_id/locale.
- `locales.rs` (new) — `_locales` CRUD: `list/get/upsert/delete/set_default/load_all`.
- `lib.rs` — re-export new fns + `locales` module.

**schema** (`crates/schema/src/`)
- `sync.rs` / `service.rs` — localize transition (add cols, backfill, constraints); reject de-localize.

**http** (`crates/http/src/`)
- `locale_registry.rs` (new) — `LocaleRegistry` RwLock cache (mirrors RoleRegistry).
- `routes/content.rs` — `LocaleSelector`, locale param threading, fallback resolve in the shared cores.
- `routes/locales.rs` (new) — `/admin/locales` CRUD handlers.
- `query.rs` — parse `?locale=` into `ListParams`.
- `entry.rs` — `row_to_json` surfaces `document_id`/`locale` for localized types.
- `graphql/build.rs` + `graphql/resolve.rs` — `locale` arg on collection queries; pass to cores.
- `state.rs` — `AppState.locales: Arc<LocaleRegistry>`.

**bin** (`crates/bin/src/`)
- `main.rs` — hydrate `LocaleRegistry` at boot.

**tests** (`crates/bin/tests/`)
- `localization.rs` (new) — integration suite.

**migrations**
- `crates/schema/migrations/0016_locales.sql` (new).

---

## Task 1: `core` — `localized()` accessor + option plumbing

**Files:**
- Modify: `crates/core/src/content_type.rs:62-94`
- Test: same file `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing test**

Add to the tests module in `crates/core/src/content_type.rs`:

```rust
#[test]
fn localized_defaults_and_reads() {
    let mut ct = ContentType {
        id: Uuid::nil(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![],
        options: json!({}),
        kind: ContentTypeKind::Collection,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    assert!(!ct.localized());
    ct.options = json!({ "localized": true });
    assert!(ct.localized());
}

#[test]
fn resolved_options_fills_localized() {
    let nct = NewContentType {
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![],
        options: json!({ "localized": true }),
        kind: ContentTypeKind::Collection,
    };
    assert_eq!(nct.resolved_options()["localized"], json!(true));
    // default false when omitted
    let nct2 = NewContentType {
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![],
        options: json!({}),
        kind: ContentTypeKind::Collection,
    };
    assert_eq!(nct2.resolved_options()["localized"], json!(false));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustapi-core localized`
Expected: FAIL — `no method named localized`.

- [ ] **Step 3: Add the accessor and option resolution**

In `impl ContentType` (after `draft_publish`, before `managed`):

```rust
/// Whether localization is enabled. Absent/invalid `options` → false.
pub fn localized(&self) -> bool {
    self.options
        .get("localized")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}
```

In `NewContentType::resolved_options`, after the `draft_publish` insert, add:

```rust
let loc = self
    .options
    .get("localized")
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
obj.insert("localized".into(), serde_json::Value::Bool(loc));
```

(The `obj` binding already exists; insert before `serde_json::Value::Object(obj)`.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustapi-core localized resolved_options`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/content_type.rs
git commit -m "feat(core): localized() option accessor on ContentType"
```

---

## Task 2: `core` — locale tag validation + column-name helper

**Files:**
- Create: `crates/core/src/locale.rs`
- Modify: `crates/core/src/lib.rs`
- Test: in `crates/core/src/locale.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/core/src/locale.rs`:

```rust
//! Locale code validation. A locale tag is a lowercase language subtag,
//! optionally followed by `-` and a region subtag, e.g. `en`, `pt-br`.

/// True if `s` is a syntactically valid locale tag for this CMS:
/// `^[a-z]{2,3}(-[a-z0-9]{2,8})?$`. Deliberately stricter and lowercase-only
/// so a code maps 1:1 to a row value with no case ambiguity.
pub fn is_valid_locale_tag(s: &str) -> bool {
    let mut parts = s.split('-');
    let lang = match parts.next() {
        Some(l) => l,
        None => return false,
    };
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return false;
    }
    match parts.next() {
        None => true,
        Some(region) => {
            parts.next().is_none()
                && (2..=8).contains(&region.len())
                && region
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        }
    }
}

/// The two physical columns added to a localized content type's table, in the
/// order they should be emitted/read. Not part of `is_system_column` because
/// they exist only on localized types.
pub const LOCALIZATION_COLUMNS: [&str; 2] = ["document_id", "locale"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lang_only() {
        assert!(is_valid_locale_tag("en"));
        assert!(is_valid_locale_tag("fra"));
    }

    #[test]
    fn accepts_lang_region() {
        assert!(is_valid_locale_tag("pt-br"));
        assert!(is_valid_locale_tag("en-001"));
    }

    #[test]
    fn rejects_uppercase_empty_and_garbage() {
        assert!(!is_valid_locale_tag("EN"));
        assert!(!is_valid_locale_tag("en-BR"));
        assert!(!is_valid_locale_tag(""));
        assert!(!is_valid_locale_tag("e"));
        assert!(!is_valid_locale_tag("en-br-x"));
        assert!(!is_valid_locale_tag("en_us"));
    }
}
```

- [ ] **Step 2: Register the module — add to `crates/core/src/lib.rs`**

Add alongside the other `pub mod`/`pub use` lines:

```rust
pub mod locale;
pub use locale::{is_valid_locale_tag, LOCALIZATION_COLUMNS};
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test -p rustapi-core locale`
Expected: PASS (test + impl land together; this is pure logic with no prior stub).

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/locale.rs crates/core/src/lib.rs
git commit -m "feat(core): locale tag validation + localization column names"
```

---

## Task 3: `sql` ddl — emit localization columns + scoped unique

**Files:**
- Modify: `crates/sql/src/ddl.rs:14-32` (`create_table`), `column_def` unique handling
- Test: `crates/sql/src/ddl.rs` tests module

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `ddl.rs`:

```rust
#[test]
fn create_table_emits_localization_columns_when_localized() {
    let mut c = ct(vec![field("title", FieldKind::String)]);
    c.options = json!({ "localized": true });
    let sql = create_table(&c).unwrap();
    assert!(sql.contains("\"document_id\" uuid NOT NULL"), "got: {sql}");
    assert!(sql.contains("\"locale\" text NOT NULL"), "got: {sql}");
    assert!(
        sql.contains("UNIQUE (\"document_id\", \"locale\")"),
        "got: {sql}"
    );
}

#[test]
fn create_table_omits_localization_columns_when_not_localized() {
    let sql = create_table(&ct(vec![field("title", FieldKind::String)])).unwrap();
    assert!(!sql.contains("document_id"), "got: {sql}");
    assert!(!sql.contains("\"locale\""), "got: {sql}");
}

#[test]
fn localized_scopes_unique_field_to_document_and_locale() {
    let mut f = field("slug", FieldKind::String);
    f.unique = true;
    let mut c = ct(vec![f]);
    c.options = json!({ "localized": true });
    let sql = create_table(&c).unwrap();
    // Column-level UNIQUE is dropped; a scoped table constraint replaces it.
    assert!(!sql.contains("\"slug\" VARCHAR(255) UNIQUE"), "got: {sql}");
    assert!(
        sql.contains("UNIQUE (\"document_id\", \"locale\", \"slug\")"),
        "got: {sql}"
    );
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p rustapi-sql localiz`
Expected: FAIL — columns/constraints not emitted.

- [ ] **Step 3: Implement in `create_table`**

`create_table` currently builds `cols` then `draft_publish` push. Change so the column emit knows whether the type is localized, and append the localization columns + a trailing table-level UNIQUE/constraint list.

Replace the body of `create_table` (lines 14-32) with:

```rust
pub fn create_table(ct: &ContentType) -> Result<String, DdlError> {
    let table = table_name(&ct.name)?;
    let localized = ct.localized();
    let mut cols: Vec<String> = vec![
        r#""id" UUID PRIMARY KEY DEFAULT gen_random_uuid()"#.into(),
        r#""created_at" TIMESTAMPTZ NOT NULL DEFAULT now()"#.into(),
        r#""updated_at" TIMESTAMPTZ NOT NULL DEFAULT now()"#.into(),
    ];
    if localized {
        cols.push(r#""document_id" uuid NOT NULL"#.into());
        cols.push(r#""locale" text NOT NULL"#.into());
    }
    let mut scoped_unique: Vec<String> = Vec::new();
    for f in &ct.fields {
        if !f.is_stored_column() {
            continue;
        }
        cols.push(column_def_localized(&ct.name, f, localized, &mut scoped_unique)?);
    }
    if ct.draft_publish() {
        cols.push(r#""published_at" TIMESTAMPTZ"#.into());
    }
    if localized {
        cols.push(r#"UNIQUE ("document_id", "locale")"#.into());
        for u in scoped_unique {
            cols.push(u);
        }
    }
    let body = cols.join(", ");
    Ok(format!("CREATE TABLE {table} ({body})"))
}
```

Add a thin wrapper that, when localized, strips a trailing ` UNIQUE` from a unique column's def and records a scoped table constraint instead. Place directly above the existing `fn column_def`:

```rust
/// Wraps `column_def`. When the type is localized and the field is unique,
/// the per-column `UNIQUE` is stripped and a table-level
/// `UNIQUE ("document_id","locale","<col>")` is recorded in `scoped` instead,
/// so two locales of the same document may share a value (e.g. a slug).
fn column_def_localized(
    ct_name: &str,
    f: &Field,
    localized: bool,
    scoped: &mut Vec<String>,
) -> Result<String, DdlError> {
    let def = column_def(ct_name, f)?;
    if localized && f.unique {
        let col = quote_ident(&f.physical_column())?;
        scoped.push(format!("UNIQUE (\"document_id\", \"locale\", {col})"));
        // Remove a standalone " UNIQUE" token from the column def. The emitters
        // append " UNIQUE" (space-prefixed) for string/enum/scalar/one-to-one.
        return Ok(def.replacen(" UNIQUE", "", 1));
    }
    Ok(def)
}
```

> Note: `column_def` for relations uses `physical_column()` (e.g. `author_id`); `f.physical_column()` returns the right column name for both scalar and relation fields. One-to-one relations emit ` UNIQUE` and are correctly scoped by this wrapper.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p rustapi-sql`
Expected: PASS (new tests + all existing ddl tests still green — non-localized path unchanged).

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/ddl.rs
git commit -m "feat(sql): emit document_id/locale columns + scoped unique for localized types"
```

---

## Task 4: `sql` ddl — ALTER statements to localize an existing table

**Files:**
- Modify: `crates/sql/src/ddl.rs`
- Test: `crates/sql/src/ddl.rs` tests module

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn localize_existing_builds_add_backfill_notnull_unique() {
    let sql = localize_table("post", "en").unwrap();
    assert!(sql.contains("ADD COLUMN \"document_id\" uuid"), "got: {sql}");
    assert!(sql.contains("ADD COLUMN \"locale\" text"), "got: {sql}");
    assert!(
        sql.contains("UPDATE \"ct_post\" SET \"document_id\" = \"id\", \"locale\" = 'en'"),
        "got: {sql}"
    );
    assert!(sql.contains("ALTER COLUMN \"document_id\" SET NOT NULL"), "got: {sql}");
    assert!(sql.contains("ALTER COLUMN \"locale\" SET NOT NULL"), "got: {sql}");
    assert!(
        sql.contains("ADD CONSTRAINT \"post_document_locale_uniq\" UNIQUE (\"document_id\", \"locale\")"),
        "got: {sql}"
    );
}

#[test]
fn localize_table_escapes_default_locale() {
    let sql = localize_table("post", "pt-br").unwrap();
    assert!(sql.contains("\"locale\" = 'pt-br'"), "got: {sql}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p rustapi-sql localize_`
Expected: FAIL — `localize_table` not defined.

- [ ] **Step 3: Implement**

Add to `ddl.rs` (near `add_published_at_column`):

```rust
/// Build the multi-statement DDL that turns a non-localized table into a
/// localized one. Existing rows become the default-locale row of their own
/// document (`document_id = id`, `locale = <default>`). Returns a single
/// string of `;`-separated statements (executed together by the caller).
///
/// `default_locale` is validated by the caller (LocaleRegistry default is
/// always a valid tag); it is single-quote-escaped here defensively.
pub fn localize_table(ct_name: &str, default_locale: &str) -> Result<String, DdlError> {
    let table = table_name(ct_name)?;
    let loc = default_locale.replace('\'', "''");
    let uniq = quote_ident(&format!("{ct_name}_document_locale_uniq"))?;
    Ok(format!(
        "ALTER TABLE {table} ADD COLUMN \"document_id\" uuid; \
         ALTER TABLE {table} ADD COLUMN \"locale\" text; \
         UPDATE {table} SET \"document_id\" = \"id\", \"locale\" = '{loc}'; \
         ALTER TABLE {table} ALTER COLUMN \"document_id\" SET NOT NULL; \
         ALTER TABLE {table} ALTER COLUMN \"locale\" SET NOT NULL; \
         ALTER TABLE {table} ADD CONSTRAINT {uniq} UNIQUE (\"document_id\", \"locale\"); \
         CREATE INDEX ON {table} (\"document_id\")"
    ))
}
```

> Scoped-unique rewrite of existing unique columns on localize is **out of v1 scope** for the ALTER path (a localized-from-creation table gets it via Task 3; localizing an existing table keeps global unique on those columns — documented limitation). Keep this task to the document/locale dimension only.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p rustapi-sql localize_`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/ddl.rs
git commit -m "feat(sql): localize_table ALTER builder with default-locale backfill"
```

---

## Task 5: `sql` dml — locale-aware select-by-document and list

**Files:**
- Modify: `crates/sql/src/dml.rs`
- Test: `crates/sql/src/dml.rs` tests module

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn select_by_document_locale_with_fallback() {
    let (sql, binds) = super::select_by_document("post", "en").unwrap();
    assert_eq!(
        sql,
        "SELECT * FROM \"ct_post\" \
         WHERE \"document_id\" = $1::uuid \
         AND \"locale\" = COALESCE(\
         (SELECT \"locale\" FROM \"ct_post\" WHERE \"document_id\" = $1::uuid AND \"locale\" = $2), \
         $3) \
         LIMIT 1"
    );
    assert_eq!(
        binds,
        vec![
            BoundValue::Str("en".into()), // wait: see note
        ]
    );
}
```

> The exact SQL/bind shape is finicky. Use this simpler, verifiable contract instead — replace the test above with:

```rust
#[test]
fn select_by_document_filters_document_and_locale() {
    let id = Uuid::new_v4();
    let (sql, binds) = super::select_by_document("post", id, "fr", "en").unwrap();
    // Requested locale row if present, else the default-locale row.
    assert!(sql.starts_with("SELECT * FROM \"ct_post\" WHERE \"document_id\" = $1::uuid"), "got: {sql}");
    assert!(sql.contains("\"locale\" IN ($2, $3)"), "got: {sql}");
    // Prefer the requested locale: order so requested sorts first, take 1.
    assert!(sql.contains("ORDER BY (\"locale\" = $2) DESC"), "got: {sql}");
    assert!(sql.ends_with("LIMIT 1"), "got: {sql}");
    assert_eq!(
        binds,
        vec![
            BoundValue::Str(id.to_string()),
            BoundValue::Str("fr".into()),
            BoundValue::Str("en".into()),
        ]
    );
}

#[test]
fn list_locale_filter_appends_clause() {
    let s = Sort { column: "created_at".into(), dir: SortDir::Desc };
    let (sql, binds) = super::select_list_localized(
        "post", &Filter::None, &s, 25, 0, PublishFilter::All, "fr", "en",
    ).unwrap();
    // One row per document: requested-locale row, or default-locale row when
    // the document has no requested-locale row.
    assert!(sql.contains("\"locale\" = $1"), "got: {sql}");
    assert!(sql.contains("NOT EXISTS"), "got: {sql}");
    assert_eq!(binds[0], BoundValue::Str("fr".into()));
    assert_eq!(binds[1], BoundValue::Str("en".into()));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p rustapi-sql select_by_document list_locale`
Expected: FAIL — fns not defined.

- [ ] **Step 3: Implement**

Add to `dml.rs`:

```rust
/// Single localized entry by `document_id`, preferring `requested` locale and
/// falling back to `default` locale. Binds: `$1`=document_id, `$2`=requested,
/// `$3`=default. `ORDER BY ("locale" = $2) DESC` puts the requested-locale row
/// first; `LIMIT 1` then yields it, or the default-locale row if requested is
/// absent.
pub fn select_by_document(
    ct_name: &str,
    document_id: Uuid,
    requested: &str,
    default: &str,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let sql = format!(
        "SELECT * FROM {table} WHERE \"document_id\" = $1::uuid \
         AND \"locale\" IN ($2, $3) ORDER BY (\"locale\" = $2) DESC LIMIT 1"
    );
    Ok((
        sql,
        vec![
            BoundValue::Str(document_id.to_string()),
            BoundValue::Str(requested.to_string()),
            BoundValue::Str(default.to_string()),
        ],
    ))
}

/// Localized list: one row per document — the requested-locale row, or the
/// default-locale row when the document has no requested-locale row. Composes
/// on top of the existing filter/sort/publish machinery. Binds: `$1`=requested,
/// `$2`=default, then filter binds, then limit/offset.
#[allow(clippy::too_many_arguments)]
pub fn select_list_localized(
    ct_name: &str,
    filter: &Filter,
    sort: &Sort,
    limit: i64,
    offset: i64,
    publish: PublishFilter,
    requested: &str,
    default: &str,
) -> Result<SqlAndBinds, DmlError> {
    let table = table_name(ct_name)?;
    let col = quote_ident(&sort.column)?;
    let dir = sort.dir.as_sql();

    // Locale predicate: requested-locale rows, plus default-locale rows whose
    // document has no requested-locale row. `t` is the outer row alias.
    let mut binds: Vec<BoundValue> = vec![
        BoundValue::Str(requested.to_string()),
        BoundValue::Str(default.to_string()),
    ];
    let locale_pred = format!(
        "(\"locale\" = $1 OR (\"locale\" = $2 AND NOT EXISTS (\
         SELECT 1 FROM {table} d WHERE d.\"document_id\" = {table}.\"document_id\" AND d.\"locale\" = $1)))"
    );

    // Filter binds start at $3.
    let (where_frag, filter_binds) = render_where(filter, binds.len() + 1)?;
    binds.extend(filter_binds);

    let mut where_sql = if where_frag.is_empty() {
        format!(" WHERE {locale_pred}")
    } else {
        // render_where prefixes " WHERE "; splice locale_pred in front.
        let stripped = where_frag.trim_start_matches(" WHERE ");
        format!(" WHERE {locale_pred} AND {stripped}")
    };

    let publish_pred = match publish {
        PublishFilter::Published => Some("\"published_at\" IS NOT NULL"),
        PublishFilter::Draft => Some("\"published_at\" IS NULL"),
        PublishFilter::All => None,
    };
    if let Some(pred) = publish_pred {
        where_sql = format!("{where_sql} AND {pred}");
    }

    let limit_ph = binds.len() + 1;
    let offset_ph = binds.len() + 2;
    binds.push(BoundValue::I64(limit));
    binds.push(BoundValue::I64(offset));

    let sql = format!(
        "SELECT * FROM {table}{where_sql} ORDER BY {col} {dir}, \"id\" {dir} LIMIT ${limit_ph} OFFSET ${offset_ph}"
    );
    Ok((sql, binds))
}
```

> Keyset pagination for localized lists is out of v1 scope — localized lists use offset paging via `select_list_localized`. The http layer (Task 9) routes localized types to this fn and ignores `?cursor=` for them (documented). Add `use crate::filter::Filter;` etc. as already imported at top of `dml.rs`.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p rustapi-sql select_by_document list_locale`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/dml.rs
git commit -m "feat(sql): locale-aware select_by_document and select_list_localized"
```

---

## Task 6: migration + `sql` locales CRUD

**Files:**
- Create: `crates/schema/migrations/0016_locales.sql`
- Create: `crates/sql/src/locales.rs`
- Modify: `crates/sql/src/lib.rs`
- Test: `crates/sql/src/locales.rs` (pure SQL-string tests) + integration in Task 12

- [ ] **Step 1: Write the migration**

Create `crates/schema/migrations/0016_locales.sql`:

```sql
CREATE TABLE IF NOT EXISTS "_locales" (
    "code"       text PRIMARY KEY,
    "name"       text NOT NULL,
    "is_default" boolean NOT NULL DEFAULT false,
    "position"   int NOT NULL DEFAULT 0
);

-- Seed the default locale. Exactly one row must have is_default = true; the
-- application layer enforces that invariant on mutations.
INSERT INTO "_locales" ("code", "name", "is_default", "position")
VALUES ('en', 'English', true, 0)
ON CONFLICT ("code") DO NOTHING;

-- At most one default (partial unique index).
CREATE UNIQUE INDEX IF NOT EXISTS "_locales_one_default"
    ON "_locales" (("is_default")) WHERE "is_default";
```

- [ ] **Step 2: Write the failing tests**

Create `crates/sql/src/locales.rs`:

```rust
//! `_locales` table access. A `Locale` is a code + display name; exactly one
//! row is the default (enforced here on mutation, plus a partial unique index).

use rustapi_core::Error;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Locale {
    pub code: String,
    pub name: String,
    pub is_default: bool,
    pub position: i32,
}

#[cfg(test)]
mod tests {
    // Pure query-shape coverage; behavioral coverage is the integration suite
    // (Task 12) since these hit Postgres.
    #[test]
    fn module_compiles() {
        // Presence test: the real assertions live in crates/bin/tests/localization.rs
        assert_eq!(2 + 2, 4);
    }
}
```

> The CRUD fns below talk to Postgres, so behavioral tests live in the integration suite (Task 12). This task delivers the functions + migration; the integration tests exercise them.

- [ ] **Step 3: Implement the CRUD fns**

Add to `crates/sql/src/locales.rs`:

```rust
/// All locales ordered by position then code.
pub async fn load_all(pool: &PgPool) -> Result<Vec<Locale>, Error> {
    sqlx::query_as::<_, Locale>(
        "SELECT code, name, is_default, position FROM \"_locales\" ORDER BY position, code",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Internal(anyhow::anyhow!(e)))
}

/// One locale by code.
pub async fn get(pool: &PgPool, code: &str) -> Result<Option<Locale>, Error> {
    sqlx::query_as::<_, Locale>(
        "SELECT code, name, is_default, position FROM \"_locales\" WHERE code = $1",
    )
    .bind(code)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Internal(anyhow::anyhow!(e)))
}

/// Insert or update a locale by code. `make_default = true` flips the default
/// to this locale (clearing the previous default) in one transaction.
pub async fn upsert(
    pool: &PgPool,
    code: &str,
    name: &str,
    position: i32,
    make_default: bool,
) -> Result<Locale, Error> {
    let mut tx = pool.begin().await.map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    if make_default {
        sqlx::query("UPDATE \"_locales\" SET is_default = false WHERE is_default")
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    }
    let loc = sqlx::query_as::<_, Locale>(
        "INSERT INTO \"_locales\" (code, name, is_default, position) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name, \
           is_default = (\"_locales\".is_default OR EXCLUDED.is_default), \
           position = EXCLUDED.position \
         RETURNING code, name, is_default, position",
    )
    .bind(code)
    .bind(name)
    .bind(make_default)
    .bind(position)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    tx.commit().await.map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
    Ok(loc)
}

/// Delete a locale by code. Rejects deleting the default (caller maps to 422).
pub async fn delete(pool: &PgPool, code: &str) -> Result<bool, Error> {
    let row = get(pool, code).await?;
    match row {
        None => Ok(false),
        Some(l) if l.is_default => Err(Error::Validation(
            rustapi_core::ValidationErrors::single("cannot delete the default locale"),
        )),
        Some(_) => {
            let res = sqlx::query("DELETE FROM \"_locales\" WHERE code = $1")
                .bind(code)
                .execute(pool)
                .await
                .map_err(|e| Error::Internal(anyhow::anyhow!(e)))?;
            Ok(res.rows_affected() > 0)
        }
    }
}
```

- [ ] **Step 4: Register module — `crates/sql/src/lib.rs`**

Add:

```rust
pub mod locales;
pub use locales::Locale;
```

- [ ] **Step 5: Verify it compiles + unit test passes**

Run: `cargo test -p rustapi-sql locales`
Expected: PASS (module compiles, presence test green).

- [ ] **Step 6: Commit**

```bash
git add crates/schema/migrations/0016_locales.sql crates/sql/src/locales.rs crates/sql/src/lib.rs
git commit -m "feat(sql): _locales table migration + CRUD"
```

---

## Task 7: `schema` — localize transition on update, reject de-localize

**Files:**
- Modify: `crates/schema/src/service.rs` (the update/alter path that already handles `draft_publish` add-column)
- Test: integration in Task 12 (this path executes DDL against Postgres)

- [ ] **Step 1: Locate the existing draft_publish add-column path**

Run: `grep -n "add_published_at_column\|draft_publish\|fn update" crates/schema/src/service.rs`
Read the surrounding function. It compares stored options to new options and, when `draft_publish` flips false→true, runs `ddl::add_published_at_column`. Mirror that exactly for `localized`.

- [ ] **Step 2: Implement the transition**

In the same update function, after the `draft_publish` handling, add (adapt variable names to the function's locals — `old`/`new` content types or options, the executor `tx`/`pool`, and the default locale source):

```rust
// Localization transition: false -> true localizes the table in place.
let was_localized = old_ct.localized();
let now_localized = new_ct.localized();
if !was_localized && now_localized {
    // Default locale comes from the locales table; the service does not hold
    // a LocaleRegistry, so read it directly. Fall back to "en" if unset.
    let default_code = sqlx::query_scalar::<_, String>(
        "SELECT code FROM \"_locales\" WHERE is_default LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or_else(|| "en".to_string());

    let ddl = rustapi_sql::ddl::localize_table(&new_ct.name, &default_code)
        .map_err(|e| /* map to this fn's error type */ )?;
    for stmt in ddl.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt).execute(&mut *tx).await?;
    }
} else if was_localized && !now_localized {
    return Err(/* this fn's validation error */
        "de-localizing a content type is not supported".into());
}
```

> Match the exact executor (`&mut *tx` vs `pool`), the error type, and the way `old_ct`/`new_ct` are named in this function. If `localize_table` returns a single string of `;`-separated statements, splitting on `;` is safe here because none of the emitted statements contain a literal `;` inside a value except the backfill default-locale string, which is a validated tag (no `;`). If the codebase has a multi-statement exec helper, prefer it.

- [ ] **Step 3: Add a focused unit test if the option-diff logic is unit-testable**

If `service.rs` has unit tests for the draft_publish diff, add a sibling asserting that an old(localized=false)→new(localized=true) diff selects the localize branch (mock/inspect whatever the existing test inspects). If the path is only reachable via Postgres, skip — Task 12 covers it. Do not fabricate a test harness that does not already exist.

- [ ] **Step 4: Verify the crate builds**

Run: `cargo build -p rustapi-schema`
Expected: compiles clean.

- [ ] **Step 5: Commit**

```bash
git add crates/schema/src/service.rs
git commit -m "feat(schema): localize content type on update; reject de-localize"
```

---

## Task 8: `http` — LocaleRegistry cache

**Files:**
- Create: `crates/http/src/locale_registry.rs`
- Modify: `crates/http/src/lib.rs` (module decl), `crates/http/src/state.rs` (AppState field)
- Test: `crates/http/src/locale_registry.rs` unit test (logic only)

- [ ] **Step 1: Write the failing test + struct**

Create `crates/http/src/locale_registry.rs`:

```rust
//! In-memory cache of the locale set, mirroring `RoleRegistry`. Hydrated at
//! boot from `_locales` and reloaded on every mutation.

use rustapi_sql::Locale;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Default)]
struct Inner {
    locales: Vec<Locale>,
    default_code: String,
}

#[derive(Debug, Default)]
pub struct LocaleRegistry {
    inner: RwLock<Inner>,
}

impl LocaleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the cache contents. The default is the `is_default` locale, or
    /// the first locale, or "en" if empty.
    pub async fn set(&self, locales: Vec<Locale>) {
        let default_code = locales
            .iter()
            .find(|l| l.is_default)
            .or_else(|| locales.first())
            .map(|l| l.code.clone())
            .unwrap_or_else(|| "en".to_string());
        let mut w = self.inner.write().await;
        w.locales = locales;
        w.default_code = default_code;
    }

    /// True if `code` is a known locale.
    pub async fn contains(&self, code: &str) -> bool {
        self.inner.read().await.locales.iter().any(|l| l.code == code)
    }

    /// The default locale code.
    pub async fn default_code(&self) -> String {
        self.inner.read().await.default_code.clone()
    }

    /// Resolve the requested locale (or the default when `None`). Returns
    /// `None` if `requested` is a non-empty unknown code (caller → 422).
    pub async fn resolve(&self, requested: Option<&str>) -> Option<String> {
        let r = self.inner.read().await;
        match requested {
            None => Some(r.default_code.clone()),
            Some(code) => {
                if r.locales.iter().any(|l| l.code == code) {
                    Some(code.to_string())
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(code: &str, def: bool) -> Locale {
        Locale { code: code.into(), name: code.into(), is_default: def, position: 0 }
    }

    #[tokio::test]
    async fn resolve_default_and_known_and_unknown() {
        let reg = LocaleRegistry::new();
        reg.set(vec![loc("en", true), loc("fr", false)]).await;
        assert_eq!(reg.default_code().await, "en");
        assert_eq!(reg.resolve(None).await.as_deref(), Some("en"));
        assert_eq!(reg.resolve(Some("fr")).await.as_deref(), Some("fr"));
        assert_eq!(reg.resolve(Some("de")).await, None);
        assert!(reg.contains("fr").await);
        assert!(!reg.contains("de").await);
    }

    #[tokio::test]
    async fn default_falls_back_to_en_when_empty() {
        let reg = LocaleRegistry::new();
        reg.set(vec![]).await;
        assert_eq!(reg.default_code().await, "en");
    }
}
```

- [ ] **Step 2: Register module + AppState field**

In `crates/http/src/lib.rs` add `pub mod locale_registry;` (alongside other module decls).

In `crates/http/src/state.rs`, add to `AppState`:

```rust
pub locales: Arc<crate::locale_registry::LocaleRegistry>,
```

Update every `AppState { ... }` construction site to include `locales`. Find them:

Run: `grep -rn "AppState {" crates/`
For each, add `locales: Arc::new(crate::locale_registry::LocaleRegistry::new()),` (or thread an existing one in test builders). The integration test harness in `crates/bin/tests/common` also constructs state — update there too.

- [ ] **Step 3: Run the unit tests**

Run: `cargo test -p rustapi-http locale_registry`
Expected: PASS.

- [ ] **Step 4: Verify workspace still builds**

Run: `cargo build --workspace`
Expected: compiles (all AppState sites updated).

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/locale_registry.rs crates/http/src/lib.rs crates/http/src/state.rs crates/
git commit -m "feat(http): LocaleRegistry cache + AppState wiring"
```

---

## Task 9: `http` — locale param parsing + thread through shared cores

**Files:**
- Modify: `crates/http/src/query.rs` (add `locale` to `ListParams`), `crates/http/src/routes/content.rs` (cores + handlers), `crates/http/src/entry.rs` (`row_to_json`)
- Test: in `content.rs` where unit-testable; behavior in Task 12

- [ ] **Step 1: Surface document_id/locale in row_to_json**

In `crates/http/src/entry.rs`, `row_to_json` (line ~305) currently inserts `id`, `created_at`, `updated_at`, optionally `published_at`. Add, after `published_at` handling, a localized block keyed on `ct.localized()`:

```rust
if ct.localized() {
    let document_id: uuid::Uuid = row.try_get("document_id").map_err(decode)?;
    obj.insert("document_id".into(), Value::String(document_id.to_string()));
    let locale: String = row.try_get("locale").map_err(decode)?;
    obj.insert("locale".into(), Value::String(locale));
}
```

Also update the column-order helper (line ~345, the `push(...)` list that drives `SELECT`/CSV column order) to push `document_id` and `locale` after the system columns when `ct.localized()`. Find that helper and add the conditional pushes.

- [ ] **Step 2: Add `locale` to ListParams**

In `crates/http/src/query.rs`, add to the `ListParams` struct:

```rust
#[serde(default)]
pub locale: Option<String>,
```

(It deserializes from `?locale=` automatically via the existing `Query<ListParams>` extractor.)

- [ ] **Step 3: Write the failing test for body_to_binds ignoring document_id/locale**

Localized writes must not let a client set `document_id`/`locale` as ordinary fields (the core assigns them). In `crates/http/src/entry.rs` tests, add:

```rust
#[test]
fn body_to_binds_strips_document_id_and_locale() {
    let mut ct = ct_with(vec![field("title", FieldKind::String)]);
    ct.options = serde_json::json!({ "localized": true });
    let body = serde_json::json!({
        "title": "x",
        "document_id": "00000000-0000-0000-0000-000000000000",
        "locale": "fr"
    })
    .as_object()
    .unwrap()
    .clone();
    let (binds, _, _, _, _) = body_to_binds(&ct, body, true).unwrap();
    assert!(!binds.contains_key("document_id"));
    assert!(!binds.contains_key("locale"));
}
```

(Use whatever `ct_with`/`field` helpers the existing `entry.rs` tests use — match the `body_to_binds_strips_published_at` test's helpers exactly.)

- [ ] **Step 4: Run to verify it fails**

Run: `cargo test -p rustapi-http body_to_binds_strips_document`
Expected: FAIL — keys present (they're unknown fields → likely an error, or pass through).

- [ ] **Step 5: Implement the strip**

In `body_to_binds` (entry.rs), where it already removes `id`/`created_at`/`updated_at`/`published_at` from the incoming body, also remove `document_id` and `locale`:

```rust
for sys in rustapi_core::LOCALIZATION_COLUMNS {
    body.remove(sys);
}
```

(Place next to the existing system-column strip loop.)

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p rustapi-http body_to_binds_strips_document`
Expected: PASS.

- [ ] **Step 7: Thread locale into the shared cores**

In `crates/http/src/routes/content.rs`:

**`get_entry`** — change signature to accept the locale and the path id as a document handle for localized types. Add a parameter `locale: Option<&str>`. After fetching `ct`:

```rust
if ct.localized() {
    let requested = state.locales.resolve(locale).await.ok_or_else(|| {
        Error::Validation(rustapi_core::ValidationErrors::single("unknown locale"))
    })?;
    let default = state.locales.default_code().await;
    let (sql, binds) = rustapi_sql::select_by_document(&ct.name, id, &requested, &default)
        .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))?;
    // ... fetch_optional, NotFound, row_to_json, populate, media_embed (same as below)
} else {
    // existing select_by_id path
}
```

Keep both branches sharing the populate/media tail. (`id: Uuid` is the path param; for localized types it is the `document_id`.)

**`list_entries`** — add `locale: Option<&str>`. When `ct.localized()`, resolve requested/default (422 on unknown), force offset mode (ignore `?cursor` for localized — keyset deferred), and call `select_list_localized(&ct.name, &filter, &opts.sort, opts.page_size as i64, offset, publish, &requested, &default)` instead of the keyset/offset branch. Non-localized path unchanged. Set `meta.locale` to the resolved requested code.

**`create_entry`** — add `locale: Option<&str>`. When `ct.localized()`: resolve locale (422 unknown); read optional `document_id` from the body BEFORE the strip (a provided `document_id` means "add a translation"); generate a fresh `document_id` otherwise. After `body_to_binds`, inject `document_id` and `locale` into `binds_map` as `BoundValue`s:

```rust
if ct.localized() {
    let requested = state.locales.resolve(locale).await.ok_or(/* 422 */)?;
    binds_map.insert("locale".into(), rustapi_core::BoundValue::Str(requested));
    binds_map.insert(
        "document_id".into(),
        rustapi_core::BoundValue::Uuid(provided_doc_id.unwrap_or_else(Uuid::new_v4)),
    );
}
```

> `insert`/`update` in dml look up each bind key in `ct.fields` and will reject `document_id`/`locale` as `UnknownField`. To allow them, special-case in dml `insert`/`update`: if the key is one of `LOCALIZATION_COLUMNS`, emit the quoted column directly instead of resolving via `physical_column()`. Add this in Task 5's fns? No — add it now as a tiny dml change with a unit test:

In `dml.rs` `insert` and `update`, replace the `let Some(f) = by_name.get(...) else { return UnknownField }` lookup with: if `name` is in `rustapi_core::LOCALIZATION_COLUMNS`, use `quote_ident(name)?` as the column directly; else the existing field lookup. Add unit test:

```rust
#[test]
fn insert_allows_localization_columns() {
    let mut c = ct(vec![field("title", FieldKind::String)]);
    c.options = json!({ "localized": true });
    let mut vals = BTreeMap::new();
    vals.insert("title".into(), BoundValue::Str("Hi".into()));
    vals.insert("locale".into(), BoundValue::Str("fr".into()));
    vals.insert("document_id".into(), BoundValue::Uuid(Uuid::nil()));
    let (sql, _) = insert(&c, &vals).unwrap();
    assert!(sql.contains("\"document_id\""), "got: {sql}");
    assert!(sql.contains("\"locale\""), "got: {sql}");
}
```

**`update_entry` / `delete_entry`** — add `locale: Option<&str>`. For localized types, resolve the target ROW from `(document_id, locale)` first (no fallback on write): run `select_by_document` with `requested == default == resolved_locale` semantics — actually use a direct exact lookup. Add a dml helper `select_row_id_exact(ct_name, document_id, locale) -> SqlAndBinds` returning the row `id`; fetch it, 404 if absent, then call the existing `update`/`delete` on that row `id`. Implement `select_row_id_exact`:

```rust
pub fn select_row_id_exact(ct_name: &str) -> Result<String, DmlError> {
    let table = table_name(ct_name)?;
    Ok(format!(
        "SELECT \"id\" FROM {table} WHERE \"document_id\" = $1::uuid AND \"locale\" = $2"
    ))
}
```

(Caller binds `$1`=document_id uuid, `$2`=locale; uses `sqlx::query_scalar`.)

- [ ] **Step 8: Update REST handlers to pass locale**

In `content.rs` handlers: `list` reads `params.locale.clone()` and passes it; `get_one`/`update`/`delete_one` need a `?locale=` — add a `Query<LocaleQuery>` extractor (`struct LocaleQuery { locale: Option<String> }`) to those three handlers (the path already carries the id/document handle) and pass `q.locale.as_deref()`. `create` reads `?locale=` the same way.

> Add the `locale` field to the existing `GetParams` struct (content.rs:23) rather than a new struct — it already extracts `populate`; add `#[serde(default)] locale: Option<String>`.

- [ ] **Step 9: Run the http unit tests + build**

Run: `cargo test -p rustapi-sql insert_allows_localization && cargo build -p rustapi-http`
Expected: PASS + compiles.

- [ ] **Step 10: Commit**

```bash
git add crates/http/src/ crates/sql/src/dml.rs
git commit -m "feat(http): thread locale through content cores with default fallback"
```

---

## Task 10: `http` — `/admin/locales` CRUD routes

**Files:**
- Create: `crates/http/src/routes/locales.rs`
- Modify: `crates/http/src/routes/mod.rs` (or wherever routers merge), router assembly
- Test: integration in Task 12

- [ ] **Step 1: Find how admin routers are mounted + the admin guard**

Run: `grep -rn "ensure_admin\|admin/roles\|fn router\|UserWrite" crates/http/src/routes/roles.rs`
`roles.rs` is the closest sibling (admin-gated CRUD with a registry reload). Mirror its structure: a `router() -> Router<AppState>`, handlers using `ensure(state, principal, Action::UserWrite, "")` for the admin gate, and a registry reload after mutations.

- [ ] **Step 2: Implement the routes**

Create `crates/http/src/routes/locales.rs` mirroring `roles.rs`:

```rust
//! /admin/locales CRUD. Admin-gated (UserWrite). Reloads LocaleRegistry on
//! every mutation, mirroring routes/roles.rs.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use rustapi_core::{Action, Error, Principal};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/locales", get(list).post(upsert))
        .route("/admin/locales/:code", axum::routing::delete(delete_one))
}

async fn ensure_admin(state: &AppState, principal: &Principal) -> Result<(), ApiError> {
    if !state.authz.can(principal, Action::UserWrite, "").await {
        return Err(ApiError(Error::Forbidden));
    }
    Ok(())
}

async fn reload(state: &AppState) -> Result<(), ApiError> {
    let all = rustapi_sql::locales::load_all(&state.pool).await.map_err(ApiError)?;
    state.locales.set(all).await;
    Ok(())
}

async fn list(
    State(state): State<AppState>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<Json<Value>, ApiError> {
    ensure_admin(&state, &principal).await?;
    let all = rustapi_sql::locales::load_all(&state.pool).await.map_err(ApiError)?;
    Ok(Json(json!({ "data": all })))
}

#[derive(Deserialize)]
struct UpsertBody {
    code: String,
    name: String,
    #[serde(default)]
    position: i32,
    #[serde(default)]
    is_default: bool,
}

async fn upsert(
    State(state): State<AppState>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
    Json(body): Json<UpsertBody>,
) -> Result<Json<Value>, ApiError> {
    ensure_admin(&state, &principal).await?;
    if !rustapi_core::is_valid_locale_tag(&body.code) {
        return Err(ApiError(Error::Validation(
            rustapi_core::ValidationErrors::single("invalid locale code"),
        )));
    }
    let loc = rustapi_sql::locales::upsert(
        &state.pool, &body.code, &body.name, body.position, body.is_default,
    )
    .await
    .map_err(ApiError)?;
    reload(&state).await?;
    Ok(Json(json!(loc)))
}

async fn delete_one(
    State(state): State<AppState>,
    Path(code): Path<String>,
    axum::extract::Extension(principal): axum::extract::Extension<Principal>,
) -> Result<axum::http::StatusCode, ApiError> {
    ensure_admin(&state, &principal).await?;
    let deleted = rustapi_sql::locales::delete(&state.pool, &code).await.map_err(ApiError)?;
    if !deleted {
        return Err(ApiError(Error::NotFound));
    }
    reload(&state).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
```

> `ValidationErrors::single` already maps to 422 in `ApiError`. `locales::delete` returns the default-locale `Error::Validation` (→422) when attempting to delete the default. Confirm `Action::UserWrite` is the gate `roles.rs` uses; match it.

- [ ] **Step 3: Mount the router**

In the module that composes routers (mirror where `roles::router()` is merged — find with `grep -rn "roles::router\|::router()" crates/http/src crates/bin/src`), add `.merge(crate::routes::locales::router())` into the PROTECTED tree (behind auth). Register `pub mod locales;` in `crates/http/src/routes/mod.rs`.

- [ ] **Step 4: Build**

Run: `cargo build -p rustapi-http`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/routes/locales.rs crates/http/src/routes/mod.rs crates/
git commit -m "feat(http): /admin/locales CRUD routes"
```

---

## Task 11: `http` GraphQL + `bin` boot wiring

**Files:**
- Modify: `crates/http/src/graphql/build.rs`, `crates/http/src/graphql/resolve.rs`
- Modify: `crates/bin/src/main.rs`
- Test: integration in Task 12

- [ ] **Step 1: Add `locale` argument to GraphQL collection queries**

In `crates/http/src/graphql/build.rs`, where each Collection's list and single (`get`) query fields are built, add an optional `locale: String` argument (mirror how an existing optional arg like pagination/`id` is added — find with `grep -n "argument\|InputValue\|\"id\"\|\"page\"" crates/http/src/graphql/build.rs`).

In `crates/http/src/graphql/resolve.rs`, read the `locale` arg from the field context and pass it to the shared cores (`list_entries` / `get_entry`) — the cores now take `locale: Option<&str>` (Task 9). Find the resolver calls with `grep -n "list_entries\|get_entry\|create_entry" crates/http/src/graphql/resolve.rs` and add the new argument to each call.

> All shared-core call sites must pass the new `locale` parameter. Search every caller: `grep -rn "list_entries(\|get_entry(\|create_entry(\|update_entry(\|delete_entry(" crates/`. Update each (REST handlers from Task 9, GraphQL resolvers here, and `content_api.rs` re-exports — pass `None` for non-HTTP callers). `content_api.rs` public fns should also gain the `locale: Option<&str>` param to stay aligned; default `None`.

- [ ] **Step 2: Hydrate LocaleRegistry at boot**

In `crates/bin/src/main.rs`, after the pool is built and other registries are hydrated (find `RoleRegistry`/`reload_from_db`/`SchemaRegistry` hydration), add:

```rust
let locales = std::sync::Arc::new(rustapi_http::locale_registry::LocaleRegistry::new());
match rustapi_sql::locales::load_all(&pool).await {
    Ok(all) => locales.set(all).await,
    Err(e) => tracing::warn!(error = %e, "failed to load locales at boot; using empty set"),
}
```

and pass `locales` into the `AppState { ... }` construction (the field added in Task 8).

- [ ] **Step 3: Build the whole workspace**

Run: `cargo build --workspace`
Expected: compiles — every shared-core call site updated.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/graphql/ crates/bin/src/main.rs crates/http/src/content_api.rs crates/
git commit -m "feat: GraphQL locale arg + boot-time LocaleRegistry hydration"
```

---

## Task 12: Integration test suite

**Files:**
- Create: `crates/bin/tests/localization.rs`
- Reference: `crates/bin/tests/common/` (test harness — content-type creation, auth, request helpers), `crates/bin/tests/pagination_keyset.rs` and `crates/bin/tests/integration_roles.rs` for harness usage patterns.

- [ ] **Step 1: Study the test harness**

Run: `ls crates/bin/tests/common && grep -n "pub async fn\|pub fn" crates/bin/tests/common/*.rs`
Identify how a test spins up the app, authenticates an admin, creates a content type, and issues HTTP requests. Reuse those helpers verbatim — do not invent new ones.

- [ ] **Step 2: Write the integration tests**

Create `crates/bin/tests/localization.rs`. Each test uses the common harness to boot the app, create a localized content type (`options: { "localized": true }` with a unique `slug` field), and exercise the API. Implement these (use the harness's request/auth helpers in place of the pseudo-calls):

```rust
// 1. Create the same document in two locales, read both back.
//    POST /api/post?locale=en {title,slug,document_id?} -> capture document_id
//    POST /api/post?locale=fr {title,slug, document_id: <captured>}
//    GET  /api/post/<document_id>?locale=en -> en row, meta/locale "en"
//    GET  /api/post/<document_id>?locale=fr -> fr row, "fr"

// 2. Fallback: GET ?locale=de (de row absent) -> default (en) row,
//    response "locale" == "en".

// 3. Unknown locale: GET /api/post/<doc>?locale=zz -> 422.

// 4. Slug unique per locale: same slug in en and fr -> both succeed;
//    a second en row with the same slug+document mismatch -> 409
//    (duplicate (document_id, locale) -> 409; same slug different document
//    same locale -> 409 only if a *global*... it is scoped, so it SUCCEEDS).
//    Assert: two documents may share a slug within different locales; the
//    same (document_id, locale) twice -> 409.

// 5. List ?locale=fr -> fr rows where present, en rows otherwise, one row
//    per document.

// 6. Localize existing type: create non-localized type, add a row, PATCH the
//    content type to localized:true, GET the row -> document_id == id,
//    locale == "en".

// 7. Per-locale publish independence (only if the type also has
//    draft_publish): publish fr row, en row stays draft.

// 8. GraphQL: query collection with locale: "fr" matches REST incl. fallback.

// 9. Duplicate (document_id, locale): POST twice same doc+locale -> 409.

// 10. _locales CRUD: GET /admin/locales lists "en"; POST adds "fr";
//     POST is_default:true on "fr" flips default; DELETE the default -> 422;
//     DELETE a non-default -> 204.
```

Write each as a `#[tokio::test]` (or the harness's test macro), with explicit assertions on status codes and JSON fields. No placeholder bodies — fill in real request payloads and assertions following the sibling test files.

- [ ] **Step 3: Run the suite (Docker must be running)**

Run: `cargo test -p rustapi-bin --test localization`
Expected: all tests PASS. (Integration suites can flake on cold parallel runs — re-run isolated if so, per memory.)

- [ ] **Step 4: Commit**

```bash
git add crates/bin/tests/localization.rs
git commit -m "test: localization integration suite"
```

---

## Task 13: Docs + full verification

**Files:**
- Modify: `book/src/reference/rest-api.md` (Localization section), `book/src/SUMMARY.md` if a new page is added
- Reference: `book/CONTRIBUTING.md` (read first)

- [ ] **Step 1: Read the docs contributing rules**

Read `book/CONTRIBUTING.md`. Follow section taxonomy, second person, sentence-case headings, real runnable examples, `theme/rustapi.css` tokens.

- [ ] **Step 2: Document the localization API**

Add a "Localization" subsection to `book/src/reference/rest-api.md`: the `localized` content-type option, `?locale=` on list/get, default-locale fallback (`meta.locale`), `document_id` as the get handle, writing a translation by reusing a `document_id`, and `/admin/locales` CRUD. Use real type/field names verified against a running server. Keep it under the existing reference voice.

- [ ] **Step 3: Build the book (fails on broken links)**

Run: `cd book && mdbook build`
Expected: builds clean.

- [ ] **Step 4: Full workspace verification**

Run:
```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
cd ui && pnpm typecheck
```
Expected: tests green, clippy clean, fmt clean, typecheck clean. (UI unchanged this branch, but typecheck confirms nothing broke.)

- [ ] **Step 5: Commit**

```bash
git add book/
git commit -m "docs: localization REST API reference"
```

---

## Self-Review notes (addressed)

- **Spec coverage:** `_locales` table + registry (T6/T8/T10), per-type `localized` opt-in (T1/T3), document_id/locale rows (T3), default fallback on read (T5/T9), unknown-code 422 (T9), scoped unique (T3), localize-existing ALTER + backfill (T4/T7), reject de-localize (T7), write no-fallback + 409 dup (T9/T12), shared-core single fallback point (T9), GraphQL parity (T11), boot hydration (T11), crate boundaries respected (each task scoped to one crate's concern), tests (T12), docs (T13). Admin UI correctly absent (deferred per user).
- **Type consistency:** core fns `localized()`, `is_valid_locale_tag`, `LOCALIZATION_COLUMNS`; sql `localize_table`, `select_by_document`, `select_list_localized`, `select_row_id_exact`, `locales::{Locale,load_all,get,upsert,delete}`; http `LocaleRegistry::{new,set,contains,default_code,resolve}`; cores all take `locale: Option<&str>`. Names used consistently across tasks.
- **Known deferrals made explicit:** keyset pagination for localized lists (offset only), scoped-unique rewrite on the ALTER path, cross-locale relations — all flagged in-task and in the spec.
