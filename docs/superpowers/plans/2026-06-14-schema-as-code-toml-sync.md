# Schema-as-Code TOML Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sync content types from declarative TOML file(s) into the database on server startup, with TOML as the source of truth for the types it defines.

**Architecture:** A new `rustapi_schema::sync` module loads + parses TOML into existing `NewContentType` values, computes a pure diff against the live registry, and applies create/patch/delete actions through the existing transactional `SchemaService`. Synced types are flagged `managed` in their `options` jsonb; the HTTP layer rejects edits to managed types (409) and the admin UI greys them out. The legacy demo seed is removed entirely.

**Tech Stack:** Rust (axum, sqlx, serde, `toml` crate), React + TypeScript admin UI. Tests via `cargo test` (unit + testcontainers integration) and `pnpm typecheck`.

**Spec:** `docs/superpowers/specs/2026-06-14-schema-as-code-toml-sync-design.md`

---

## File Structure

- Create: `crates/schema/src/sync.rs` — TOML load/parse, `SyncMode`, pure diff (`plan_sync`), apply loop (`sync_from_path`).
- Modify: `crates/schema/src/lib.rs` — add `pub mod sync;` + re-exports.
- Modify: `crates/schema/Cargo.toml` — add `toml` dependency.
- Modify: `Cargo.toml` (workspace) — add `toml` to `[workspace.dependencies]`.
- Modify: `crates/core/src/content_type.rs` — add `ContentType::managed()` helper.
- Modify: `crates/bin/src/config.rs` — add `schema_path`, `schema_sync_mode`; remove `seed`/`RUSTAPI_SEED`.
- Modify: `crates/bin/src/main.rs` — replace seed call with sync call; drop `mod seed`.
- Delete: `crates/bin/src/seed.rs`.
- Modify: `crates/http/src/routes/schema.rs` — managed-type guard in `patch_one`/`delete_one`.
- Create: `examples/schema/blog/*.toml` — preset fixture (doubles as integration-test input).
- Modify: `ui/src/api/types.ts` — `managedType()` helper.
- Modify: `ui/src/builder/SchemaEditor.tsx` — managed badge + disabled actions.

---

## Task 1: Add `toml` dependency

**Files:**
- Modify: `Cargo.toml` (workspace `[workspace.dependencies]`)
- Modify: `crates/schema/Cargo.toml` (`[dependencies]`)

- [ ] **Step 1: Add toml to workspace deps**

In `Cargo.toml`, under `[workspace.dependencies]`, in the Serialization group (after the `csv = "1"` line):

```toml
toml = "0.8"
```

- [ ] **Step 2: Add toml to the schema crate**

In `crates/schema/Cargo.toml`, under `[dependencies]`, after `serde_json.workspace = true`:

