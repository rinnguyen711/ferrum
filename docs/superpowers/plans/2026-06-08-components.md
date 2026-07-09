# Component Field Types Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a component registry (`_components` table, `/admin/components` CRUD) and a `component` field kind that stores validated structured sub-objects as `jsonb` in entry rows, with single and repeatable (`multiple: true`) variants.

**Architecture:** New `ComponentStore` in `crates/sql`, `ComponentService` in `crates/schema`, HTTP router in `crates/http/src/routes/components.rs`. `FieldKind::Component` stores data as `BoundValue::Json` (no new variant). Validation runs in the content write path before `body_to_binds`. `getContentType` injects `_component_fields` into the response. UI adds a Component Builder screen and inline component editors in the Entry Editor.

**Tech Stack:** Rust (sqlx, axum, serde_json), React + TypeScript (existing patterns).

Spec: `docs/superpowers/specs/2026-06-08-components-design.md`

---

## File structure

**New files:**
- `crates/schema/migrations/0005_components.sql` — `_components` table
- `crates/sql/src/component.rs` — `ComponentStore`: CRUD against `_components`
- `crates/schema/src/component.rs` — `ComponentService`: validation + referential integrity
- `crates/http/src/routes/components.rs` — axum router for 5 admin endpoints
- `crates/bin/tests/components.rs` — integration tests
- `ui/src/screens/ComponentBuilder.tsx` — new admin screen
- `ui/src/screens/ComponentEditor.tsx` — create/edit panel for a single component

**Modified files:**
- `crates/core/src/field.rs` — add `FieldKind::Component`, `ComponentMeta`, `component_meta()`, errors, `Field::validate` arm, `BoundValue::from_json` arm, `column_def` arm
- `crates/sql/src/ddl.rs` — `column_def` emits `jsonb` for `Component`; `render_default` arm
- `crates/sql/src/lib.rs` — re-export `ComponentStore`
- `crates/sql/src/filter.rs` — `op_allows_kind`: `Component` allows no operators
- `crates/schema/src/lib.rs` — re-export `ComponentService`; add `ComponentRegistry`; extend `MIGRATOR` (automatic via `sqlx::migrate!`)
- `crates/schema/src/registry.rs` — add `ComponentRegistry` (in-memory map, same `Arc<RwLock<HashMap>>` pattern)
- `crates/schema/src/service.rs` — `SchemaService` injects `_component_fields` on `get` / `list`; check component uid valid on `create`/`patch`
- `crates/http/src/state.rs` — `AppState` gains `components: ComponentService`
- `crates/http/src/lib.rs` — export `ComponentService`
- `crates/http/src/routes/mod.rs` — merge `components::router()` into protected router
- `crates/http/src/routes/content.rs` — call `validate_component_fields` before `body_to_binds`
- `crates/bin/src/main.rs` — wire `ComponentService` into `AppState`
- `crates/bin/tests/common/mod.rs` — expose `ComponentService` on `TestApp`
- `ui/src/api/types.ts` — add `"component"` to `FieldKind`, `ComponentMeta`, `componentMeta()`; extend `Field` with `_component_fields?`
- `ui/src/api/endpoints.ts` — add component CRUD calls
- `ui/src/builder/draftModel.ts` — add `"component"` to `KINDS`/`FIELD_CARDS`; extend `DraftField` with `componentUid`, `componentMultiple`; update `draftFieldToField`, `blankField`, `seedFromContentType`
- `ui/src/builder/FieldConfigModal.tsx` — add component kind config UI (uid input, multiple toggle)
- `ui/src/screens/EntryEditor.tsx` — add `ComponentField` input in `FieldInput` dispatch
- `ui/src/app.tsx` — add `/components` and `/components/:uid` routes
- `ui/src/components/shell.tsx` — add "Component Library" nav item

---

## Task 1: DB migration — `_components` table

**Files:**
- Create: `crates/schema/migrations/0005_components.sql`

- [ ] **Step 1: Write the migration**

```sql
CREATE TABLE IF NOT EXISTS _components (
    uid          TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    fields       JSONB NOT NULL DEFAULT '[]'
);
```

- [ ] **Step 2: Verify it applies**

```bash
cargo build -p ferrum-schema 2>&1 | tail -5
```

Expected: compiles without error (sqlx embed macro picks up the new file).

- [ ] **Step 3: Commit**

```bash
git add crates/schema/migrations/0005_components.sql
git commit -m "feat(sql): add _components migration"
```

---

## Task 2: `FieldKind::Component` and `ComponentMeta` in `core`

**Files:**
- Modify: `crates/core/src/field.rs`

- [ ] **Step 1: Add `Component` variant to `FieldKind`**

In `crates/core/src/field.rs`, after the `RichText` variant:

```rust
/// Phase N: structured sub-object backed by a registered component shape.
/// Configuration in `Field.kind_meta`; see `ComponentMeta`.
/// Stored as `jsonb` (single object or array when `multiple: true`).
Component,
```

Serde uses `rename_all = "lowercase"` on the enum — `Component` serializes as `"component"` automatically.

- [ ] **Step 2: Add `ComponentMeta` struct**

After the `MediaMeta` impl block (around line 608), add:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentMeta {
    pub component: String,
    pub multiple: bool,
}

impl ComponentMeta {
    pub fn from_value(v: &serde_json::Value) -> Result<Self, FieldError> {
        let obj = v.as_object().ok_or(FieldError::ComponentMetaShape)?;
        for key in obj.keys() {
            if !matches!(key.as_str(), "component" | "multiple") {
                return Err(FieldError::ComponentMetaShape);
            }
        }
        let component = obj
            .get("component")
            .and_then(|x| x.as_str())
            .ok_or(FieldError::ComponentMetaShape)?
            .to_string();
        if component.is_empty() {
            return Err(FieldError::ComponentMetaShape);
        }
        let multiple = match obj.get("multiple") {
            None => false,
            Some(serde_json::Value::Bool(b)) => *b,
            Some(_) => return Err(FieldError::ComponentMetaShape),
        };
        Ok(Self { component, multiple })
    }
}
```

- [ ] **Step 3: Add error variants to `FieldError`**

In the `FieldError` enum, after `MediaFieldRequiredUnsupported`:

```rust
#[error("component kind_meta must be {{component: \"uid\", multiple?: bool}}")]
ComponentMetaShape,
#[error("component field cannot be unique")]
ComponentFieldUniqueUnsupported,
#[error("component field cannot have a default")]
ComponentFieldDefaultUnsupported,
```

- [ ] **Step 4: Add `Field::validate` arm for `Component`**

In `Field::validate`, after the `FieldKind::Media` arm (before the `Email | Url | Slug` arm):

```rust
if self.kind == FieldKind::Component {
    if self.unique {
        return Err(FieldError::ComponentFieldUniqueUnsupported);
    }
    if !self.default.is_null() {
        return Err(FieldError::ComponentFieldDefaultUnsupported);
    }
    ComponentMeta::from_value(&self.kind_meta)?;
    return Ok(());
}
```

- [ ] **Step 5: Add `BoundValue::from_json` arm for `Component`**

In `BoundValue::from_json`, after the `(FieldKind::RichText, v)` arm:

```rust
(FieldKind::Component, v) => Ok(BoundValue::Json(v.clone())),
```

- [ ] **Step 6: Add `component_meta()` method on `Field`**

After the `media_meta()` method:

```rust
pub fn component_meta(&self) -> Option<ComponentMeta> {
    if self.kind == FieldKind::Component {
        ComponentMeta::from_value(&self.kind_meta).ok()
    } else {
        None
    }
}
```

- [ ] **Step 7: Export `ComponentMeta` from `crates/core/src/lib.rs`**

In `crates/core/src/lib.rs`, extend the `field` pub use line:

```rust
pub use field::{
    BoundValue, Cardinality, CoerceError, ComponentMeta, EnumMeta, Field, FieldError, FieldKind, RelationMeta,
};
```

- [ ] **Step 8: Write unit tests**

At the bottom of the `field_tests` module in `crates/core/src/field.rs`:

```rust
#[test]
fn component_meta_parses_valid() {
    let v = serde_json::json!({"component": "shared.hero", "multiple": false});
    let m = ComponentMeta::from_value(&v).unwrap();
    assert_eq!(m.component, "shared.hero");
    assert!(!m.multiple);
}

