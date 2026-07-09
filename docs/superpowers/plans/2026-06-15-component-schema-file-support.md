# Component Schema-File Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `[[component]]` blocks in the schema TOML define components that startup sync reconciles (before content types), with managed components locked in the UI/API.

**Architecture:** Extend the existing `crates/schema/src/sync.rs` engine with a component parse target, a pure `plan_components` diff, and component application (via the existing `ComponentService`) that runs before content-type application. Add a `managed` column to `_components` so managed components can be flagged and locked. Mirror the content-type managed-lock in the HTTP layer and the component editor UI.

**Tech Stack:** Rust (sqlx, serde, `toml`), React + TypeScript admin UI. Tests via `cargo test` (unit + testcontainers) and `pnpm typecheck`.

**Spec:** `docs/superpowers/specs/2026-06-15-component-schema-file-support-design.md`

---

## File Structure

- Modify: `crates/schema/migrations/0013_component_managed.sql` (new migration).
- Modify: `crates/sql/src/component.rs` — `managed` on `Component` + `RawComponent`; `create`/`update` take `managed`.
- Modify: `crates/schema/src/component.rs` — `ComponentService::create`/`update` thread `managed`.
- Modify: `crates/schema/src/sync.rs` — `TomlComponent`, parse components, `plan_components`, apply components first in `sync_from_path`.
- Modify: `crates/bin/src/main.rs` — pass `&components` to `sync_from_path`.
- Modify: `crates/http/src/routes/components.rs` — pass `managed=false` on create/update; 409 guard on managed update/delete.
- Modify: `ui/src/api/types.ts` — `managedComponent` helper (or reuse).
- Modify: `ui/src/screens/ComponentEditor.tsx` — badge + disable on managed.
- Modify: `examples/schema/blog/post.toml` + new `examples/schema/blog/seo.toml` — fixture.
- Modify: `crates/schema/tests/sync_it.rs` — integration coverage.

---

## Task 1: Add `managed` column migration + plumb through SQL store

**Files:**
- Create: `crates/schema/migrations/0013_component_managed.sql`
- Modify: `crates/sql/src/component.rs`

- [ ] **Step 1: Create the migration**

Create `crates/schema/migrations/0013_component_managed.sql`:

```sql
ALTER TABLE _components ADD COLUMN managed boolean NOT NULL DEFAULT false;
```

- [ ] **Step 2: Add `managed` to the structs**

In `crates/sql/src/component.rs`, add the field to `Component` (after `fields`):

```rust
pub struct Component {
    pub uid: String,
    pub display_name: String,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub managed: bool,
}
```

And to `RawComponent`:

```rust
#[derive(sqlx::FromRow)]
struct RawComponent {
    uid: String,
    display_name: String,
    fields: sqlx::types::Json<Vec<Field>>,
    managed: bool,
}
```

And map it in `into_component`:

```rust
    fn into_component(self) -> Component {
        Component {
            uid: self.uid,
            display_name: self.display_name,
            fields: self.fields.0,
            managed: self.managed,
        }
    }
```

- [ ] **Step 3: Update SELECTs to read `managed`**

In `list()` and `get()`, change both SQL strings to select the column:

```rust
"SELECT uid, display_name, fields, managed FROM _components ORDER BY uid"
```
```rust
"SELECT uid, display_name, fields, managed FROM _components WHERE uid = $1"
```

- [ ] **Step 4: Add `managed` param to `create`/`update`**

Change `create`:

```rust
    pub async fn create(
        &self,
        uid: &str,
        display_name: &str,
        fields: &[Field],
        managed: bool,
    ) -> Result<Component, sqlx::Error> {
        sqlx::query("INSERT INTO _components (uid, display_name, fields, managed) VALUES ($1, $2, $3, $4)")
            .bind(uid)
            .bind(display_name)
            .bind(sqlx::types::Json(fields))
            .bind(managed)
            .execute(&self.pool)
            .await?;
        Ok(Component {
            uid: uid.to_string(),
            display_name: display_name.to_string(),
            fields: fields.to_vec(),
            managed,
        })
    }
```

Change `update`:

```rust
    pub async fn update(
        &self,
        uid: &str,
        display_name: &str,
        fields: &[Field],
        managed: bool,
    ) -> Result<Option<Component>, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE _components SET display_name = $1, fields = $2, managed = $3 WHERE uid = $4",
        )
        .bind(display_name)
        .bind(sqlx::types::Json(fields))
        .bind(managed)
        .bind(uid)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        Ok(Some(Component {
            uid: uid.to_string(),
            display_name: display_name.to_string(),
            fields: fields.to_vec(),
            managed,
        }))
    }
```

- [ ] **Step 5: Build the sql crate (expect downstream breaks)**

Run: `cargo build -p ferrum-sql`
Expected: `ferrum-sql` itself compiles clean. (Callers in `ferrum-schema`/`ferrum-http` will break until Task 2 — that's expected; this step only verifies the sql crate.)

- [ ] **Step 6: Commit**

```bash
git add crates/schema/migrations/0013_component_managed.sql crates/sql/src/component.rs
git commit -m "feat(sql): add managed column to components store"
```

---

## Task 2: Thread `managed` through `ComponentService` + HTTP callers

**Files:**
- Modify: `crates/schema/src/component.rs`
- Modify: `crates/http/src/routes/components.rs`

- [ ] **Step 1: Update `ComponentService::create`/`update` signatures**

In `crates/schema/src/component.rs`, add a `managed: bool` param to `create` and pass it to the store:

```rust
    pub async fn create(
        &self,
        uid: &str,
        display_name: &str,
        fields: Vec<Field>,
        managed: bool,
    ) -> Result<Component, Error> {
        validate_uid(uid)?;
        validate_inner_fields(&fields)?;
        for f in &fields {
            f.validate()
                .map_err(|e| Error::Validation(ValidationErrors::field(&f.name, e.to_string())))?;
        }
        if self.registry.get(uid).await.is_some() {
            return Err(Error::Conflict(format!("component `{uid}` already exists")));
        }
        let c = self
            .store
            .create(uid, display_name, &fields, managed)
            .await
            .map_err(internal)?;
        self.registry.insert(c.clone()).await;
        Ok(c)
    }
```

Same for `update` (add `managed: bool`, pass to `self.store.update(uid, display_name, &fields, managed)`):

```rust
    pub async fn update(
        &self,
        uid: &str,
        display_name: &str,
        fields: Vec<Field>,
        managed: bool,
    ) -> Result<Component, Error> {
        validate_inner_fields(&fields)?;
        for f in &fields {
            f.validate()
                .map_err(|e| Error::Validation(ValidationErrors::field(&f.name, e.to_string())))?;
        }
        let c = self
            .store
            .update(uid, display_name, &fields, managed)
            .await
            .map_err(internal)?
            .ok_or(Error::NotFound)?;
        self.registry.insert(c.clone()).await;
        Ok(c)
    }
```

- [ ] **Step 2: Update HTTP callers to pass `managed = false`**

In `crates/http/src/routes/components.rs`, the `create` handler:

```rust
    let c = state
        .components
        .create(&payload.uid, &payload.display_name, payload.fields, false)
        .await?;
```

The `update_one` handler:

```rust
    let c = state
        .components
        .update(&uid, &payload.display_name, payload.fields, false)
        .await?;
```

- [ ] **Step 3: Build workspace**

Run: `cargo build --workspace`
Expected: clean (sync.rs does not call these yet; only the HTTP + any test callers needed updating). If a test elsewhere calls `components.create(...)`/`update(...)` with the old arity, fix those call sites to pass `false`. Find them: `grep -rn "\.create(" crates/*/tests crates/http/src 2>/dev/null | grep -i component` and `grep -rn "components\.\(create\|update\)" crates`.

- [ ] **Step 4: Run component tests**

Run: `cargo test -p ferrum-schema component`
Expected: existing component tests pass (with Docker for any integration ones; unit ones pass regardless).

- [ ] **Step 5: Commit**

```bash
git add crates/schema/src/component.rs crates/http/src/routes/components.rs
git commit -m "feat(schema): thread managed flag through ComponentService"
```

---

## Task 3: Parse `[[component]]` from TOML

**Files:**
- Modify: `crates/schema/src/sync.rs`

- [ ] **Step 1: Write the failing test**

In `crates/schema/src/sync.rs` `mod tests`, add:

```rust
    #[test]
    fn parses_component_blocks() {
        let doc = r#"
[[component]]
uid = "shared.seo"
display_name = "SEO"
  [[component.field]]
  name = "meta_title"
  kind = "string"

[[content_type]]
name = "post"
display_name = "Post"
  [[content_type.field]]
  name = "title"
  kind = "string"
"#;
        let parsed = parse_schema(doc).expect("parse");
        assert_eq!(parsed.content_types.len(), 1);
        assert_eq!(parsed.components.len(), 1);
        assert_eq!(parsed.components[0].uid, "shared.seo");
        assert_eq!(parsed.components[0].fields[0].name, "meta_title");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p ferrum-schema sync::tests::parses_component_blocks`
Expected: FAIL — `parse_schema` / `ParsedSchema` not found.

- [ ] **Step 3: Add the parse types + function**

In `crates/schema/src/sync.rs`, add `components` to `SchemaFile` and a new `TomlComponent`:

```rust
#[derive(Debug, Deserialize)]
struct SchemaFile {
    #[serde(default, rename = "content_type")]
    content_types: Vec<TomlContentType>,
    #[serde(default, rename = "component")]
    components: Vec<TomlComponent>,
}

/// A component as declared in TOML. `field` is renamed so the TOML key is
/// `[[component.field]]`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TomlComponent {
    pub uid: String,
    pub display_name: String,
    #[serde(default, rename = "field")]
    pub fields: Vec<Field>,
}
```

Add a combined parse result + replace `parse_toml`'s internals with a `parse_schema` that returns both, keeping `parse_toml` as a thin wrapper for existing callers if any remain (search first: `grep -n "parse_toml" crates/schema/src/sync.rs`). Implement:

```rust
/// Parsed content of one or more TOML schema documents.
pub(crate) struct ParsedSchema {
    pub content_types: Vec<NewContentType>,
    pub components: Vec<TomlComponent>,
}

/// Parse a single TOML document into content types + components.
pub(crate) fn parse_schema(doc: &str) -> Result<ParsedSchema, Error> {
    let parsed: SchemaFile = toml::from_str(doc).map_err(|e| {
        Error::Validation(ValidationErrors::single(format!("schema TOML parse: {e}")))
    })?;
    Ok(ParsedSchema {
        content_types: parsed.content_types.into_iter().map(Into::into).collect(),
        components: parsed.components,
    })
}
```

Then update the existing `parse_toml` (used by `load_desired`) — change `load_desired` to call `parse_schema` (see Task 5). For now, keep `parse_toml` returning only content types by delegating, so existing tests still pass:

```rust
/// Parse a single TOML document into content types (components ignored).
/// Retained for the content-type-only unit tests.
pub(crate) fn parse_toml(doc: &str) -> Result<Vec<NewContentType>, Error> {
    Ok(parse_schema(doc)?.content_types)
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p ferrum-schema sync::tests::parses_component_blocks`
Expected: PASS. Also run `cargo test -p ferrum-schema sync::tests` — existing parse test still green.

- [ ] **Step 5: Build (watch for dead-code under -D warnings)**

Run: `cargo build -p ferrum-schema`
Expected: clean. If `ParsedSchema.components` / `TomlComponent` is flagged dead (not yet consumed outside tests), add `#[allow(dead_code)]` with a `// consumed by plan_components in a later task` comment; it will be removed in Task 5.

- [ ] **Step 6: Commit**

```bash
git add crates/schema/src/sync.rs
git commit -m "feat(schema): parse [[component]] blocks from schema TOML"
```

---

## Task 4: Pure `plan_components` diff

**Files:**
- Modify: `crates/schema/src/sync.rs`

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/schema/src/sync.rs`. First a helper to build a stored `Component` and a `TomlComponent`:

```rust
    fn comp(uid: &str, fields: Vec<Field>, managed: bool) -> ferrum_sql::Component {
        ferrum_sql::Component {
            uid: uid.into(),
            display_name: uid.into(),
            fields,
            managed,
        }
    }

    fn tcomp(uid: &str, fields: Vec<Field>) -> super::TomlComponent {
        super::TomlComponent { uid: uid.into(), display_name: uid.into(), fields }
    }

    #[test]
    fn plan_components_create_update_skip() {
        // create: in TOML, not DB
        let desired = vec![tcomp("shared.seo", vec![fld("title")])];
        let acts = plan_components(&desired, &[], SyncMode::Additive).unwrap();
        assert!(matches!(&acts[0], ComponentAction::Create(c) if c.uid == "shared.seo"));

        // skip: equal
        let cur = vec![comp("shared.seo", vec![fld("title")], true)];
        let acts = plan_components(&desired, &cur, SyncMode::Additive).unwrap();
        assert!(acts.is_empty(), "equal component must produce no action");

        // update: fields differ
        let desired2 = vec![tcomp("shared.seo", vec![fld("title"), fld("body")])];
        let acts = plan_components(&desired2, &cur, SyncMode::Additive).unwrap();
        assert!(matches!(&acts[0], ComponentAction::Update(c) if c.fields.len() == 2));
    }

    #[test]
    fn plan_components_delete_full_unmanage_additive() {
        let cur = vec![comp("shared.seo", vec![fld("title")], true)];
        let full = plan_components(&[], &cur, SyncMode::Full).unwrap();
        assert_eq!(full, vec![ComponentAction::Delete("shared.seo".into())]);

        let add = plan_components(&[], &cur, SyncMode::Additive).unwrap();
        assert_eq!(add, vec![ComponentAction::Unmanage("shared.seo".into())]);
    }

    #[test]
    fn plan_components_unmanaged_db_only_left_alone_additive() {
        let cur = vec![comp("ui.only", vec![fld("title")], false)];
        let add = plan_components(&[], &cur, SyncMode::Additive).unwrap();
        assert!(add.is_empty());
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p ferrum-schema sync::tests::plan_components`
Expected: FAIL — `ComponentAction` / `plan_components` not found.

- [ ] **Step 3: Implement `ComponentAction` + `plan_components`**

Above `#[cfg(test)]` in `crates/schema/src/sync.rs`:

```rust
use ferrum_sql::Component;

/// One reconciliation step for components.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComponentAction {
    Create(TomlComponent),
    Update(TomlComponent),
    Delete(String),
    Unmanage(String),
}

/// Pure diff for components. `desired` is the TOML set, `current` the live
/// component registry list.
pub(crate) fn plan_components(
    desired: &[TomlComponent],
    current: &[Component],
    mode: SyncMode,
) -> Result<Vec<ComponentAction>, Error> {
    use std::collections::HashMap;
    let cur: HashMap<&str, &Component> = current.iter().map(|c| (c.uid.as_str(), c)).collect();
    let des: HashMap<&str, &TomlComponent> = desired.iter().map(|c| (c.uid.as_str(), c)).collect();

    let mut actions = Vec::new();

    for d in desired {
        match cur.get(d.uid.as_str()) {
            None => actions.push(ComponentAction::Create(d.clone())),
            Some(existing) => {
                if existing.display_name != d.display_name || existing.fields != d.fields {
                    actions.push(ComponentAction::Update(d.clone()));
                }
            }
        }
    }

    for c in current {
        if !des.contains_key(c.uid.as_str()) {
            match mode {
                SyncMode::Full => actions.push(ComponentAction::Delete(c.uid.clone())),
                SyncMode::Additive => {
                    if c.managed {
                        actions.push(ComponentAction::Unmanage(c.uid.clone()));
                    }
                }
            }
        }
    }

    Ok(actions)
}
```

NOTE: `existing.fields != d.fields` requires `Field: PartialEq`. Verify (`grep -n "derive(.*PartialEq.*)" crates/core/src/field.rs` near `struct Field`). It is derived (the content-type diff already compares fields). If for some reason it is not, report BLOCKED.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p ferrum-schema sync::tests::plan_components`
Expected: PASS (all three tests).

- [ ] **Step 5: Build clean**

Run: `cargo build -p ferrum-schema && cargo clippy -p ferrum-schema --all-targets`
Expected: clean. Remove any now-stale `#[allow(dead_code)]` from Task 3 on `TomlComponent`/`ParsedSchema.components` if those are now reachable (they are referenced by `plan_components` which is `pub(crate)` + used by tests — if still flagged for the non-test build, keep a single suppression and note Task 5 wires it live).

- [ ] **Step 6: Commit**

```bash
git add crates/schema/src/sync.rs
git commit -m "feat(schema): pure plan_components diff"
```

---

## Task 5: Apply components in `sync_from_path` (before content types)

**Files:**
- Modify: `crates/schema/src/sync.rs`
- Modify: `crates/bin/src/main.rs`

- [ ] **Step 1: Change `load_desired` to return content types + components**

In `crates/schema/src/sync.rs`, change `load_desired` to return `ParsedSchema` (merged across files, with duplicate-uid rejection for components alongside the existing duplicate-name rejection for types):

```rust
pub(crate) fn load_desired(path: &Path) -> Result<ParsedSchema, Error> {
    let mut docs: Vec<(String, String)> = Vec::new();
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

    let mut content_types: Vec<NewContentType> = Vec::new();
    let mut components: Vec<TomlComponent> = Vec::new();
    let mut seen_ct: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_comp: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (label, body) in docs {
        let parsed = parse_schema(&body)?;
        for ct in parsed.content_types {
            if !seen_ct.insert(ct.name.clone()) {
                return Err(Error::Validation(ValidationErrors::single(format!(
                    "duplicate content type `{}` (in {label})",
                    ct.name
                ))));
            }
            content_types.push(ct);
        }
        for c in parsed.components {
            if !seen_comp.insert(c.uid.clone()) {
                return Err(Error::Validation(ValidationErrors::single(format!(
                    "duplicate component `{}` (in {label})",
                    c.uid
                ))));
            }
            components.push(c);
        }
    }
    Ok(ParsedSchema { content_types, components })
}
```

- [ ] **Step 2: Add `&ComponentService` to `sync_from_path` + apply components first**

Change the signature and add component application before content-type application. The function currently takes `(schemas: &SchemaService, path: &str, mode: SyncMode)`. New signature:

```rust
use crate::{ComponentService, SchemaService};

pub async fn sync_from_path(
    schemas: &SchemaService,
    components: &ComponentService,
    path: &str,
    mode: SyncMode,
) -> Result<(), Error> {
    let path = Path::new(path);
    let desired = load_desired(path)?;

    // ---- components first ----
    let cur_components = components.registry().list().await;
    let comp_actions = plan_components(&desired.components, &cur_components, mode)?;
    apply_components(components, schemas, &desired, comp_actions).await?;

    // ---- then content types (existing logic) ----
    for ct in &desired.content_types {
        ct.validate().map_err(Error::from)?;
    }
    let current = schemas.registry().list().await;
    let actions = plan_sync(&desired.content_types, &current, mode)?;
    // ... existing create/patch/drop/unmanage application unchanged ...
}
```

IMPORTANT: keep the existing content-type application body exactly as-is, just sourced from `desired.content_types` instead of the old `desired` variable. Update the existing references (`desired` → `desired.content_types`, and the `order_drops(drop_names, &current)` call still uses the content-type `current`).

- [ ] **Step 3: Add the `apply_components` helper**

Add below `sync_from_path`:

```rust
/// Apply component actions: create/update with managed=true, delete (full) or
/// unmanage (additive). Delete is blocked by ComponentService when the component
/// is still referenced by a content type — that surfaces as a fail-fast error.
async fn apply_components(
    components: &ComponentService,
    schemas: &SchemaService,
    desired: &ParsedSchema,
    actions: Vec<ComponentAction>,
) -> Result<(), Error> {
    for action in actions {
        match action {
            ComponentAction::Create(c) => {
                components
                    .create(&c.uid, &c.display_name, c.fields, true)
                    .await?;
            }
            ComponentAction::Update(c) => {
                components
                    .update(&c.uid, &c.display_name, c.fields, true)
                    .await?;
            }
            ComponentAction::Delete(uid) => {
                let referencing = referencing_types(&uid, desired, schemas).await;
                components.delete(&uid, &referencing).await?;
            }
            ComponentAction::Unmanage(uid) => {
                if let Some(existing) = components.registry().get(&uid).await {
                    components
                        .update(&existing.uid, &existing.display_name, existing.fields, false)
                        .await?;
                }
            }
        }
    }
    Ok(())
}

/// Content-type names that reference component `uid`, drawn from both the desired
/// TOML types and the live registry (a superset → conservative; the service's own
/// check is the backstop).
async fn referencing_types(
    uid: &str,
    desired: &ParsedSchema,
    schemas: &SchemaService,
) -> Vec<String> {
    use std::collections::HashSet;
    let mut names: HashSet<String> = HashSet::new();
    let refs = |fields: &[Field]| {
        fields.iter().any(|f| {
            f.component_meta()
                .map(|m| m.component == uid)
                .unwrap_or(false)
        })
    };
    for ct in &desired.content_types {
        if refs(&ct.fields) {
            names.insert(ct.name.clone());
        }
    }
    for ct in schemas.registry().list().await {
        if refs(&ct.fields) {
            names.insert(ct.name.clone());
        }
    }
    names.into_iter().collect()
}
```

NOTE: `Field::component_meta()` exists (used in `crates/http/src/routes/components.rs` `delete_one`). Verify: `grep -n "fn component_meta" crates/core/src/field.rs`.

- [ ] **Step 4: Update the call site in main.rs**

In `crates/bin/src/main.rs`, the sync call currently is:

```rust
        ferrum_schema::sync::sync_from_path(&schemas, path, cfg.schema_sync_mode)
```

Change to pass the components service (already constructed in main as `components`):

```rust
        ferrum_schema::sync::sync_from_path(&schemas, &components, path, cfg.schema_sync_mode)
```

Confirm the `components` variable name in main.rs (`grep -n "ComponentService::new\|let components" crates/bin/src/main.rs`); use the actual binding.

- [ ] **Step 5: Fix the existing integration test call site**

`crates/schema/tests/sync_it.rs` calls `sync_from_path(&svc, path, mode)`. It now needs a `ComponentService`. Add a helper there:

```rust
async fn comp_service(pool: &PgPool) -> ferrum_schema::ComponentService {
    let reg = ferrum_schema::ComponentRegistry::new();
    reg.reload_from_db(pool).await.unwrap();
    ferrum_schema::ComponentService::new(pool.clone(), reg)
}
```

And update each `sync_from_path(&svc, path, MODE)` call to `sync_from_path(&svc, &comp_service(&pool).await, path, MODE)`.

- [ ] **Step 6: Remove stale dead-code suppressions + build**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets`
Expected: clean. Remove any `#[allow(dead_code)]` added in Tasks 3/4 that is now unnecessary (components are wired live through `sync_from_path`). Keep only suppressions the compiler still demands.

- [ ] **Step 7: Run schema unit tests**

Run: `cargo test -p ferrum-schema sync::tests`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add crates/schema/src/sync.rs crates/bin/src/main.rs crates/schema/tests/sync_it.rs
git commit -m "feat(schema): sync components before content types"
```

---

## Task 6: HTTP managed-lock guard for components

**Files:**
- Modify: `crates/http/src/routes/components.rs`

- [ ] **Step 1: Guard `update_one`**

In `crates/http/src/routes/components.rs`, at the start of `update_one` (before the `state.components.update(...)` call):

```rust
    if let Some(existing) = state.components.get(&uid).await {
        if existing.managed {
            return Err(ApiError(Error::Conflict(format!(
                "component `{uid}` is managed by a schema file; edit the TOML instead"
            ))));
        }
    }
```

- [ ] **Step 2: Guard `delete_one`**

In `delete_one`, after the `confirm` check and before computing `referencing` / calling delete:

```rust
    if let Some(existing) = state.components.get(&uid).await {
        if existing.managed {
            return Err(ApiError(Error::Conflict(format!(
                "component `{uid}` is managed by a schema file; edit the TOML instead"
            ))));
        }
    }
```

Confirm `Error` and `ApiError` are in scope (the file already uses `ApiError(Error::Validation(...))` in `delete_one`, so they are).

- [ ] **Step 3: Build**

Run: `cargo build -p ferrum-http`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/routes/components.rs
git commit -m "feat(http): reject PUT/DELETE on schema-file-managed components"
```

---

## Task 7: Integration tests for component sync

**Files:**
- Modify: `crates/schema/tests/sync_it.rs`

- [ ] **Step 1: Add a component+type fixture writer + tests**

Append to `crates/schema/tests/sync_it.rs`. Reuse the existing `setup_pool`, `service`, and the `comp_service` helper added in Task 5.

```rust
fn write_component_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut s = std::fs::File::create(dir.path().join("seo.toml")).unwrap();
    write!(s, r#"
[[component]]
uid = "shared.seo"
display_name = "SEO"
  [[component.field]]
  name = "meta_title"
  kind = "string"
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
  name = "seo"
  kind = "component"
  kind_meta = {{ component = "shared.seo", multiple = false }}
"#).unwrap();
    dir
}

#[tokio::test]
async fn sync_creates_component_then_type_marked_managed() {
    let pool = setup_pool().await;
    let svc = service(&pool).await;
    let comps = comp_service(&pool).await;
    let dir = write_component_dir();
    let path = dir.path().to_str().unwrap();

    sync_from_path(&svc, &comps, path, SyncMode::Additive).await.expect("sync");
    let seo = comps.registry().get("shared.seo").await.expect("component created");
    assert!(seo.managed, "synced component must be managed");
    let post = svc.registry().get("post").await.expect("type created");
    assert!(post.fields.iter().any(|f| f.name == "seo"), "component field present on type");

    // idempotent: second run, component unchanged
    sync_from_path(&svc, &comps, path, SyncMode::Additive).await.expect("re-sync");
    let seo2 = comps.registry().get("shared.seo").await.unwrap();
    assert_eq!(seo2.fields.len(), 1);
}

#[tokio::test]
async fn full_drop_of_referenced_component_errors() {
    let pool = setup_pool().await;
    let svc = service(&pool).await;
    let comps = comp_service(&pool).await;
    let dir = write_component_dir();
    let path = dir.path().to_str().unwrap();
    sync_from_path(&svc, &comps, path, SyncMode::Additive).await.unwrap();

    // New TOML dir that drops the component but keeps the type referencing it.
    let dir2 = tempfile::tempdir().unwrap();
    let mut p = std::fs::File::create(dir2.path().join("post.toml")).unwrap();
    write!(p, r#"
[[content_type]]
name = "post"
display_name = "Post"
  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true
  [[content_type.field]]
  name = "seo"
  kind = "component"
  kind_meta = {{ component = "shared.seo", multiple = false }}
"#).unwrap();
    let err = sync_from_path(&svc, &comps, dir2.path().to_str().unwrap(), SyncMode::Full).await;
    assert!(err.is_err(), "full-dropping a referenced component must error");
}
```

- [ ] **Step 2: Run the integration tests (Docker running)**

Run: `cargo test -p ferrum-schema --test sync_it`
Expected: all pass (the prior sync tests + the two new component tests). May take a while (containers).

- [ ] **Step 3: Commit**

```bash
git add crates/schema/tests/sync_it.rs
git commit -m "test(schema): integration tests for component sync"
```

---

## Task 8: Fixture — add a component to the blog preset

**Files:**
- Create: `examples/schema/blog/seo.toml`
- Modify: `examples/schema/blog/post.toml`

- [ ] **Step 1: Create the component fixture**

Create `examples/schema/blog/seo.toml`:

```toml
# Blog preset — reusable SEO component.
[[component]]
uid = "shared.seo"
display_name = "SEO"

  [[component.field]]
  name = "meta_title"
  kind = "string"

  [[component.field]]
  name = "meta_description"
  kind = "text"
```

- [ ] **Step 2: Reference it from post.toml**

In `examples/schema/blog/post.toml`, add a field to the `post` type (after the existing fields, inside the same `[[content_type]]`):

```toml
  [[content_type.field]]
  name = "seo"
  kind = "component"
  kind_meta = { component = "shared.seo", multiple = false }
```

- [ ] **Step 3: Verify the preset still loads + orders**

The existing `blog_preset_parses_and_orders` test calls `load_desired` on this dir. Since `load_desired` now returns `ParsedSchema`, that test was updated in Task 5 (or update it now): it should assert `desired.content_types` for the ordering and may assert `desired.components` contains `shared.seo`. Update the test body to:

```rust
    #[test]
    fn blog_preset_parses_and_orders() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/schema/blog");
        let desired = super::load_desired(&dir).expect("load blog preset");
        let names: Vec<&str> = desired.content_types.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"author"));
        assert!(names.contains(&"post"));
        assert!(desired.components.iter().any(|c| c.uid == "shared.seo"));
        for c in &desired.content_types {
            c.validate().expect("preset type valid");
        }
        let ordered = super::order_creates(desired.content_types);
        let pos = |n: &str| ordered.iter().position(|c| c.name == n).unwrap();
        assert!(pos("author") < pos("post"), "author must be created before post");
    }
```

- [ ] **Step 4: Run the preset test**

Run: `cargo test -p ferrum-schema sync::tests::blog_preset_parses_and_orders`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add examples/schema/blog/seo.toml examples/schema/blog/post.toml crates/schema/src/sync.rs
git commit -m "feat(schema): add SEO component to blog preset fixture"
```

---

## Task 9: UI — lock managed components in the component editor

**Files:**
- Modify: `ui/src/api/types.ts`
- Modify: `ui/src/screens/ComponentEditor.tsx`

- [ ] **Step 1: Add the helper**

In `ui/src/api/types.ts`, the component type now has a `managed` boolean (the API returns it). Add a helper next to `managedType`:

```ts
export function managedComponent(c: { managed?: boolean }): boolean {
  return c.managed === true;
}
```

(If the existing TS component type doesn't declare `managed`, add `managed?: boolean` to that interface so it type-checks.)

- [ ] **Step 2: Gate the component editor**

In `ui/src/screens/ComponentEditor.tsx`, mirror `SchemaEditor.tsx`'s managed handling:
1. Import `managedComponent` from `../api/types`.
2. Compute `const isManaged = <loadedComponent> ? managedComponent(<loadedComponent>) : false;` using the actual loaded-component variable in this file (inspect how the component is loaded — likely from the builder draft's server snapshot, like SchemaEditor's `snapshot`).
3. Render a `Notice` when `isManaged`:
   ```tsx
   {isManaged && (
     <Notice tone="ok">
       Managed by a schema file — edit the TOML and restart to change this component.
     </Notice>
   )}
   ```
   (Match the `Notice` prop API used in SchemaEditor — it uses `tone="ok"`.)
4. Disable: the display-name input, save (SaveBar `disabled={isManaged}`), delete, and field add/edit/remove — exactly as SchemaEditor does. SaveBar already supports a `disabled` prop (added for content types). For field edit/remove, wrap callbacks `() => { if (!isManaged) ... }` and pass `disabled={isManaged}` to the add button, matching SchemaEditor.

- [ ] **Step 3: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add ui/src/api/types.ts ui/src/screens/ComponentEditor.tsx
git commit -m "feat(ui): lock schema-file-managed components in the editor"
```

---

## Task 10: Docs + full verification

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Document components in the schema-as-code section**

In `README.md`'s "Schema as code" section, add a short paragraph + example showing `[[component]]` blocks and referencing one via a `component` field. State: components are synced before content types; managed components are read-only in the UI/API; `full` mode will not drop a component still referenced by a type (sync aborts).

```toml
[[component]]
uid = "shared.seo"
display_name = "SEO"

  [[component.field]]
  name = "meta_title"
  kind = "string"
```

- [ ] **Step 2: Commit docs**

```bash
git add README.md
git commit -m "docs: components in schema files"
```

- [ ] **Step 3: Full backend suite (Docker running)**

Run: `cargo test --workspace`
Expected: pass. (A `PoolTimedOut` flake under heavy parallel container load is known; re-run the single failing test in isolation to confirm if it appears.)

- [ ] **Step 4: Clippy + fmt**

Run: `cargo clippy --workspace --all-targets && cargo fmt --all -- --check`
Expected: clean. Run `cargo fmt --all` if the check fails, then commit the fmt.

- [ ] **Step 5: UI typecheck + build**

Run: `cd ui && pnpm typecheck && pnpm build`
Expected: clean.

- [ ] **Step 6: Manual smoke (optional)**

With Docker Postgres + env set:

Run: `FERRUM_SCHEMA_DIR=examples/schema/blog cargo run -p ferrum`
Expected: log `schema sync complete`; `/admin/components` lists `shared.seo` (managed); `post` has a `seo` component field; PUT on `shared.seo` returns 409.

- [ ] **Step 7: Final fmt commit (only if fmt changed files)**

```bash
git add -A
git commit -m "chore: fmt"
```