```toml
toml.workspace = true
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p rustapi-schema`
Expected: compiles clean (toml unused-import warning is fine until Task 3).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/schema/Cargo.toml
git commit -m "build(schema): add toml dependency"
```

---

## Task 2: `ContentType::managed()` helper

**Files:**
- Modify: `crates/core/src/content_type.rs` (add method to `impl ContentType`, near `draft_publish` at line 62-70; add test in the `tests` module)

- [ ] **Step 1: Write the failing test**

In `crates/core/src/content_type.rs`, inside `mod tests`, add:

```rust
#[test]
fn managed_defaults_and_reads() {
    use serde_json::json;
    let mut ct = ContentType {
        id: Uuid::nil(),
        name: "post".into(),
        display_name: "Post".into(),
        fields: vec![field("title")],
        options: json!({}),
        kind: ContentTypeKind::Collection,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    assert!(!ct.managed());
    ct.options = json!({ "managed": true });
    assert!(ct.managed());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustapi-core managed_defaults_and_reads`
Expected: FAIL — `no method named managed found`.

- [ ] **Step 3: Add the helper**

In `impl ContentType`, directly after the `draft_publish` method (after line 70):

```rust
    /// Whether this type is managed by a schema file (TOML sync). Absent/invalid
    /// `options` → false. Managed types are read-only in the UI/API.
    pub fn managed(&self) -> bool {
        self.options
            .get("managed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustapi-core managed_defaults_and_reads`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/content_type.rs
git commit -m "feat(core): add ContentType::managed() helper"
```

---

## Task 3: Sync module — types, TOML parse, `SyncMode`

**Files:**
- Create: `crates/schema/src/sync.rs`
- Modify: `crates/schema/src/lib.rs:6` (add `pub mod sync;`)

- [ ] **Step 1: Register the module**

In `crates/schema/src/lib.rs`, after `pub mod service;` (line 6):

```rust
pub mod sync;
```

And after `pub use service::SchemaService;` (line 10):

```rust
pub use sync::{sync_from_path, SyncMode};
```

- [ ] **Step 2: Write the failing test (parse + mode)**

Create `crates/schema/src/sync.rs`:

```rust
//! Declarative schema sync: load content types from TOML file(s) and reconcile
//! the database to match on startup. See
//! docs/superpowers/specs/2026-06-14-schema-as-code-toml-sync-design.md.

use rustapi_core::{ContentType, Error, Field, NewContentType, ValidationErrors};
use serde::Deserialize;

/// How aggressively sync reconciles the DB toward the TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    /// Create missing types, add missing fields. Never drop. (default)
    #[default]
    Additive,
    /// Also drop types/fields absent from the TOML.
    Full,
}

impl SyncMode {
    /// Parse from the `RUSTAPI_SCHEMA_SYNC` env value. Unknown/empty → Additive.
    pub fn from_env_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "full" => SyncMode::Full,
            _ => SyncMode::Additive,
        }
    }
}

/// One TOML file's worth of content types.
#[derive(Debug, Deserialize)]
struct SchemaFile {
    #[serde(default, rename = "content_type")]
    content_types: Vec<TomlContentType>,
}

/// A content type as declared in TOML. Maps onto `NewContentType`; `field` is
/// renamed so the TOML key is `[[content_type.field]]`.
#[derive(Debug, Deserialize)]
struct TomlContentType {
    name: String,
    display_name: String,
    #[serde(default)]
    kind: rustapi_core::ContentTypeKind,
    #[serde(default)]
    options: serde_json::Value,
    #[serde(default, rename = "field")]
    fields: Vec<Field>,
}

impl From<TomlContentType> for NewContentType {
    fn from(t: TomlContentType) -> Self {
        NewContentType {
            name: t.name,
            display_name: t.display_name,
            fields: t.fields,
            options: t.options,
            kind: t.kind,
        }
    }
}

/// Parse a single TOML document into content types.
pub(crate) fn parse_toml(doc: &str) -> Result<Vec<NewContentType>, Error> {
    let parsed: SchemaFile = toml::from_str(doc)
        .map_err(|e| Error::Validation(ValidationErrors::single(format!("schema TOML parse: {e}"))))?;
    Ok(parsed.content_types.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustapi_core::FieldKind;

    #[test]
    fn parses_type_with_fields() {
        let doc = r#"
[[content_type]]
name = "post"
display_name = "Post"
kind = "collection"
options = { draft_publish = true }

  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true
"#;
        let cts = parse_toml(doc).expect("parse");
        assert_eq!(cts.len(), 1);
        assert_eq!(cts[0].name, "post");
        assert_eq!(cts[0].fields.len(), 1);
        assert_eq!(cts[0].fields[0].name, "title");
        assert_eq!(cts[0].fields[0].kind, FieldKind::String);
        assert!(cts[0].fields[0].required);
    }

    #[test]
    fn sync_mode_from_env() {
        assert_eq!(SyncMode::from_env_str("full"), SyncMode::Full);
        assert_eq!(SyncMode::from_env_str("FULL"), SyncMode::Full);
        assert_eq!(SyncMode::from_env_str("additive"), SyncMode::Additive);
        assert_eq!(SyncMode::from_env_str("garbage"), SyncMode::Additive);
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p rustapi-schema sync::tests`
Expected: PASS (`parses_type_with_fields`, `sync_mode_from_env`).

Note: `ContentType` import is unused at this step — allow the warning; consumed in Task 4.

- [ ] **Step 4: Commit**

```bash
git add crates/schema/src/sync.rs crates/schema/src/lib.rs
git commit -m "feat(schema): TOML schema-file parse + SyncMode"
```

---

## Task 4: Sync module — pure diff (`plan_sync`)

**Files:**
- Modify: `crates/schema/src/sync.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/schema/src/sync.rs` (above `#[cfg(test)]`), add the action enum and a stub so tests compile, then write tests. First add this above the test module:

```rust
/// One reconciliation step computed by `plan_sync`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SyncAction {
    /// Type in TOML, absent from DB.
    Create(NewContentType),
    /// Type in both: add these fields, drop these field names (drops only in Full).
    Patch {
        name: String,
        add_fields: Vec<Field>,
        drop_fields: Vec<String>,
        options: serde_json::Value,
    },
    /// Type in DB, absent from TOML (Full mode only).
    DropType(String),
    /// Type in DB, absent from TOML (Additive): clear its `managed` flag.
    Unmanage(String),
}

/// Compute the reconciliation plan. Pure: no DB. `desired` is the TOML set,
/// `current` the live registry list. Returns actions in no particular order;
/// the apply loop orders creates by relation dependency.
pub(crate) fn plan_sync(
    desired: &[NewContentType],
    current: &[ContentType],
    mode: SyncMode,
) -> Result<Vec<SyncAction>, Error> {
    use std::collections::HashMap;
    let cur: HashMap<&str, &ContentType> = current.iter().map(|c| (c.name.as_str(), c)).collect();
    let des: HashMap<&str, &NewContentType> =
        desired.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut actions = Vec::new();

    for d in desired {
        match cur.get(d.name.as_str()) {
            None => actions.push(SyncAction::Create(d.clone())),
            Some(existing) => {
                let cur_fields: HashMap<&str, &Field> =
                    existing.fields.iter().map(|f| (f.name.as_str(), f)).collect();
                let des_fields: HashMap<&str, &Field> =
                    d.fields.iter().map(|f| (f.name.as_str(), f)).collect();

                let mut add_fields = Vec::new();
                for f in &d.fields {
                    match cur_fields.get(f.name.as_str()) {
                        None => add_fields.push(f.clone()),
                        Some(cf) => {
                            // Field present in both: kind/meta change is unsupported.
                            if cf.kind != f.kind || cf.kind_meta != f.kind_meta {
                                return Err(Error::Validation(ValidationErrors::field(
                                    &f.name,
                                    format!(
                                        "field `{}` on `{}` changed kind/meta; not supported \
                                         (drop+add in full mode, or edit in UI)",
                                        f.name, d.name
                                    ),
                                )));
                            }
                        }
                    }
                }

                let mut drop_fields = Vec::new();
                if mode == SyncMode::Full {
                    for f in &existing.fields {
                        if !des_fields.contains_key(f.name.as_str())
                            && !rustapi_core::is_system_column(&f.name)
                        {
                            drop_fields.push(f.name.clone());
                        }
                    }
                }

                actions.push(SyncAction::Patch {
                    name: d.name.clone(),
                    add_fields,
                    drop_fields,
                    options: managed_options(&d.options),
                });
            }
        }
    }

    for c in current {
        if !des.contains_key(c.name.as_str()) {
            match mode {
                SyncMode::Full => actions.push(SyncAction::DropType(c.name.clone())),
                SyncMode::Additive => {
                    if c.managed() {
                        actions.push(SyncAction::Unmanage(c.name.clone()));
                    }
                }
            }
        }
    }

    Ok(actions)
}

/// Merge `managed = true` into a type's declared options.
fn managed_options(declared: &serde_json::Value) -> serde_json::Value {
    let mut obj = declared.as_object().cloned().unwrap_or_default();
    obj.insert("managed".into(), serde_json::Value::Bool(true));
    serde_json::Value::Object(obj)
}
```

Then add to `mod tests`:

```rust
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn fld(name: &str) -> Field {
        Field {
            name: name.into(),
            kind: FieldKind::String,
            required: false,
            unique: false,
            default: json!(null),
            max_length: None,
            kind_meta: json!({}),
        }
    }

    fn nct(name: &str, fields: Vec<Field>) -> NewContentType {
        NewContentType {
            name: name.into(),
            display_name: name.into(),
            fields,
            options: json!({}),
            kind: rustapi_core::ContentTypeKind::Collection,
        }
    }

    fn ct(name: &str, fields: Vec<Field>, managed: bool) -> ContentType {
        ContentType {
            id: Uuid::nil(),
            name: name.into(),
            display_name: name.into(),
            fields,
            options: if managed { json!({ "managed": true }) } else { json!({}) },
            kind: rustapi_core::ContentTypeKind::Collection,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn diff_creates_missing_type() {
        let desired = vec![nct("post", vec![fld("title")])];
        let actions = plan_sync(&desired, &[], SyncMode::Additive).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], SyncAction::Create(c) if c.name == "post"));
    }

    #[test]
    fn diff_adds_missing_field() {
        let desired = vec![nct("post", vec![fld("title"), fld("body")])];
        let current = vec![ct("post", vec![fld("title")], true)];
        let actions = plan_sync(&desired, &current, SyncMode::Additive).unwrap();
        match &actions[0] {
            SyncAction::Patch { add_fields, drop_fields, .. } => {
                assert_eq!(add_fields.len(), 1);
                assert_eq!(add_fields[0].name, "body");
                assert!(drop_fields.is_empty());
            }
            other => panic!("expected Patch, got {other:?}"),
        }
    }

    #[test]
    fn diff_drop_field_only_in_full() {
        let desired = vec![nct("post", vec![fld("title")])];
        let current = vec![ct("post", vec![fld("title"), fld("body")], true)];

        let add = plan_sync(&desired, &current, SyncMode::Additive).unwrap();
        match &add[0] {
            SyncAction::Patch { drop_fields, .. } => assert!(drop_fields.is_empty()),
            other => panic!("expected Patch, got {other:?}"),
        }

        let full = plan_sync(&desired, &current, SyncMode::Full).unwrap();
        match &full[0] {
            SyncAction::Patch { drop_fields, .. } => assert_eq!(drop_fields, &vec!["body".to_string()]),
            other => panic!("expected Patch, got {other:?}"),
        }
    }

    #[test]
    fn diff_drop_type_full_unmanage_additive() {
        let current = vec![ct("legacy", vec![fld("x")], true)];
        let full = plan_sync(&[], &current, SyncMode::Full).unwrap();
        assert_eq!(full, vec![SyncAction::DropType("legacy".into())]);

        let add = plan_sync(&[], &current, SyncMode::Additive).unwrap();
        assert_eq!(add, vec![SyncAction::Unmanage("legacy".into())]);
    }

    #[test]
    fn diff_unmanaged_db_only_type_left_alone_additive() {
        // A type the UI made (not managed) that's absent from TOML: no action in additive.
        let current = vec![ct("uionly", vec![fld("x")], false)];
        let add = plan_sync(&[], &current, SyncMode::Additive).unwrap();
        assert!(add.is_empty());
    }

    #[test]
    fn diff_field_kind_change_errors() {
        let mut changed = fld("title");
        changed.kind = FieldKind::Integer;
        let desired = vec![nct("post", vec![changed])];
        let current = vec![ct("post", vec![fld("title")], true)];
        let err = plan_sync(&desired, &current, SyncMode::Full).unwrap_err();
        assert!(format!("{err:?}").contains("changed kind/meta"));
    }

    #[test]
    fn diff_patch_sets_managed_option() {
        let desired = vec![nct("post", vec![fld("title")])];
        let current = vec![ct("post", vec![fld("title")], false)];
        let actions = plan_sync(&desired, &current, SyncMode::Additive).unwrap();
        match &actions[0] {
            SyncAction::Patch { options, .. } => {
                assert_eq!(options.get("managed").and_then(|v| v.as_bool()), Some(true));
            }
            other => panic!("expected Patch, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run tests to verify they fail/compile-error first**

Run: `cargo test -p rustapi-schema sync::tests`
Expected: at first this won't compile because `rustapi_core::is_system_column` may not be re-exported. If the compiler reports `is_system_column` not found, do Step 3.

- [ ] **Step 3: Ensure `is_system_column` is reachable from core**

Check the export:

Run: `grep -n "is_system_column\|pub use system" crates/core/src/lib.rs`

If it is NOT re-exported at the crate root, add to `crates/core/src/lib.rs` near the other `pub use` lines:

```rust
pub use system::is_system_column;
```

(`PatchContentType::validate` already calls `crate::system::is_system_column`, so the function exists; this only ensures the crate-root path used in `sync.rs` resolves.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustapi-schema sync::tests`
Expected: PASS — all diff tests green.

- [ ] **Step 5: Commit**

```bash
git add crates/schema/src/sync.rs crates/core/src/lib.rs
git commit -m "feat(schema): pure plan_sync diff with mode + managed flag"
```

---

## Task 5: Sync module — load from path + apply loop

**Files:**
- Modify: `crates/schema/src/sync.rs`

- [ ] **Step 1: Add the loader (with test)**

Append above `#[cfg(test)]` in `crates/schema/src/sync.rs`:

```rust
use std::path::Path;

/// Load + merge all content types from a path. If `path` is a directory, every
/// `*.toml` file in it (non-recursive) is parsed and merged. If a file, that one
/// file is parsed. Duplicate type names across files are rejected.
pub(crate) fn load_desired(path: &Path) -> Result<Vec<NewContentType>, Error> {
    let mut docs: Vec<(String, String)> = Vec::new(); // (source label, contents)
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .map_err(|e| Error::Internal(anyhow::anyhow!("read schema dir {path:?}: {e}")))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "toml").unwrap_or(false))
            .collect();
        entries.sort();
        for p in entries {
            let body = std::fs::read_to_string(&p)
                .map_err(|e| Error::Internal(anyhow::anyhow!("read {p:?}: {e}")))?;
            docs.push((p.display().to_string(), body));
        }
    } else {
        let body = std::fs::read_to_string(path)
            .map_err(|e| Error::Internal(anyhow::anyhow!("read {path:?}: {e}")))?;
        docs.push((path.display().to_string(), body));
    }

    let mut merged: Vec<NewContentType> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (label, body) in docs {
        for ct in parse_toml(&body)? {
            if !seen.insert(ct.name.clone()) {
                return Err(Error::Validation(ValidationErrors::single(format!(
                    "duplicate content type `{}` (in {label})",
                    ct.name
                ))));
            }
            merged.push(ct);
        }
    }
    Ok(merged)
}

/// Order creates so a relation's target is created before the dependent type.
/// Stable topological sort by relation targets; self-references and cycles fall
/// back to declaration order (the DB-level checks still apply at apply time).
fn order_creates(mut creates: Vec<NewContentType>) -> Vec<NewContentType> {
    use std::collections::HashSet;
    let names: HashSet<String> = creates.iter().map(|c| c.name.clone()).collect();
    let mut ordered: Vec<NewContentType> = Vec::with_capacity(creates.len());
    let mut placed: HashSet<String> = HashSet::new();

    while !creates.is_empty() {
        let idx = creates.iter().position(|c| {
            c.fields.iter().all(|f| match f.relation_meta() {
                Some(m) => {
                    m.target == c.name // self-ref ok
                        || !names.contains(&m.target) // external target ok
                        || placed.contains(&m.target) // dependency already placed
                }
                None => true,
            })
        });
        match idx {
            Some(i) => {
                let c = creates.remove(i);
                placed.insert(c.name.clone());
                ordered.push(c);
            }
            // Cycle: emit the rest in declaration order.
            None => {
                ordered.append(&mut creates);
                break;
            }
        }
    }
    ordered
}
```

Add to `mod tests`:

```rust
    #[test]
    fn order_creates_places_target_before_dependent() {
        let mut author = nct("author", vec![fld("name")]);
        let mut post = nct("post", vec![fld("title")]);
        // post.author -> author
        let mut rel = fld("author");
        rel.kind = FieldKind::Relation;
        rel.kind_meta = json!({"target": "author", "cardinality": "many_to_one"});
        post.fields.push(rel);
        author.display_name = "Author".into();
        post.display_name = "Post".into();

        // Declared post-first; ordering must move author ahead.
        let ordered = super::order_creates(vec![post, author]);
        assert_eq!(ordered[0].name, "author");
        assert_eq!(ordered[1].name, "post");
    }
```

- [ ] **Step 2: Add the apply entry point**

Append the public sync driver below `order_creates`:

```rust
use crate::SchemaService;

/// Entry point called at boot. Loads TOML from `path`, diffs against the live
/// registry, and applies the plan through `SchemaService`. Fail-fast: the first
/// error aborts (and propagates so the server refuses to boot).
pub async fn sync_from_path(
    schemas: &SchemaService,
    path: &str,
    mode: SyncMode,
) -> Result<(), Error> {
    let path = Path::new(path);
    let desired = load_desired(path)?;
    for ct in &desired {
        ct.validate().map_err(Error::from)?;
    }

    let current = schemas.registry().list().await;
    let actions = plan_sync(&desired, &current, mode)?;

    // Split creates out so we can order them by relation dependency.
    let (creates, others): (Vec<_>, Vec<_>) = actions
        .into_iter()
        .partition(|a| matches!(a, SyncAction::Create(_)));
    let create_cts: Vec<NewContentType> = creates
        .into_iter()
        .map(|a| match a {
            SyncAction::Create(c) => c,
            _ => unreachable!(),
        })
        .collect();

    let mut created = 0usize;
    let mut patched = 0usize;
    let mut dropped = 0usize;
    let mut unmanaged = 0usize;

    for mut nct in order_creates(create_cts) {
        nct.options = managed_options(&nct.options);
        schemas.create(nct).await?;
        created += 1;
    }

    for action in others {
        match action {
            SyncAction::Patch { name, add_fields, drop_fields, options } => {
                // Skip a truly empty patch (no field changes and options already match).
                let existing = schemas.registry().get(&name).await;
                let options_changed = existing
                    .as_ref()
                    .map(|e| e.options != options)
                    .unwrap_or(true);
                if add_fields.is_empty() && drop_fields.is_empty() && !options_changed {
                    continue;
                }
                let patch = rustapi_core::PatchContentType {
                    display_name: None,
                    add_fields,
                    drop_fields,
                    extend_enum_values: vec![],
                    options: Some(options),
                };
                schemas.patch(&name, patch).await?;
                patched += 1;
            }
            SyncAction::DropType(name) => {
                schemas.delete(&name).await?;
                dropped += 1;
            }
            SyncAction::Unmanage(name) => {
                if let Some(existing) = schemas.registry().get(&name).await {
                    let mut obj = existing.options.as_object().cloned().unwrap_or_default();
                    obj.remove("managed");
                    let patch = rustapi_core::PatchContentType {
                        display_name: None,
                        add_fields: vec![],
                        drop_fields: vec![],
                        extend_enum_values: vec![],
                        options: Some(serde_json::Value::Object(obj)),
                    };
                    schemas.patch(&name, patch).await?;
                    unmanaged += 1;
                }
            }
            SyncAction::Create(_) => unreachable!("creates handled above"),
        }
    }

    tracing::info!(created, patched, dropped, unmanaged, ?mode, "schema sync complete");
    Ok(())
}
```

- [ ] **Step 3: Run unit tests to verify they pass**

Run: `cargo test -p rustapi-schema sync::tests`
Expected: PASS including `order_creates_places_target_before_dependent`.

- [ ] **Step 4: Build the whole schema crate**

Run: `cargo build -p rustapi-schema`
Expected: clean build (no unused `ContentType`/`SchemaService` warnings now).

- [ ] **Step 5: Commit**

```bash
git add crates/schema/src/sync.rs
git commit -m "feat(schema): load TOML from path + apply sync plan via SchemaService"
```

---

## Task 6: Wire config — add sync settings, remove seed

**Files:**
- Modify: `crates/bin/src/config.rs`

- [ ] **Step 1: Replace the `seed` field with sync fields in `Config`**

In `crates/bin/src/config.rs`, in the `Config` struct, delete the `seed` field and its doc comment (lines 16-18), and add:

```rust
    /// Path to a schema TOML file or a directory of `*.toml`. Unset = sync off.
    /// `RUSTAPI_SCHEMA_DIR` wins over `RUSTAPI_SCHEMA_FILE` if both are set.
    pub schema_path: Option<String>,
    /// Reconcile aggressiveness. `RUSTAPI_SCHEMA_SYNC`: additive (default) | full.
    pub schema_sync_mode: rustapi_schema::SyncMode,
```

- [ ] **Step 2: Update `from_env`**

In `from_env`, delete the `seed` block (lines 48-52). Before the `Ok(Self { ... })`, add:

```rust
        let schema_path = std::env::var("RUSTAPI_SCHEMA_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("RUSTAPI_SCHEMA_FILE")
                    .ok()
                    .filter(|s| !s.is_empty())
            });
        let schema_sync_mode = std::env::var("RUSTAPI_SCHEMA_SYNC")
            .ok()
            .map(|s| rustapi_schema::SyncMode::from_env_str(&s))
            .unwrap_or_default();
```

In the `Ok(Self { ... })` literal, remove `seed,` and add `schema_path,` and `schema_sync_mode,`.

- [ ] **Step 3: Confirm `rustapi-schema` is a dep of the bin crate**

Run: `grep -n "rustapi-schema" crates/bin/Cargo.toml`
Expected: a line present (main.rs already imports it). If missing, add `rustapi-schema = { path = "../schema" }` under `[dependencies]`.

- [ ] **Step 4: Build the bin crate (expect a known break in main.rs)**

Run: `cargo build -p rustapi 2>&1 | head -30`
Expected: FAIL — `main.rs` and `seed.rs` still reference `cfg.seed` / the seed module. Fixed in Task 7. Confirm `config.rs` itself has no errors (errors should point at `main.rs`/`seed.rs`, not `config.rs`).

- [ ] **Step 5: Commit**

```bash
git add crates/bin/src/config.rs crates/bin/Cargo.toml
git commit -m "feat(bin): config for RUSTAPI_SCHEMA_DIR/FILE/SYNC, drop RUSTAPI_SEED"
```

---

## Task 7: Wire boot — call sync, delete seed

**Files:**
- Modify: `crates/bin/src/main.rs:83` (replace seed call), `:17` (drop `mod migrate` is kept; drop `mod seed`), import line `:3`
- Delete: `crates/bin/src/seed.rs`

- [ ] **Step 1: Delete the seed module file**

```bash
git rm crates/bin/src/seed.rs
```

- [ ] **Step 2: Remove the seed import and module from `main.rs`**

In `crates/bin/src/main.rs`, delete `use rustapi::seed;` (line 3). Check whether `seed` is declared as a module in the bin library root rather than `main.rs`:

Run: `grep -rn "mod seed\|pub mod seed" crates/bin/src/`

Delete whichever `mod seed;` / `pub mod seed;` line that grep reports (likely in `crates/bin/src/lib.rs`).

- [ ] **Step 3: Replace the seed call with the sync call**

In `crates/bin/src/main.rs`, replace the seed block (lines 83-85):

```rust
    seed::seed_if_empty(&pool, &schemas, cfg.seed)
        .await
        .context("seed default content")?;
```

with:

```rust
    if let Some(path) = &cfg.schema_path {
        rustapi_schema::sync::sync_from_path(&schemas, path, cfg.schema_sync_mode)
            .await
            .context("schema sync")?;
        registry
            .reload_from_db(&pool)
            .await
            .context("reload schema registry after sync")?;
        tracing::info!(schemas = registry.list().await.len(), "schema sync applied");
    }
```

(The reload keeps the registry authoritative after sync mutations; `SchemaService` also updates the registry in-process, so this is belt-and-suspenders and harmless.)

- [ ] **Step 4: Build the bin crate**

Run: `cargo build -p rustapi`
Expected: clean build.

- [ ] **Step 5: Workspace check**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets 2>&1 | tail -20`
Expected: builds; clippy clean (no warnings introduced by these crates).

- [ ] **Step 6: Commit**

```bash
git add crates/bin/src/main.rs crates/bin/src/lib.rs
git commit -m "feat(bin): run schema sync on boot; remove demo seed"
```

---

## Task 8: HTTP guard — reject edits to managed types

**Files:**
- Modify: `crates/http/src/routes/schema.rs:83-107` (`patch_one`), `:114-138` (`delete_one`)

- [ ] **Step 1: Guard `patch_one`**

In `patch_one`, immediately after the function's opening line `) -> Result<Json<ContentType>, ApiError> {` and before `let ct = state.schemas.patch(...)`, insert:

```rust
    if let Some(existing) = state.schemas.registry().get(&name).await {
        if existing.managed() {
            return Err(ApiError(Error::Conflict(format!(
                "content type `{name}` is managed by a schema file; edit the TOML instead"
            ))));
        }
    }
```

- [ ] **Step 2: Guard `delete_one`**

In `delete_one`, after the `confirm` check block (after line 125, before `state.schemas.delete(&name).await?;`), insert the same guard:

```rust
    if let Some(existing) = state.schemas.registry().get(&name).await {
        if existing.managed() {
            return Err(ApiError(Error::Conflict(format!(
                "content type `{name}` is managed by a schema file; edit the TOML instead"
            ))));
        }
    }
```

- [ ] **Step 3: Build the http crate**

Run: `cargo build -p rustapi-http`
Expected: clean. (`Error` and `state.schemas.registry()` are already in scope — `Error` is imported at line 9-12; `registry()` is used elsewhere in this file.)

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/routes/schema.rs
git commit -m "feat(http): reject PATCH/DELETE on schema-file-managed content types"
```

---

## Task 9: Example preset fixture

**Files:**
- Create: `examples/schema/blog/author.toml`
- Create: `examples/schema/blog/post.toml`

- [ ] **Step 1: Write the author preset**

Create `examples/schema/blog/author.toml`:

```toml
# Blog preset — Author. Synced via RUSTAPI_SCHEMA_DIR=examples/schema/blog
[[content_type]]
name = "author"
display_name = "Author"
kind = "collection"

  [[content_type.field]]
  name = "name"
  kind = "string"
  required = true

  [[content_type.field]]
  name = "bio"
  kind = "text"
```

- [ ] **Step 2: Write the post preset**

Create `examples/schema/blog/post.toml`:

```toml
# Blog preset — Post (relates to Author).
[[content_type]]
name = "post"
display_name = "Post"
kind = "collection"
options = { draft_publish = true }

  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true

  [[content_type.field]]
  name = "slug"
  kind = "slug"
  required = true

  [[content_type.field]]
  name = "body"
  kind = "text"

  [[content_type.field]]
  name = "author"
  kind = "relation"
  kind_meta = { target = "author", cardinality = "many_to_one", inverse = "posts" }
```

- [ ] **Step 3: Sanity-parse the fixture with the unit parser**

Add a test to `crates/schema/src/sync.rs` `mod tests` that loads the dir (relative to the workspace root, which is the crate's parent's parent):

```rust
    #[test]
    fn blog_preset_parses_and_orders() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/schema/blog");
        let desired = super::load_desired(&dir).expect("load blog preset");
        let names: Vec<&str> = desired.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"author"));
        assert!(names.contains(&"post"));
        for c in &desired {
            c.validate().expect("preset type valid");
        }
        let ordered = super::order_creates(desired);
        let pos = |n: &str| ordered.iter().position(|c| c.name == n).unwrap();
        assert!(pos("author") < pos("post"), "author must be created before post");
    }
```

- [ ] **Step 4: Run the fixture test**

Run: `cargo test -p rustapi-schema sync::tests::blog_preset_parses_and_orders`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add examples/schema/blog crates/schema/src/sync.rs
git commit -m "feat(schema): blog preset fixture + parse/order test"
```

---

## Task 10: Integration test — end-to-end sync against Postgres

**Files:**
- Create: `crates/schema/tests/sync_it.rs` (testcontainers integration test)

First inspect an existing integration test to copy the harness exactly (pool setup, MIGRATOR run, container spawn):

- [ ] **Step 1: Find the existing integration-test harness pattern**

Run: `ls crates/schema/tests crates/http/tests 2>/dev/null; grep -rln "testcontainers\|MIGRATOR\|PgPool" crates/*/tests 2>/dev/null | head`

Open one match and copy its container/pool/migrate boilerplate. Use that exact pattern for the helper below (the snippet here shows intent; match the real harness for container + pool + `MIGRATOR.run`).

- [ ] **Step 2: Write the integration test**

Create `crates/schema/tests/sync_it.rs`. Replace the `setup_pool()` body with the boilerplate found in Step 1:

```rust
//! End-to-end TOML sync against an ephemeral Postgres (testcontainers).

use rustapi_schema::{SchemaRegistry, SchemaService, SyncMode};
use rustapi_schema::sync::sync_from_path;
use sqlx::PgPool;
use std::io::Write;

// Spawn an ephemeral Postgres, run MIGRATOR, return a pool.
// COPY the exact harness from the integration test found in Step 1.
async fn setup_pool() -> PgPool {
    todo!("paste container+pool+MIGRATOR boilerplate from an existing tests/ file")
}

async fn service(pool: &PgPool) -> SchemaService {
    let registry = SchemaRegistry::new();
    registry.reload_from_db(pool).await.unwrap();
    SchemaService::new(pool.clone(), registry)
}

fn write_blog_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut a = std::fs::File::create(dir.path().join("author.toml")).unwrap();
    write!(a, r#"
[[content_type]]
name = "author"
display_name = "Author"
  [[content_type.field]]
  name = "name"
  kind = "string"
  required = true
"#).unwrap();
    let mut p = std::fs::File::create(dir.path().join("post.toml")).unwrap();
    write!(p, r#"
[[content_type]]
name = "post"
display_name = "Post"
  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true
  [[content_type.field]]
  name = "author"
  kind = "relation"
  kind_meta = {{ target = "author", cardinality = "many_to_one" }}
"#).unwrap();
    dir
}

#[tokio::test]
async fn sync_creates_types_marked_managed_and_idempotent() {
    let pool = setup_pool().await;
    let svc = service(&pool).await;
    let dir = write_blog_dir();
    let path = dir.path().to_str().unwrap();

    sync_from_path(&svc, path, SyncMode::Additive).await.expect("first sync");
    let author = svc.registry().get("author").await.expect("author created");
    let post = svc.registry().get("post").await.expect("post created");
    assert!(author.managed(), "synced type must be marked managed");
    assert!(post.fields.iter().any(|f| f.name == "author"), "relation field present");

    // Idempotent re-run: no error, still present.
    sync_from_path(&svc, path, SyncMode::Additive).await.expect("second sync no-op");
    assert!(svc.registry().get("post").await.is_some());
}

#[tokio::test]
async fn additive_ignores_db_only_field_full_drops_it() {
    let pool = setup_pool().await;
    let svc = service(&pool).await;
    let dir = write_blog_dir();
    let path = dir.path().to_str().unwrap();
    sync_from_path(&svc, path, SyncMode::Additive).await.unwrap();

    // Add a field to `author` via the service (simulates a UI edit / extra column).
    let patch = rustapi_core::PatchContentType {
        display_name: None,
        add_fields: vec![rustapi_core::Field {
            name: "nickname".into(),
            kind: rustapi_core::FieldKind::String,
            required: false,
            unique: false,
            default: serde_json::Value::Null,
            max_length: None,
            kind_meta: serde_json::json!({}),
        }],
        drop_fields: vec![],
        extend_enum_values: vec![],
        options: None,
    };
    svc.patch("author", patch).await.unwrap();

    // additive: TOML lacks nickname, must NOT drop it.
    sync_from_path(&svc, path, SyncMode::Additive).await.unwrap();
    assert!(svc.registry().get("author").await.unwrap().fields.iter().any(|f| f.name == "nickname"));

    // full: TOML lacks nickname, must drop it.
    sync_from_path(&svc, path, SyncMode::Full).await.unwrap();
    assert!(!svc.registry().get("author").await.unwrap().fields.iter().any(|f| f.name == "nickname"));
}

#[tokio::test]
async fn bad_toml_returns_error() {
    let pool = setup_pool().await;
    let svc = service(&pool).await;
    let dir = tempfile::tempdir().unwrap();
    let mut f = std::fs::File::create(dir.path().join("bad.toml")).unwrap();
    // Missing display_name → NewContentType deserialize/validate fails.
    write!(f, "[[content_type]]\nname = \"x\"\n").unwrap();
    let err = sync_from_path(&svc, dir.path().to_str().unwrap(), SyncMode::Additive).await;
    assert!(err.is_err(), "invalid TOML must error (fail-fast on boot)");
}
```

- [ ] **Step 3: Add `tempfile` dev-dependency to the schema crate**

In `crates/schema/Cargo.toml`, under `[dev-dependencies]`, add (match the version other crates use — check with `grep -rn tempfile crates/*/Cargo.toml`; default to `tempfile = "3"` if none):

```toml
tempfile = "3"
```

Also ensure `tokio` test macros and `testcontainers` are available as dev-deps the same way the harness file from Step 1 declares them; mirror that crate's `[dev-dependencies]`.

- [ ] **Step 4: Run the integration tests (Docker must be running)**

Run: `cargo test -p rustapi-schema --test sync_it`
Expected: PASS (spawns ephemeral Postgres). If `setup_pool` still has `todo!()`, replace it with the real harness from Step 1 first.

- [ ] **Step 5: Commit**

```bash
git add crates/schema/tests/sync_it.rs crates/schema/Cargo.toml
git commit -m "test(schema): integration tests for TOML sync (additive/full/idempotent/bad)"
```

---

## Task 11: UI — managed badge + disabled actions

**Files:**
- Modify: `ui/src/api/types.ts` (add `managedType` helper next to `draftPublishEnabled`)
- Modify: `ui/src/builder/SchemaEditor.tsx` (badge + disable edit/delete when managed)

- [ ] **Step 1: Find the existing `draftPublishEnabled` helper to mirror**

Run: `grep -n "draftPublishEnabled\|export function\|options" ui/src/api/types.ts | head`

- [ ] **Step 2: Add the `managedType` helper**

In `ui/src/api/types.ts`, next to `draftPublishEnabled`, add (mirror its exact shape — it reads `ct.options`):

```ts
export function managedType(ct: { options?: Record<string, unknown> | null }): boolean {
  return ct.options?.["managed"] === true;
}
```

- [ ] **Step 3: Show badge + disable actions in SchemaEditor**

In `ui/src/builder/SchemaEditor.tsx`:
1. Import the helper: add `managedType` to the existing import from `../api/types` (line 10 currently imports `draftPublishEnabled, enumValues`).
2. Where the loaded content type is in scope, compute `const isManaged = ct ? managedType(ct) : false;`.
3. For the edit/save controls and the delete button (the `DeleteTypeModal` trigger and the `SaveBar` save action), disable them when `isManaged` and render a read-only badge. Follow the existing disabled-control + `Notice` patterns already in this file. Example badge near the type header:

```tsx
{isManaged && (
  <Notice kind="info">
    Managed by a schema file — edit the TOML and restart to change this type.
  </Notice>
)}
```

Use existing DESIGN.md tokens via the existing `Notice`/button components; do not hard-code colors. Match how `draftPublishEnabled` already gates UI in this file.

- [ ] **Step 4: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: no type errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/api/types.ts ui/src/builder/SchemaEditor.tsx
git commit -m "feat(ui): lock schema-file-managed content types in the builder"
```

---

## Task 12: Docs + README

**Files:**
- Modify: `README.md` (env vars + schema-as-code section)

- [ ] **Step 1: Document the feature**

In `README.md`, in the environment-variables section, add `RUSTAPI_SCHEMA_DIR`, `RUSTAPI_SCHEMA_FILE`, `RUSTAPI_SCHEMA_SYNC`. Add a short "Schema as code" section covering: TOML format (point to `examples/schema/blog/`), `additive` vs `full`, that synced types are read-only in the UI, that **rename is not supported** (change in UI or drop+add in full mode, with data loss), and that a sync error aborts boot. Note `RUSTAPI_SEED` was removed.

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: schema-as-code TOML sync + env vars"
```

---

## Task 13: Full verification

- [ ] **Step 1: Backend suite (Docker running)**

Run: `cargo test --workspace`
Expected: all pass.

- [ ] **Step 2: Clippy + fmt**

Run: `cargo clippy --workspace --all-targets && cargo fmt --all -- --check`
Expected: clippy clean; fmt clean (run `cargo fmt --all` if not).

- [ ] **Step 3: UI typecheck + build**

Run: `cd ui && pnpm typecheck && pnpm build`
Expected: clean.

- [ ] **Step 4: Manual smoke (optional but recommended)**

With Docker Postgres + `DATABASE_URL`/`RUSTAPI_JWT_SECRET` set:

Run: `RUSTAPI_SCHEMA_DIR=examples/schema/blog cargo run -p rustapi`
Expected: log line `schema sync complete` with `created=2`; `/admin/content-types` lists `author` + `post`; PATCH on `post` returns 409.

- [ ] **Step 5: Final commit (only if fmt changed files)**

```bash
git add -A
git commit -m "chore: fmt"
```