#[test]
fn component_meta_defaults_multiple_false() {
    let v = serde_json::json!({"component": "shared.hero"});
    let m = ComponentMeta::from_value(&v).unwrap();
    assert!(!m.multiple);
}

#[test]
fn component_meta_rejects_unknown_key() {
    let v = serde_json::json!({"component": "shared.hero", "extra": 1});
    assert_eq!(ComponentMeta::from_value(&v).unwrap_err(), FieldError::ComponentMetaShape);
}

#[test]
fn component_meta_rejects_empty_uid() {
    let v = serde_json::json!({"component": ""});
    assert_eq!(ComponentMeta::from_value(&v).unwrap_err(), FieldError::ComponentMetaShape);
}

#[test]
fn component_field_validate_rejects_unique() {
    let f = Field {
        name: "hero".into(),
        kind: FieldKind::Component,
        required: false,
        unique: true,
        default: serde_json::json!(null),
        max_length: None,
        kind_meta: serde_json::json!({"component": "shared.hero"}),
    };
    assert_eq!(f.validate().unwrap_err(), FieldError::ComponentFieldUniqueUnsupported);
}

#[test]
fn component_field_validate_rejects_default() {
    let f = Field {
        name: "hero".into(),
        kind: FieldKind::Component,
        required: false,
        unique: false,
        default: serde_json::json!({"title": "hi"}),
        max_length: None,
        kind_meta: serde_json::json!({"component": "shared.hero"}),
    };
    assert_eq!(f.validate().unwrap_err(), FieldError::ComponentFieldDefaultUnsupported);
}

#[test]
fn component_field_validate_ok() {
    let f = Field {
        name: "hero".into(),
        kind: FieldKind::Component,
        required: false,
        unique: false,
        default: serde_json::json!(null),
        max_length: None,
        kind_meta: serde_json::json!({"component": "shared.hero", "multiple": true}),
    };
    assert!(f.validate().is_ok());
}
```

- [ ] **Step 9: Run tests**

```bash
cargo test -p ferrum-core 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 10: Commit**

```bash
git add crates/core/src/field.rs crates/core/src/lib.rs
git commit -m "feat(core): add FieldKind::Component, ComponentMeta, component_meta()"
```

---

## Task 3: DDL and filter support for `Component`

**Files:**
- Modify: `crates/sql/src/ddl.rs`
- Modify: `crates/sql/src/filter.rs`

- [ ] **Step 1: Add `Component` to `column_def` in `ddl.rs`**

In `crates/sql/src/ddl.rs`, the `column_def` function has:

```rust
if f.kind == FieldKind::Json || f.kind == FieldKind::RichText {
```

Change it to:

```rust
if f.kind == FieldKind::Json || f.kind == FieldKind::RichText || f.kind == FieldKind::Component {
```

- [ ] **Step 2: Add `Component` to `render_default` in `ddl.rs`**

In `render_default`, there is:

```rust
(FieldKind::RichText, v) if !v.is_null() => {
    let s = serde_json::to_string(v).unwrap_or_default();
    format!("'{s}'::jsonb")
}
```

After that arm, add:

```rust
(FieldKind::Component, v) if !v.is_null() => {
    let s = serde_json::to_string(v).unwrap_or_default();
    format!("'{s}'::jsonb")
}
```

- [ ] **Step 3: Block filter operators on `Component` in `filter.rs`**

In `crates/sql/src/filter.rs`, find the `op_allows_kind` function. It should have arms for various kinds. Add a guard for `Component` (it allows no filter operators, same treatment as `Media`):

```rust
if kind == FieldKind::Component {
    return false;
}
```

Add this at the top of the `op_allows_kind` function body, before the existing arms.

- [ ] **Step 4: Run tests**

```bash
cargo test -p ferrum-sql 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/sql/src/ddl.rs crates/sql/src/filter.rs
git commit -m "feat(sql): DDL and filter support for Component kind"
```

---

## Task 4: `ComponentStore` in `crates/sql`

**Files:**
- Create: `crates/sql/src/component.rs`
- Modify: `crates/sql/src/lib.rs`

- [ ] **Step 1: Write `ComponentStore`**

Create `crates/sql/src/component.rs`:

```rust
//! CRUD against the `_components` table.

use ferrum_core::Field;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub uid: String,
    pub display_name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct ComponentStore {
    pool: PgPool,
}

impl ComponentStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Component>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RawComponent>(
            "SELECT uid, display_name, fields FROM _components ORDER BY uid",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_component()).collect())
    }

    pub async fn get(&self, uid: &str) -> Result<Option<Component>, sqlx::Error> {
        let row = sqlx::query_as::<_, RawComponent>(
            "SELECT uid, display_name, fields FROM _components WHERE uid = $1",
        )
        .bind(uid)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_component()))
    }

    pub async fn create(&self, uid: &str, display_name: &str, fields: &[Field]) -> Result<Component, sqlx::Error> {
        sqlx::query(
            "INSERT INTO _components (uid, display_name, fields) VALUES ($1, $2, $3)",
        )
        .bind(uid)
        .bind(display_name)
        .bind(sqlx::types::Json(fields))
        .execute(&self.pool)
        .await?;
        Ok(Component {
            uid: uid.to_string(),
            display_name: display_name.to_string(),
            fields: fields.to_vec(),
        })
    }

    pub async fn update(&self, uid: &str, display_name: &str, fields: &[Field]) -> Result<Option<Component>, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE _components SET display_name = $1, fields = $2 WHERE uid = $3",
        )
        .bind(display_name)
        .bind(sqlx::types::Json(fields))
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
        }))
    }

    pub async fn delete(&self, uid: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM _components WHERE uid = $1")
            .bind(uid)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct RawComponent {
    uid: String,
    display_name: String,
    fields: sqlx::types::Json<Vec<Field>>,
}

impl RawComponent {
    fn into_component(self) -> Component {
        Component {
            uid: self.uid,
            display_name: self.display_name,
            fields: self.fields.0,
        }
    }
}
```

- [ ] **Step 2: Export from `crates/sql/src/lib.rs`**

