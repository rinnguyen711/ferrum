# Component support in schema files

**Status:** design approved, ready for implementation plan
**Date:** 2026-06-15
**Builds on:** `2026-06-14-schema-as-code-toml-sync-design.md` (the TOML content-type sync feature, already implemented on branch `feat/schema-as-code-toml`).

## Goal

Let developers define **components** (reusable field groups) in the same TOML
schema files that already define content types, and have startup sync reconcile
them — so a `component`-kind field in a content type can reference a component
that the schema file itself provides, with no manual UI step.

## Decisions (locked)

| Topic | Decision |
|---|---|
| TOML syntax | New top-level `[[component]]` array, with `[[component.field]]` inner fields. Lives in the same files as `[[content_type]]`. |
| Apply order | Components are synced **before** content types (a `component` field references a component that must already exist). |
| Managed flag | New **`managed` boolean column** on the `_components` table (migration). Mirrors `options.managed` on content types. |
| Update semantics | Wholesale field replace when the desired field set / display_name differs; idempotent skip when equal. **No** kind/meta-change restriction (components store fields as jsonb — no DDL, no data-loss risk). |
| Nested components | **Not allowed.** Inherit the existing `ALLOWED_INNER_KINDS` rule (`component` and `relation` are already forbidden inside a component). Sync errors via `validate_inner_fields` if violated. |
| `full` drop of a referenced component | **Fail-fast abort.** `ComponentService::delete` already refuses (`Error::Conflict`) when `referencing_types` is non-empty; sync surfaces that and aborts boot. |
| Managed lock | Managed components are read-only in the component editor UI and the HTTP API (PUT/DELETE → 409), mirroring the content-type managed lock. |

## Existing code this builds on

- `ferrum_sql::Component { uid, display_name, fields }` and `ComponentStore` (table `_components`) with `create(uid, display_name, &fields)`, `update(...)`, `delete(uid)`, `list()`, `get(uid)`. There is also a private `RawComponent` sqlx `FromRow`.
- `ferrum_schema::ComponentService::{create, update, delete, get, list}` + `ComponentRegistry`. `create` calls `validate_uid` + `validate_inner_fields`; `delete(uid, referencing_types)` rejects when referenced.
- `validate_inner_fields` enforces `ALLOWED_INNER_KINDS` (scalars + media; no relation/component).
- `crates/http/src/routes/components.rs`: routes use `put(update_one)` and `delete(delete_one)` (components use **PUT**, not PATCH).
- The content-type sync engine in `crates/schema/src/sync.rs`: `SchemaFile`, `parse_toml`, `plan_sync`, `load_desired`, `order_creates`, `order_drops`, `sync_from_path`, `managed_options`.
- Internal migrations live in `crates/schema/migrations/` (last is `0012_audit_log.sql`; next is `0013`). Per gotcha: the schema crate must be rebuilt after adding a migration (`sqlx::migrate!` embeds at compile time).

## Section 1 — TOML format + parse

```toml
[[component]]
uid = "shared.seo"
display_name = "SEO"

  [[component.field]]
  name = "meta_title"
  kind = "string"

  [[component.field]]
  name = "og_image"
  kind = "media"

[[content_type]]
name = "post"
display_name = "Post"

  [[content_type.field]]
  name = "seo"
  kind = "component"
  kind_meta = { component = "shared.seo", multiple = false }
```

- Add `components: Vec<TomlComponent>` to `SchemaFile` with `#[serde(default, rename = "component")]`.
- `TomlComponent { uid: String, display_name: String, fields: Vec<Field> }` with `#[serde(default, rename = "field")]` on `fields`.
- `parse_toml` returns both content types and components. To keep its signature simple, change it to return a small struct (e.g. `ParsedSchema { content_types: Vec<NewContentType>, components: Vec<TomlComponent> }`) OR add a sibling `parse_components`. The plan picks the concrete shape; prefer one parse pass returning both.
- `load_desired` merges components across files too; **duplicate uid across files is rejected** (same as duplicate type name), with an `Error::Validation` containing "duplicate component".

## Section 2 — Managed column migration

- New migration `crates/schema/migrations/0013_component_managed.sql`:
  ```sql
  ALTER TABLE _components ADD COLUMN managed boolean NOT NULL DEFAULT false;
  ```
