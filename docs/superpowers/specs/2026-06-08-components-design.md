# Structured / Component Field Types — Design

Date: 2026-06-08
Status: approved

## Problem

Rustapi has no way to define structured sub-objects as first-class field types. Authors must use `json` (raw textarea, no validation, no editor UI) when they need nested structured data. There is no reuse story — the same shape must be redefined on every content type that needs it.

## Goal

Add a **component registry** and a `component` field kind. A component is a named, reusable shape (collection of scalar fields) that can be referenced by any content type field. Component data is stored as `jsonb`, validated at the HTTP layer against the registered schema. A field can be single-instance or repeatable (`multiple: true`).

Dynamic zones (polymorphic component arrays) are explicitly deferred to a follow-up.

---

## Component Registry

### Storage

New `components` table:

```sql
CREATE TABLE components (
    uid          TEXT PRIMARY KEY,   -- e.g. "shared.hero_block"
    display_name TEXT NOT NULL,
    fields       JSONB NOT NULL DEFAULT '[]'
);
```

`uid` follows the `category.name` convention (dot-separated, lowercase, no spaces). Validated at the service layer. The `fields` column stores the same `Field[]` shape used by `content_types.fields`.

### API Surface

All routes under `/admin/components`, JWT-required:

| Method | Path | Description |
|---|---|---|
| `GET` | `/admin/components` | List all components |
| `POST` | `/admin/components` | Create a component |
| `GET` | `/admin/components/:uid` | Get one component |
| `PUT` | `/admin/components/:uid` | Update fields |
| `DELETE` | `/admin/components/:uid` | Delete (rejected if referenced) |

### Rust Home

- `crates/sql/src/component.rs` — `ComponentStore`: CRUD against the `components` table
- `crates/schema/src/component.rs` — `ComponentService`: wraps `ComponentStore`, validates inner fields on create/update, enforces referential integrity on delete
- `crates/http/src/routes/components.rs` — axum router for the five endpoints above

---

## Field Kind

### New variant: `FieldKind::Component`

Added to `crates/core/src/field.rs`. Serde wire name: `"component"`.

Field metadata stored in `content_types.fields` JSONB:

```json
{
  "name": "hero",
  "kind": "component",
  "component": "shared.hero_block",
  "multiple": false,
  "required": false
}
```

A new `ComponentMeta` struct holds the extra properties:

```rust
pub struct ComponentMeta {
    pub component: String,   // uid
    pub multiple: bool,
}
```

Helper `component_meta(field: &Field) -> Option<ComponentMeta>` in `core`, mirroring `relation_meta` / `media_meta`.

### Allowed inner field kinds

Component definitions may use: `string`, `text`, `integer`, `float`, `boolean`, `datetime`, `email`, `url`, `slug`, `enum`, `json`, `rich_text`, `media`.

Disallowed: `relation`, `component` (no nesting in v1).

`ComponentService::validate_fields` enforces this on create/update.

### Storage

`jsonb` column in the entry table — same DDL path as `Json`/`RichText`. `ddl.rs` emits `jsonb` for `FieldKind::Component`.

- Single (`multiple: false`) → JSON object
- Repeatable (`multiple: true`) → JSON array of objects

No new `BoundValue` variant. Component data passes through as `BoundValue::Json` after structural validation.

---

## Validation at Write Time

In `crates/http/src/routes/content.rs`, before `body_to_binds`:

1. For each field of kind `Component` in the content type, extract the value from the request body.
2. Fetch the component schema from `ComponentService` (cached on `AppState`).
3. For `multiple: false` — validate the object against the component's fields using `BoundValue::from_json` per inner field.
4. For `multiple: true` — iterate the array, validate each item.
5. On failure, return `400` with a dotted field path: `"hero.title"`, `"sections[1].body"`.

`required` on a component field: the outer value must be present (not null/missing). `required` on inner fields: validated per-instance.

---

## `getContentType` Response

The `/admin/content-types/:name` response includes resolved component shapes so the UI needs no extra fetch:

```json
{
  "name": "article",
  "fields": [
    {
      "name": "hero",
      "kind": "component",
      "component": "shared.hero_block",
      "multiple": false,
      "_component_fields": [
        { "name": "title", "kind": "string", "required": true },
        { "name": "image", "kind": "media", "required": false }
      ]
    }
  ]
}
```

`_component_fields` is injected by `SchemaService` when serializing the content type. It is read-only and not stored.

---

## Crate Changes Summary

| Crate | Change |
|---|---|
| `core` | Add `FieldKind::Component`, `ComponentMeta`, `component_meta()` helper |
| `sql` | New `component.rs` (`ComponentStore`); `ddl.rs` emits `jsonb` for `Component` |
| `schema` | New `component.rs` (`ComponentService`); `SchemaService` injects `_component_fields` on read |
| `http` | New `routes/components.rs`; `AppState` gains `Arc<ComponentService>`; content write validates component fields |
| `bin` | Wire `ComponentService` into `AppState`; new migration |

---

## Admin UI

### New screen: Component Builder (`/builder/components`)

Mirrors Content-Type Builder layout:
- Left panel: list of components (uid + display name), "New Component" button
- Right panel: field list for selected component, reuses `FieldRow` + `FieldConfigModal`
- Field kind picker in component context excludes `relation` and `component`

### Entry Editor

**Single component field** (`multiple: false`): inline card with one `FieldInput` per component field, using existing `FieldInput` dispatch.

**Repeatable component field** (`multiple: true`): ordered list of cards. Each card shows sub-field inputs. Add / Remove / Reorder (drag) controls — same pattern as the multi-media field.

**Form state:**

```ts
// single
form["hero"] = { title: "Hello", image: "asset-uuid" }

// repeatable
form["sections"] = [
  { title: "Intro", body: "..." },
  { title: "Features", body: "..." },
]
```

Component schema sourced from `_component_fields` in the `getContentType` response — no extra API call.

---

## DB Migration

Single new migration file `crates/sql/migrations/NNNN_components.sql`:

```sql
CREATE TABLE components (
    uid          TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    fields       JSONB NOT NULL DEFAULT '[]'
);
```

No changes to `content_types` table — the component uid is already in the field's JSONB metadata.

---

## Testing

**Backend integration tests** (`crates/bin/tests/components.rs`):

- Create component, read back — fields round-trip correctly
- Create content type referencing component, write entry with valid data — persists and reads back
- Write entry with invalid inner field (wrong type, missing required) — `400` with dotted path (`hero.title`)
- Repeatable: write array, read back in order
- Delete component referenced by content type — `400`
- Update component fields — existing entries still readable (schema-on-read)

**UI** — manual verification via `pnpm dev`:
- Component Builder: create, edit, delete
- Entry Editor: single component renders sub-inputs
- Entry Editor: repeatable renders card list with add/remove/reorder

---

## Out of Scope

- Dynamic zones (polymorphic `kind: dynamic_zone`) — deferred, noted in extensibility roadmap
- Nested components (component-inside-component)
- Filtering / sorting by component inner fields
- Component versioning or migration tooling