Add at the bottom of `crates/sql/src/lib.rs`:

```rust
pub mod component;
pub use component::{Component, ComponentStore};
```

- [ ] **Step 3: Build**

```bash
cargo build -p ferrum-sql 2>&1 | tail -10
```

Expected: compiles without error.

- [ ] **Step 4: Commit**

```bash
git add crates/sql/src/component.rs crates/sql/src/lib.rs
git commit -m "feat(sql): add ComponentStore"
```

---

## Task 5: `ComponentService` and `ComponentRegistry` in `crates/schema`

**Files:**
- Create: `crates/schema/src/component.rs`
- Modify: `crates/schema/src/lib.rs`

- [ ] **Step 1: Write `ComponentRegistry` and `ComponentService`**

Create `crates/schema/src/component.rs`:

```rust
//! In-memory component registry + transactional service.

use ferrum_core::{Error, Field, FieldKind, ValidationErrors};
use ferrum_sql::{Component, ComponentStore};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Allowed inner field kinds for component definitions.
const ALLOWED_INNER_KINDS: &[FieldKind] = &[
    FieldKind::String,
    FieldKind::Text,
    FieldKind::Integer,
    FieldKind::Float,
    FieldKind::Boolean,
    FieldKind::Datetime,
    FieldKind::Email,
    FieldKind::Url,
    FieldKind::Slug,
    FieldKind::Enum,
    FieldKind::Json,
    FieldKind::RichText,
    FieldKind::Media,
];

/// In-memory cache of all components. Keyed by uid.
#[derive(Clone, Default)]
pub struct ComponentRegistry {
    inner: Arc<RwLock<HashMap<String, Component>>>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(&self, uid: &str) -> Option<Component> {
        self.inner.read().await.get(uid).cloned()
    }

    pub async fn list(&self) -> Vec<Component> {
        let mut out: Vec<_> = self.inner.read().await.values().cloned().collect();
        out.sort_by(|a, b| a.uid.cmp(&b.uid));
        out
    }

    pub async fn insert(&self, c: Component) {
        self.inner.write().await.insert(c.uid.clone(), c);
    }

    pub async fn remove(&self, uid: &str) {
        self.inner.write().await.remove(uid);
    }

    pub async fn reload_from_db(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        let store = ComponentStore::new(pool.clone());
        let all = store.list().await?;
        let mut map = HashMap::with_capacity(all.len());
        for c in all {
            map.insert(c.uid.clone(), c);
        }
        *self.inner.write().await = map;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ComponentService {
    store: ComponentStore,
    registry: ComponentRegistry,
}

impl ComponentService {
    pub fn new(pool: PgPool, registry: ComponentRegistry) -> Self {
        Self {
            store: ComponentStore::new(pool),
            registry,
        }
    }

    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }

    pub async fn list(&self) -> Vec<Component> {
        self.registry.list().await
    }

    pub async fn get(&self, uid: &str) -> Option<Component> {
        self.registry.get(uid).await
    }

    pub async fn create(&self, uid: &str, display_name: &str, fields: Vec<Field>) -> Result<Component, Error> {
        validate_uid(uid)?;
        validate_inner_fields(&fields)?;
        for f in &fields {
            f.validate().map_err(|e| Error::Validation(ValidationErrors::field(&f.name, e.to_string())))?;
        }
        if self.registry.get(uid).await.is_some() {
            return Err(Error::Conflict(format!("component `{}` already exists", uid)));
        }
        let c = self.store.create(uid, display_name, &fields).await.map_err(internal)?;
        self.registry.insert(c.clone()).await;
        Ok(c)
    }

    pub async fn update(&self, uid: &str, display_name: &str, fields: Vec<Field>) -> Result<Component, Error> {
        validate_inner_fields(&fields)?;
        for f in &fields {
            f.validate().map_err(|e| Error::Validation(ValidationErrors::field(&f.name, e.to_string())))?;
        }
        self.store
            .update(uid, display_name, &fields)
            .await
            .map_err(internal)?
            .ok_or(Error::NotFound)
            .map(|c| {
                let c = c.clone();
                let registry = self.registry.clone();
                tokio::spawn(async move { registry.insert(c).await });
                c
            })
    }

    pub async fn delete(&self, uid: &str, referencing_types: &[String]) -> Result<(), Error> {
        if !referencing_types.is_empty() {
            return Err(Error::Conflict(format!(
                "component `{}` is referenced by: {}",
                uid,
                referencing_types.join(", ")
            )));
        }
        let deleted = self.store.delete(uid).await.map_err(internal)?;
        if !deleted {
            return Err(Error::NotFound);
        }
        self.registry.remove(uid).await;
        Ok(())
    }
}

/// uid must match `category.name` — two dot-separated lowercase ident segments.
fn validate_uid(uid: &str) -> Result<(), Error> {
    let parts: Vec<&str> = uid.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err(Error::Validation(ValidationErrors::field(
            "uid",
            "uid must be two dot-separated segments, e.g. \"shared.hero_block\"",
        )));
    }
    for p in &parts {
        if !ferrum_core::reserved::is_valid_ident(p) {
            return Err(Error::Validation(ValidationErrors::field(
                "uid",
                format!("uid segment `{p}` is not a valid identifier (^[a-z][a-z0-9_]{{0,62}}$)"),
            )));
        }
    }
    Ok(())
}

fn validate_inner_fields(fields: &[Field]) -> Result<(), Error> {
    for f in fields {
        if !ALLOWED_INNER_KINDS.contains(&f.kind) {
            return Err(Error::Validation(ValidationErrors::field(
                &f.name,
                format!(
                    "field kind `{:?}` is not allowed inside a component; use scalar or media kinds",
                    f.kind
                ),
            )));
        }
    }
    Ok(())
}

fn internal(e: sqlx::Error) -> Error {
    Error::Internal(anyhow::anyhow!(e))
}
```

- [ ] **Step 2: Fix the update spawn pattern**

The `tokio::spawn` in `update` above has a subtle issue — it clones `c` twice. Replace the `update` body with:

```rust
pub async fn update(&self, uid: &str, display_name: &str, fields: Vec<Field>) -> Result<Component, Error> {
    validate_inner_fields(&fields)?;
    for f in &fields {
        f.validate().map_err(|e| Error::Validation(ValidationErrors::field(&f.name, e.to_string())))?;
    }
    let c = self.store
        .update(uid, display_name, &fields)
        .await
        .map_err(internal)?
        .ok_or(Error::NotFound)?;
    self.registry.insert(c.clone()).await;
    Ok(c)
}
```

- [ ] **Step 3: Export from `crates/schema/src/lib.rs`**

Add after the existing exports:

```rust
pub mod component;
pub use component::{ComponentRegistry, ComponentService};
```

- [ ] **Step 4: Build**

```bash
cargo build -p ferrum-schema 2>&1 | tail -10
```

Expected: compiles without error.

- [ ] **Step 5: Commit**

```bash
git add crates/schema/src/component.rs crates/schema/src/lib.rs
git commit -m "feat(schema): add ComponentRegistry and ComponentService"
```

---

## Task 6: Wire `ComponentService` into `AppState` and boot