- Add `pub managed: bool` to `ferrum_sql::Component` and to the private `RawComponent` FromRow; update the two `SELECT` statements to include `managed`, and `into_component()` to map it.
- Change `ComponentStore::create` and `update` to take a `managed: bool` parameter and bind it (INSERT adds the column; UPDATE sets it).
- Update `ComponentService::create` / `update` signatures to thread `managed` through. Existing callers:
  - HTTP routes (`components.rs` create/update) pass `managed = false`.
  - Sync passes `managed = true`.
- Add `Component::managed` to the JSON the API returns (it already serializes the whole struct, so the new field flows to the UI for free).

## Section 3 — Component sync engine

In `crates/schema/src/sync.rs`:

- `ComponentAction` enum: `Create(TomlComponent)`, `Update(TomlComponent)`, `Delete(String)`, `Unmanage(String)`.
- `plan_components(desired: &[TomlComponent], current: &[Component], mode: SyncMode) -> Result<Vec<ComponentAction>, Error>`:
  - uid in TOML, not DB → `Create`
  - uid in both → compare desired `(display_name, fields)` to current; differ → `Update`; equal → no action (idempotent)
  - uid in DB, not TOML → `Full`: `Delete`; `Additive`: `Unmanage` only when `current.managed` is true
- `sync_from_path` order becomes:
  1. load desired (types + components), validate each component's inner fields via the service path (or call `validate_inner_fields` equivalent — the service `create`/`update` already validate, so applying through the service is enough)
  2. **apply components first**: creates, then updates, then (Full) deletes / (Additive) unmanages — all through `ComponentService` with `managed = true` for create/update; Unmanage re-saves with `managed = false`
  3. **then apply content types** (existing logic unchanged)
- `sync_from_path` needs a `&ComponentService` in addition to `&SchemaService`. Update its signature and the single call site in `crates/bin/src/main.rs` (the `components` service is already constructed there).
- Delete of a referenced component: `ComponentService::delete` requires `referencing_types`. Sync computes them from the **post-sync** content-type set (the registry after types are known) — but since components apply *before* types, compute referencing types from the desired TOML content types + current registry. Simplest correct rule: gather all `component` field targets across the desired content types and the current registry; if a to-be-deleted uid appears, abort with the conflict. (Implementation detail for the plan; the service's own check is the backstop.)

## Section 4 — Managed lock (component editor)

- `crates/http/src/routes/components.rs`: in `update_one` and `delete_one`, look up the existing component; if `managed`, return `Error::Conflict` ("component `{uid}` is managed by a schema file; edit the TOML instead") → 409. Mirror the content-type guard in `schema.rs`.
- UI `ui/src/screens/ComponentEditor.tsx`: read `managed` off the loaded component; when true, show a "Managed by a schema file" `Notice` and disable save / delete / field add-edit-remove controls. Mirror `SchemaEditor.tsx`'s managed handling exactly (same tokens, same `Notice`, same disabled wiring). Add a `managedComponent(c)` helper to `ui/src/api/types.ts` if the component type differs from the content-type shape, or reuse `managedType` if the shape matches.

## Section 5 — Testing

**Unit** (`sync.rs`, no DB):
- `plan_components`: create, update-when-changed, skip-when-equal, delete (full), unmanage (additive, only when managed), unmanaged DB-only component left alone (additive)
- duplicate uid across files → error
- nested-component inner field rejected (via the service validation path; can be a service-level unit test)

**Integration** (testcontainers):
- TOML with a component + a content type referencing it → both created; component row has `managed = true`; component applied before the type (type create succeeds)
- re-run sync → no-op (no redundant component update; assert fields unchanged)
- `full` drop of a referenced component → sync returns error (boot would abort)
- managed component → PUT/DELETE via HTTP returns 409

**Fixture:** extend `examples/schema/blog/` with a component (e.g. `shared.seo`) and reference it from `post.toml`.

## Out of scope

- Nested components (component inside component)
- Relation fields inside components (already forbidden by core)
- Renaming a component uid (uid is the identity; a changed uid = create new + drop old, same as type names)
- Migrating existing UI-created components to managed (they stay `managed = false` unless re-declared in TOML)