**Files:**
- Modify: `crates/http/src/state.rs`
- Modify: `crates/http/src/lib.rs`
- Modify: `crates/bin/src/main.rs`

- [ ] **Step 1: Add `components` field to `AppState`**

In `crates/http/src/state.rs`, add the import:

```rust
use ferrum_schema::{ComponentService, SchemaService};
```

(Replace the existing `use ferrum_schema::SchemaService;` line.)

Then in the `AppState` struct, after `schemas: SchemaService`:

```rust
pub components: ComponentService,
```

- [ ] **Step 2: Export `ComponentService` from `crates/http/src/lib.rs`**

In `crates/http/src/lib.rs`, extend the state re-export:

```rust
pub use state::{
    AlwaysAllow, AppConfig, AppState, Authz, EventSink, NoopHook, NoopSink, RoleAuthz,
    WriteContext, WriteHook, WriteOp,
};
```

No change needed here since `ComponentService` is exported from `ferrum_schema` directly. Callers that need it import from `ferrum_schema`.

- [ ] **Step 3: Wire `ComponentService` in `crates/bin/src/main.rs`**

In `main.rs`, add to the imports:

```rust
use ferrum_schema::{ComponentRegistry, ComponentService, SchemaRegistry, SchemaService, MIGRATOR};
```

(Replace the existing `use ferrum_schema::{SchemaRegistry, SchemaService, MIGRATOR};`.)

After `let schemas = SchemaService::new(pool.clone(), registry.clone());`, add:

```rust
let component_registry = ComponentRegistry::new();
component_registry.reload_from_db(&pool).await.context("hydrate component registry")?;
let components = ComponentService::new(pool.clone(), component_registry);
```

In the `AppState { .. }` literal, add:

```rust
components,
```

- [ ] **Step 4: Wire `ComponentService` in `crates/bin/tests/common/mod.rs`**

Add to imports:

```rust
use ferrum_schema::{ComponentRegistry, ComponentService, SchemaRegistry, SchemaService, MIGRATOR};
```

Add field to `TestApp`:

```rust
pub components: ComponentService,
```

In `spawn_full`, after `let schemas = SchemaService::new(pool.clone(), registry.clone());`:

```rust
let component_registry = ComponentRegistry::new();
component_registry.reload_from_db(&pool).await.expect("hydrate components");
let components = ComponentService::new(pool.clone(), component_registry.clone());
```

In the `AppState` literal, add `components: components.clone()`. Store on `TestApp`:

```rust
Self {
    base_url,
    pool,
    client,
    schemas,
    components,
    token,
    _pg: pg,
    _shutdown: tx,
}
```

- [ ] **Step 5: Build**

```bash
cargo build --workspace 2>&1 | tail -15
```

Expected: compiles without error.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/state.rs crates/http/src/lib.rs crates/bin/src/main.rs crates/bin/tests/common/mod.rs
git commit -m "feat(http): add ComponentService to AppState"
```

---

## Task 7: HTTP router for `/admin/components`

**Files:**
- Create: `crates/http/src/routes/components.rs`
- Modify: `crates/http/src/routes/mod.rs`

- [ ] **Step 1: Write the router**

Create `crates/http/src/routes/components.rs`:

```rust
//! /admin/components/* handlers.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put, delete};
use axum::{Json, Router};
use ferrum_core::{Error, Field};
use ferrum_sql::Component;
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/components", get(list).post(create))
        .route("/admin/components/:uid", get(get_one).put(update_one).delete(delete_one))
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<Component>>, ApiError> {
    Ok(Json(state.components.list().await))
}

async fn get_one(
    State(state): State<AppState>,
    Path(uid): Path<String>,
) -> Result<Json<Component>, ApiError> {
    state
        .components
        .get(&uid)
        .await
        .map(Json)
        .ok_or(ApiError(Error::NotFound))
}

#[derive(Debug, Deserialize)]
struct ComponentPayload {
    uid: String,
    display_name: String,
    #[serde(default)]
    fields: Vec<Field>,
}

#[derive(Debug, Deserialize)]
struct UpdatePayload {
    display_name: String,
    #[serde(default)]
    fields: Vec<Field>,
}

async fn create(
    State(state): State<AppState>,
    Json(payload): Json<ComponentPayload>,
) -> Result<(StatusCode, Json<Component>), ApiError> {
    let c = state.components.create(&payload.uid, &payload.display_name, payload.fields).await?;
    Ok((StatusCode::CREATED, Json(c)))
}

async fn update_one(
    State(state): State<AppState>,
    Path(uid): Path<String>,
    Json(payload): Json<UpdatePayload>,
) -> Result<Json<Component>, ApiError> {
    let c = state.components.update(&uid, &payload.display_name, payload.fields).await?;
    Ok(Json(c))
}

#[derive(Deserialize)]
struct DeleteQuery {
    confirm: Option<bool>,
}

async fn delete_one(
    State(state): State<AppState>,
    Path(uid): Path<String>,
    axum::extract::Query(q): axum::extract::Query<DeleteQuery>,
) -> Result<StatusCode, ApiError> {
    if q.confirm != Some(true) {
        return Err(ApiError(Error::Validation(
            ferrum_core::ValidationErrors::single("confirm_required: pass ?confirm=true"),
        )));
    }
    // Check referential integrity: find all content types that reference this uid.
    let referencing: Vec<String> = state
        .schemas
        .registry()
        .list()
        .await
        .into_iter()
        .filter(|ct| {
            ct.fields.iter().any(|f| {
                f.component_meta()
                    .map(|m| m.component == uid)
                    .unwrap_or(false)
            })
        })
        .map(|ct| ct.name)
        .collect();
    state.components.delete(&uid, &referencing).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 2: Mount in `routes/mod.rs`**

In `crates/http/src/routes/mod.rs`, add:

```rust
pub mod components;
```

And in `build_router`, add to the `protected` router:

```rust
.merge(components::router())
```

after `.merge(media::router())`.

- [ ] **Step 3: Build**

```bash
cargo build -p ferrum-http 2>&1 | tail -10
```

Expected: compiles without error.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/routes/components.rs crates/http/src/routes/mod.rs
git commit -m "feat(http): add /admin/components CRUD router"
```

---

## Task 8: Component validation in the content write path

**Files:**
- Modify: `crates/http/src/routes/content.rs`

- [ ] **Step 1: Add `validate_component_fields` function**

At the bottom of `crates/http/src/routes/content.rs` (before the `#[cfg(test)]` block if any, else at the end), add:

```rust
/// Validate all component fields in the request body against their registered
/// schemas. Called for both create and update before `body_to_binds`.
async fn validate_component_fields(
    state: &AppState,
    ct: &ferrum_core::ContentType,
    body: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), ApiError> {
    use ferrum_core::{BoundValue, FieldKind};

    for f in &ct.fields {
        let Some(meta) = f.component_meta() else { continue };
        let component = state
            .components
            .get(&meta.component)
            .await
            .ok_or_else(|| {
                ApiError(Error::Validation(ferrum_core::ValidationErrors::field(
                    &f.name,
                    format!("component `{}` not found in registry", meta.component),
                )))
            })?;

        let raw = body.get(&f.name);

        // required outer check
        if f.required && (raw.is_none() || raw == Some(&serde_json::Value::Null)) {
            return Err(ApiError(Error::Validation(ferrum_core::ValidationErrors::field(
                &f.name,
                "field is required",
            ))));
        }

        let Some(raw) = raw else { continue };
        if raw.is_null() { continue; }

        if meta.multiple {
            let arr = raw.as_array().ok_or_else(|| {
                ApiError(Error::Validation(ferrum_core::ValidationErrors::field(
                    &f.name,
                    "repeatable component field must be an array",
                )))
            })?;
            for (i, item) in arr.iter().enumerate() {
                validate_component_instance(item, &component.fields, &format!("{}[{}]", f.name, i))?;
            }
        } else {
            validate_component_instance(raw, &component.fields, &f.name)?;
        }
    }
    Ok(())
}

fn validate_component_instance(
    value: &serde_json::Value,
    fields: &[ferrum_core::Field],
    path_prefix: &str,
) -> Result<(), ApiError> {
    use ferrum_core::BoundValue;
    let obj = value.as_object().ok_or_else(|| {
        ApiError(Error::Validation(ferrum_core::ValidationErrors::field(
            path_prefix,
            "component instance must be an object",
        )))
    })?;

    for f in fields {
        let field_path = format!("{}.{}", path_prefix, f.name);
        let v = obj.get(&f.name).unwrap_or(&serde_json::Value::Null);

        if f.required && v.is_null() {
            return Err(ApiError(Error::Validation(ferrum_core::ValidationErrors::field(
                &field_path,
                "field is required",
            ))));
        }
        if !v.is_null() {
            BoundValue::from_json(f.kind, v).map_err(|_| {
                ApiError(Error::Validation(ferrum_core::ValidationErrors::field(
                    &field_path,
                    format!("invalid value for kind {:?}", f.kind),
                )))
            })?;
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Call `validate_component_fields` in `create`**

In the `create` handler, after:

```rust
let body = state.hooks.before_write(&ctx, body).await.map_err(ApiError)?;
```

Add:

```rust
validate_component_fields(&state, &ct, &body).await?;
```

- [ ] **Step 3: Call `validate_component_fields` in `update`**

In the `update` handler, after:

```rust
let body = state.hooks.before_write(&ctx, body).await.map_err(ApiError)?;
```

Add:

```rust
validate_component_fields(&state, &ct, &body).await?;
```

- [ ] **Step 4: Build**

```bash
cargo build -p ferrum-http 2>&1 | tail -10
```

Expected: compiles without error.

- [ ] **Step 5: Commit**

```bash
git add crates/http/src/routes/content.rs
git commit -m "feat(http): validate component fields in content write path"
```

---

## Task 9: Inject `_component_fields` into `getContentType` response

**Files:**
- Modify: `crates/schema/src/service.rs`
- Modify: `crates/http/src/routes/schema.rs`

The simplest approach: `SchemaService` doesn't own `ComponentService`, so we inject component fields at the HTTP layer in the schema route handlers. The route handlers have access to `AppState` which has both.

- [ ] **Step 1: Add a helper to inject `_component_fields`**

In `crates/http/src/routes/schema.rs`, add at the bottom:

```rust
/// Inject `_component_fields` into every component-kind field on a ContentType.
async fn inject_component_fields(
    state: &AppState,
    mut ct: ferrum_core::ContentType,
) -> ferrum_core::ContentType {
    use ferrum_core::FieldKind;
    use serde_json::json;

    for f in &mut ct.fields {
        if f.kind != FieldKind::Component { continue; }
        let Some(meta) = f.component_meta() else { continue };
        if let Some(comp) = state.components.get(&meta.component).await {
            let fields_json = serde_json::to_value(&comp.fields).unwrap_or(json!([]));
            if let serde_json::Value::Object(ref mut m) = f.kind_meta {
                m.insert("_component_fields".into(), fields_json);
            } else {
                f.kind_meta = json!({
                    "component": meta.component,
                    "multiple": meta.multiple,
                    "_component_fields": fields_json,
                });
            }
        }
    }
    ct
}
```

- [ ] **Step 2: Use the helper in `get_one` and `list`**

Replace the `get_one` handler:

```rust
async fn get_one(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ContentType>, ApiError> {
    let ct = state
        .schemas
        .registry()
        .get(&name)
        .await
        .ok_or(ApiError(Error::NotFound))?;
    Ok(Json(inject_component_fields(&state, ct).await))
}
```

Replace the `list` handler:

```rust
async fn list(State(state): State<AppState>) -> Result<Json<Vec<ContentType>>, ApiError> {
    let cts = state.schemas.registry().list().await;
    let mut out = Vec::with_capacity(cts.len());
    for ct in cts {
        out.push(inject_component_fields(&state, ct).await);
    }
    Ok(Json(out))
}
```

- [ ] **Step 3: Build**

```bash
cargo build -p ferrum-http 2>&1 | tail -10
```

Expected: compiles without error.

- [ ] **Step 4: Commit**

```bash
git add crates/http/src/routes/schema.rs
git commit -m "feat(http): inject _component_fields into getContentType response"
```

---

## Task 10: Integration tests

**Files:**
- Create: `crates/bin/tests/components.rs`

- [ ] **Step 1: Write the integration tests**

Create `crates/bin/tests/components.rs`:

```rust
mod common;

use common::TestApp;
use serde_json::json;

// ----- helpers -----

async fn make_hero_component(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/components")))
        .json(&json!({
            "uid": "shared.hero",
            "display_name": "Hero Block",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "subtitle", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

async fn make_article_type(app: &TestApp) {
    let resp = app
        .admin(app.client.post(app.url("/admin/content-types")))
        .json(&json!({
            "name": "article",
            "display_name": "Article",
            "fields": [
                {
                    "name": "hero",
                    "kind": "component",
                    "kind_meta": {"component": "shared.hero", "multiple": false}
                },
                {
                    "name": "sections",
                    "kind": "component",
                    "kind_meta": {"component": "shared.hero", "multiple": true}
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
}

// ----- tests -----

#[tokio::test]
async fn create_and_read_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;

    let resp = app
        .admin(app.client.get(app.url("/admin/components/shared.hero")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["uid"], "shared.hero");
    assert_eq!(body["display_name"], "Hero Block");
    assert_eq!(body["fields"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn list_components() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;

    let resp = app
        .admin(app.client.get(app.url("/admin/components")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
}

#[tokio::test]
async fn update_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;

    let resp = app
        .admin(app.client.put(app.url("/admin/components/shared.hero")))
        .json(&json!({
            "display_name": "Hero Block v2",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "subtitle", "kind": "string"},
                {"name": "cta", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["display_name"], "Hero Block v2");
    assert_eq!(body["fields"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn delete_unreferenced_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;

    let resp = app
        .admin(app.client.delete(app.url("/admin/components/shared.hero?confirm=true")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
}

#[tokio::test]
async fn delete_referenced_component_rejected() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.delete(app.url("/admin/components/shared.hero?confirm=true")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn get_content_type_injects_component_fields() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.get(app.url("/admin/content-types/article")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let hero_field = body["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "hero")
        .unwrap();
    let comp_fields = hero_field["kind_meta"]["_component_fields"].as_array().unwrap();
    assert_eq!(comp_fields.len(), 2);
    assert_eq!(comp_fields[0]["name"], "title");
}

#[tokio::test]
async fn write_entry_with_valid_single_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "hero": {"title": "Welcome", "subtitle": "Sub"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["hero"]["title"], "Welcome");
}

#[tokio::test]
async fn write_entry_with_valid_repeatable_component() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "sections": [
                {"title": "Intro"},
                {"title": "Features", "subtitle": "All features"}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["sections"].as_array().unwrap().len(), 2);
    assert_eq!(body["sections"][0]["title"], "Intro");
}

#[tokio::test]
async fn write_entry_missing_required_inner_field_rejected() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "hero": {"subtitle": "no title here"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
    let body: serde_json::Value = resp.json().await.unwrap();
    let err_text = serde_json::to_string(&body).unwrap();
    assert!(err_text.contains("hero.title"), "expected hero.title in {err_text}");
}

#[tokio::test]
async fn write_entry_wrong_inner_field_type_rejected() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "hero": {"title": 42}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn write_entry_repeatable_wrong_type_rejected() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    // sections must be an array, not an object
    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({
            "sections": {"title": "not an array"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn uid_must_have_two_segments() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/components")))
        .json(&json!({
            "uid": "noperiod",
            "display_name": "Bad",
            "fields": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn component_cannot_have_relation_inner_field() {
    let app = TestApp::spawn().await;
    let resp = app
        .admin(app.client.post(app.url("/admin/components")))
        .json(&json!({
            "uid": "shared.bad",
            "display_name": "Bad",
            "fields": [
                {"name": "author", "kind": "relation", "kind_meta": {"target": "user", "cardinality": "many_to_one"}}
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn existing_entries_readable_after_component_update() {
    let app = TestApp::spawn().await;
    make_hero_component(&app).await;
    make_article_type(&app).await;

    // Create entry
    let resp = app
        .admin(app.client.post(app.url("/api/article")))
        .json(&json!({"hero": {"title": "Old title"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let entry: serde_json::Value = resp.json().await.unwrap();
    let id = entry["id"].as_str().unwrap();

    // Update component schema (add a field)
    app.admin(app.client.put(app.url("/admin/components/shared.hero")))
        .json(&json!({
            "display_name": "Hero Block",
            "fields": [
                {"name": "title", "kind": "string", "required": true},
                {"name": "subtitle", "kind": "string"},
                {"name": "new_field", "kind": "string"}
            ]
        }))
        .send()
        .await
        .unwrap();

    // Existing entry still readable
    let resp = app
        .admin(app.client.get(app.url(&format!("/api/article/{id}"))))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["hero"]["title"], "Old title");
}
```

- [ ] **Step 2: Run the tests**

```bash
cargo test -p ferrum --test components 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/bin/tests/components.rs
git commit -m "test(bin): integration tests for component registry and write validation"
```

---

## Task 11: Run full backend suite

- [ ] **Step 1: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 2: Clippy**

```bash
cargo clippy --workspace --all-targets 2>&1 | grep -E "^error" | head -20
```

Expected: no errors.

- [ ] **Step 3: Commit if any lint fixes were needed**

```bash
git add -A && git commit -m "chore: clippy fixes for component implementation"
```

---

## Task 12: UI — types and API client

**Files:**
- Modify: `ui/src/api/types.ts`
- Modify: `ui/src/api/endpoints.ts`

- [ ] **Step 1: Add `"component"` to `FieldKind` and `Component` types**

In `ui/src/api/types.ts`, extend the `FieldKind` union:

```ts
export type FieldKind =
  | "string"
  | "text"
  | "integer"
  | "float"
  | "boolean"
  | "datetime"
  | "uuid"
  | "relation"
  | "media"
  | "enum"
  | "json"
  | "email"
  | "url"
  | "slug"
  | "rich_text"
  | "component";
```

Extend `Field` to include the optional injected field:

```ts
export interface Field {
  name: string;
  kind: FieldKind;
  required: boolean;
  unique: boolean;
  default: unknown;
  max_length?: number;
  kind_meta: Record<string, unknown>;
  _component_fields?: Field[];
}
```

Add `ComponentMeta` and helper after `mediaMeta`:

```ts
// Component kind_meta shape (when kind === "component").
export interface ComponentMeta {
  component: string;
  multiple: boolean;
}

export function componentMeta(f: Field): ComponentMeta | null {
  if (f.kind !== "component") return null;
  const m = f.kind_meta as Partial<ComponentMeta>;
  return typeof m.component === "string"
    ? { component: m.component, multiple: m.multiple === true }
    : null;
}
```

Add `Component` wire type:

```ts
export interface Component {
  uid: string;
  display_name: string;
  fields: Field[];
}

export interface NewComponent {
  uid: string;
  display_name: string;
  fields: Field[];
}

export interface UpdateComponent {
  display_name: string;
  fields: Field[];
}
```

- [ ] **Step 2: Add component endpoints**

In `ui/src/api/endpoints.ts`, add:

```ts
export function listComponents(): Promise<Component[]> {
  return apiFetch<Component[]>("/admin/components");
}
export function getComponent(uid: string): Promise<Component> {
  return apiFetch<Component>(`/admin/components/${encodeURIComponent(uid)}`);
}
export function createComponent(body: NewComponent): Promise<Component> {
  return apiFetch<Component>("/admin/components", { method: "POST", body });
}
export function updateComponent(uid: string, body: UpdateComponent): Promise<Component> {
  return apiFetch<Component>(`/admin/components/${encodeURIComponent(uid)}`, { method: "PUT", body });
}
export function deleteComponent(uid: string): Promise<void> {
  return apiFetch<void>(`/admin/components/${encodeURIComponent(uid)}?confirm=true`, { method: "DELETE" });
}
```

Add the import for `Component`, `NewComponent`, `UpdateComponent` to the top of `endpoints.ts` (alongside the other type imports).

- [ ] **Step 3: Typecheck**

```bash
cd ui && pnpm typecheck 2>&1 | tail -10
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add ui/src/api/types.ts ui/src/api/endpoints.ts
git commit -m "feat(ui): add Component types and API endpoints"
```

---

## Task 13: UI — draftModel and FieldConfigModal updates

**Files:**
- Modify: `ui/src/builder/draftModel.ts`
- Modify: `ui/src/builder/FieldConfigModal.tsx`

- [ ] **Step 1: Add `"component"` to `KINDS` and `FIELD_CARDS` in `draftModel.ts`**

In `KINDS`:
```ts
export const KINDS: FieldKind[] = [
  "string", "text", "integer", "float", "boolean", "datetime",
  "relation", "media", "enum", "json", "email", "url", "slug", "rich_text", "component",
];
```

In `FIELD_CARDS`:
```ts
{ kind: "component", label: "Component", desc: "Reusable structured sub-object", icon: "layers" },
```

- [ ] **Step 2: Extend `DraftField` with component props**

```ts
export interface DraftField {
  id: string;
  name: string;
  kind: FieldKind;
  required: boolean;
  unique: boolean;
  enumValues: string[];
  target: string;
  inverse: string;
  cardinality: Cardinality;
  mediaMultiple: boolean;
  componentUid: string;       // kind === "component"
  componentMultiple: boolean; // kind === "component"
  defaultValue: string;
  isPrivate: boolean;
  origin: "existing" | "new";
}
```

- [ ] **Step 3: Update `blankField`**

```ts
export function blankField(kind: FieldKind = "string"): DraftField {
  return {
    id: crypto.randomUUID(),
    name: "",
    kind,
    required: false,
    unique: false,
    enumValues: [],
    target: "",
    inverse: "",
    cardinality: "many_to_one",
    mediaMultiple: false,
    componentUid: "",
    componentMultiple: false,
    defaultValue: "",
    isPrivate: false,
    origin: "new",
  };
}
```

- [ ] **Step 4: Update `seedFromContentType`**

In the `fields.map` inside `seedFromContentType`, add after `mediaMultiple`:

```ts
componentUid: (f.kind_meta as any)?.component ?? "",
componentMultiple: (f.kind_meta as any)?.multiple === true,
```

- [ ] **Step 5: Update `draftFieldToField`**

In `draftFieldToField`, add the component arm:

```ts
} else if (d.kind === "component") {
  kind_meta = { component: d.componentUid, multiple: d.componentMultiple };
}
```

- [ ] **Step 6: Update `FieldConfigModal.tsx` — add component config UI**

In `FieldConfigModal.tsx`, add a component-specific config section. After the existing media config section (look for `field.kind === "media"` block in the JSX), add:

```tsx
{field.kind === "component" && (
  <div className="rs-field">
    <div className="rs-field-label"><label>Component</label></div>
    <input
      className="rs-input"
      placeholder="e.g. shared.hero_block"
      value={field.componentUid ?? ""}
      onChange={(e) => set({ componentUid: e.target.value })}
      disabled={locked}
    />
    <div className="rs-field-label" style={{ marginTop: 12 }}><label>Repeatable</label></div>
    <button
      type="button"
      className={"rs-toggle" + (field.componentMultiple ? " is-on" : "")}
      onClick={() => !locked && set({ componentMultiple: !field.componentMultiple })}
    >
      <span className="rs-toggle-knob" />
    </button>
  </div>
)}
```

Also add `componentUid` and `componentMultiple` to the `save` validation — reject if kind is component and uid is empty:

```ts
if (field.kind === "component" && !field.componentUid.trim()) {
  setErr("A component uid is required (e.g. shared.hero_block).");
  return;
}
```

- [ ] **Step 7: Typecheck**

```bash
cd ui && pnpm typecheck 2>&1 | tail -10
```

Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add ui/src/builder/draftModel.ts ui/src/builder/FieldConfigModal.tsx
git commit -m "feat(ui): add component kind to field builder"
```

---

## Task 14: UI — ComponentBuilder screen

**Files:**
- Create: `ui/src/screens/ComponentBuilder.tsx`
- Modify: `ui/src/app.tsx`
- Modify: `ui/src/components/shell.tsx`

- [ ] **Step 1: Create `ComponentBuilder.tsx`**

Create `ui/src/screens/ComponentBuilder.tsx`:

```tsx
import { useState, useEffect } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Plus, Trash2 } from "lucide-react";
import { LoadingState, EmptyState, Notice } from "../components/ui";
import { useResource } from "../hooks/useResource";
import {
  listComponents, getComponent, createComponent, updateComponent, deleteComponent,
} from "../api/endpoints";
import type { Component, Field } from "../api/types";
import { FieldRow } from "../builder/FieldRow";
import { FieldConfigModal } from "../builder/FieldConfigModal";
import { blankField, draftFieldToField } from "../builder/draftModel";
import type { DraftField } from "../builder/draftModel";

export function ComponentBuilder() {
  const { uid } = useParams<{ uid: string }>();
  const navigate = useNavigate();

  const all = useResource(() => listComponents(), []);
  const selected = useResource(
    () => (uid ? getComponent(uid) : Promise.resolve(null)),
    [uid],
  );

  const [fields, setFields] = useState<DraftField[]>([]);
  const [displayName, setDisplayName] = useState("");
  const [newUid, setNewUid] = useState("");
  const [adding, setAdding] = useState(false);
  const [editingField, setEditingField] = useState<DraftField | null>(null);
  const [saving, setSaving] = useState(false);
  const [banner, setBanner] = useState<string | null>(null);
  const [isNew, setIsNew] = useState(false);

  useEffect(() => {
    if (selected.data) {
      setDisplayName(selected.data.display_name);
      setFields(
        selected.data.fields.map((f) => ({
          ...blankField(f.kind),
          name: f.name,
          required: f.required,
          unique: f.unique,
          origin: "existing" as const,
        }))
      );
      setIsNew(false);
    }
  }, [selected.data]);

  const startNew = () => {
    setIsNew(true);
    setNewUid("");
    setDisplayName("");
    setFields([]);
    navigate("/components/new");
  };

  const save = async () => {
    setSaving(true);
    setBanner(null);
    try {
      const wireFields: Field[] = fields.map((d) => ({
        name: d.name,
        kind: d.kind,
        required: d.required,
        unique: d.unique,
        default: null,
        kind_meta: {},
      }));
      if (isNew) {
        const c = await createComponent({ uid: newUid, display_name: displayName, fields: wireFields });
        navigate(`/components/${encodeURIComponent(c.uid)}`);
      } else if (uid) {
        await updateComponent(uid, { display_name: displayName, fields: wireFields });
      }
      all.reload?.();
    } catch (e: any) {
      setBanner(e?.message ?? "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!uid || !confirm(`Delete component "${uid}"?`)) return;
    try {
      await deleteComponent(uid);
      navigate("/components");
      all.reload?.();
    } catch (e: any) {
      setBanner(e?.message ?? "Delete failed");
    }
  };

  return (
    <div style={{ display: "flex", height: "100%", overflow: "hidden" }}>
      {/* Left panel — component list */}
      <div style={{ width: 240, borderRight: "1px solid var(--rs-border)", overflowY: "auto", padding: "16px 0" }}>
        <div style={{ padding: "0 16px 12px" }}>
          <button className="rs-btn rs-btn--primary rs-btn--sm" style={{ width: "100%" }} onClick={startNew}>
            <Plus size={14} /> New Component
          </button>
        </div>
        {all.loading && <LoadingState />}
        {all.data?.map((c) => (
          <button
            key={c.uid}
            className={"rs-nav-item" + (c.uid === uid ? " is-active" : "")}
            onClick={() => navigate(`/components/${encodeURIComponent(c.uid)}`)}
            style={{ width: "100%", textAlign: "left", padding: "8px 16px", background: "none", border: "none", cursor: "pointer" }}
          >
            <div style={{ fontWeight: 500, fontSize: 13 }}>{c.display_name}</div>
            <div style={{ fontSize: 11, color: "var(--rs-fg-muted)" }}>{c.uid}</div>
          </button>
        ))}
      </div>

      {/* Right panel — editor */}
      <div style={{ flex: 1, overflowY: "auto", padding: 24 }}>
        {!uid && !isNew && (
          <EmptyState>Select a component or create a new one.</EmptyState>
        )}
        {(uid || isNew) && (
          <>
            {banner && <Notice>{banner}</Notice>}
            {isNew && (
              <div className="rs-field" style={{ marginBottom: 16 }}>
                <div className="rs-field-label"><label>UID</label><span className="rs-field-hint">e.g. shared.hero_block</span></div>
                <input className="rs-input" value={newUid} onChange={(e) => setNewUid(e.target.value)} placeholder="category.name" />
              </div>
            )}
            <div className="rs-field" style={{ marginBottom: 16 }}>
              <div className="rs-field-label"><label>Display Name</label></div>
              <input className="rs-input" value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
            </div>

            <div style={{ marginBottom: 12, fontWeight: 600 }}>Fields</div>
            <div className="rs-fields">
              {fields.map((f) => (
                <FieldRow
                  key={f.id}
                  field={f}
                  onEdit={() => setEditingField(f)}
                  onRemove={() => setFields((prev) => prev.filter((x) => x.id !== f.id))}
                />
              ))}
            </div>
            <button
              className="rs-btn rs-btn--ghost rs-btn--sm"
              style={{ marginTop: 8 }}
              onClick={() => { setAdding(true); setEditingField(blankField()); }}
            >
              <Plus size={13} /> Add field
            </button>

            <div style={{ marginTop: 24, display: "flex", gap: 8 }}>
              <button className="rs-btn rs-btn--primary" onClick={save} disabled={saving}>
                {saving ? "Saving…" : isNew ? "Create" : "Save"}
              </button>
              {!isNew && uid && (
                <button className="rs-btn rs-btn--ghost" onClick={handleDelete}>
                  <Trash2 size={14} /> Delete
                </button>
              )}
            </div>
          </>
        )}
      </div>

      {editingField && (
        <FieldConfigModal
          initial={editingField}
          isNew={adding}
          typeNames={[]}
          lockedEnumValues={[]}
          onSave={(f) => {
            if (adding) {
              setFields((prev) => [...prev, { ...f, origin: "new" }]);
            } else {
              setFields((prev) => prev.map((x) => (x.id === f.id ? f : x)));
            }
            setEditingField(null);
            setAdding(false);
          }}
          onBack={() => { setEditingField(null); setAdding(false); }}
          onClose={() => { setEditingField(null); setAdding(false); }}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 2: Add route in `ui/src/app.tsx`**

Add import:
```tsx
import { ComponentBuilder } from "./screens/ComponentBuilder";
```

Add routes inside the protected `<Route>`:
```tsx
<Route path="components" element={<ComponentBuilder />} />
<Route path="components/new" element={<ComponentBuilder />} />
<Route path="components/:uid" element={<ComponentBuilder />} />
```

- [ ] **Step 3: Add nav item in `ui/src/components/shell.tsx`**

In the `Sidebar` nav items array, after the `builder` entry:

```ts
{ to: "/components", label: "Components", icon: "layers" },
```

- [ ] **Step 4: Typecheck**

```bash
cd ui && pnpm typecheck 2>&1 | tail -15
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/screens/ComponentBuilder.tsx ui/src/app.tsx ui/src/components/shell.tsx
git commit -m "feat(ui): add Component Builder screen"
```

---

## Task 15: UI — ComponentField input in EntryEditor

**Files:**
- Modify: `ui/src/screens/EntryEditor.tsx`

- [ ] **Step 1: Add `ComponentField` component**

At the bottom of `ui/src/screens/EntryEditor.tsx`, add:

```tsx
function ComponentField({
  field,
  value,
  onChange,
}: {
  field: Field;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const meta = componentMeta(field);
  const innerFields = field._component_fields ?? [];

  if (!meta) return null;

  if (meta.multiple) {
    const arr = Array.isArray(value) ? (value as Record<string, unknown>[]) : [];
    const setItem = (i: number, patch: Record<string, unknown>) => {
      const next = arr.slice();
      next[i] = { ...next[i], ...patch };
      onChange(next);
    };
    const addItem = () => onChange([...arr, {}]);
    const removeItem = (i: number) => onChange(arr.filter((_, idx) => idx !== i));
    return (
      <div className="rs-component-list">
        {arr.map((item, i) => (
          <div key={i} className="rs-component-card">
            <div className="rs-component-card-head">
              <span style={{ fontWeight: 500, fontSize: 12, color: "var(--rs-fg-muted)" }}>#{i + 1}</span>
              <button type="button" className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => removeItem(i)}>
                <Trash2 size={13} />
              </button>
            </div>
            {innerFields.map((f) => (
              <div key={f.name} className="rs-field">
                <div className="rs-field-label"><label>{f.name}</label><span className="rs-field-hint">{f.kind}</span></div>
                <FieldInput
                  field={f}
                  value={item[f.name]}
                  onChange={(v) => setItem(i, { [f.name]: v })}
                  type=""
                />
              </div>
            ))}
          </div>
        ))}
        <button type="button" className="rs-btn rs-btn--ghost rs-btn--sm" onClick={addItem}>
          <Plus size={13} /> Add item
        </button>
      </div>
    );
  }

  // Single
  const obj = (value && typeof value === "object" && !Array.isArray(value))
    ? (value as Record<string, unknown>)
    : {};
  const setField = (name: string, v: unknown) => onChange({ ...obj, [name]: v });

  return (
    <div className="rs-component-card">
      {innerFields.map((f) => (
        <div key={f.name} className="rs-field">
          <div className="rs-field-label"><label>{f.name}</label><span className="rs-field-hint">{f.kind}</span></div>
          <FieldInput
            field={f}
            value={obj[f.name]}
            onChange={(v) => setField(f.name, v)}
            type=""
          />
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Import `componentMeta` and `Trash2` at the top**

`Trash2` is already imported. Add `componentMeta` to the import from `../api/types`:

```tsx
import { draftPublishEnabled, enumValues, mediaMeta, relationMeta, componentMeta } from "../api/types";
```

Also ensure `Plus` is imported from `lucide-react` (it already is).

- [ ] **Step 3: Wire `ComponentField` in `FieldInput`**

In `FieldInput`, add a case for `"component"` before the `default`:

```tsx
case "component":
  return <ComponentField field={field} value={value} onChange={onChange} />;
```

- [ ] **Step 4: Typecheck**

```bash
cd ui && pnpm typecheck 2>&1 | tail -15
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/screens/EntryEditor.tsx
git commit -m "feat(ui): add ComponentField input to EntryEditor"
```

---

## Task 16: Final verification

- [ ] **Step 1: Full backend suite**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 2: UI typecheck**

```bash
cd ui && pnpm typecheck 2>&1 | tail -10
```

Expected: no errors.

- [ ] **Step 3: Clippy**

```bash
cargo clippy --workspace --all-targets 2>&1 | grep "^error" | head -20
```

Expected: no errors.

- [ ] **Step 4: Manual smoke test (if dev server available)**

```bash
cd ui && pnpm dev
```

Verify:
- `/components` screen loads, can create a component with a `string` field
- Content-Type Builder can add a `component` field referencing the new component
- Entry Editor renders the component field as an inline form
- Repeatable component field renders a list with Add/Remove

- [ ] **Step 5: Merge commit**

```bash
git checkout main
git merge - --no-ff -m "feat: component field types with registry, validation, and UI builder"
```
