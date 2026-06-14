# Schema-as-Code: TOML sync on startup

**Status:** design approved, ready for implementation plan
**Date:** 2026-06-14

## Goal

Let developers define content types declaratively in TOML files (Prisma/Sanity
style) and have the server sync the database to match those files on startup.
Devs ship ready-made schemas (blog, ecommerce, ...) that work instantly — no
manual click-through in the admin UI.

## Decisions (locked)

| Topic | Decision |
|---|---|
| Format | **TOML**. Rust-native, solid `toml` crate, comments, matches project idiom. No custom DSL in v1. |
| Source of truth | TOML file(s) describe the **desired state**. Server reconciles DB toward it on boot. |
| Sync modes | `RUSTAPI_SCHEMA_SYNC`: `additive` (default) = create types + add fields, never drop. `full` = also drop types/fields absent from TOML. |
| Rename | **Not supported in v1** (documented). Declarative TOML can't distinguish rename from drop+add. A name change = new field; old field stays until dropped (full mode). |
| Failure mode | **Fail fast.** Any parse/validate/apply error aborts boot with non-zero exit. Matches Prisma/Atlas/Sanity/Django. Only triggers when a schema path is configured. |
| Data scope | **Schema only.** No row seeding via TOML. Row data stays with CSV import. |
| Default demo seed | **Removed entirely.** Delete `bin/src/seed.rs` demo types (author/article/category) + sample rows + `RUSTAPI_SEED`. Ship empty by default. |
| File location | `RUSTAPI_SCHEMA_DIR` (load + merge all `*.toml`) or `RUSTAPI_SCHEMA_FILE` (single file). Dir wins if both set. Unset = feature off. |
| Two-writer conflict | **TOML owns; UI locks managed types.** Types from TOML are marked `managed`; UI/API reject edits (409) and grey them out. UI freely manages types NOT in TOML. Matches Sanity/Prisma. |

## Existing code this builds on

- `rustapi_core::NewContentType` / `Field` / `PatchContentType` — already serde-derive, so TOML deserializes straight into them.
- `rustapi_core::NewContentType::validate()` — local shape validation, reused as-is.
- `rustapi_schema::SchemaService::{create, patch, delete}` — transactional DDL + registry update, **one transaction per type**.
- `rustapi_schema::service::validate_relation_cross_refs` — cross-type relation validation, reused.
- `SchemaRegistry::reload_from_db` — registry hydrated before sync runs.
- `ContentType::draft_publish()` — pattern for the new `managed()` helper reading `options` jsonb.
- Boot sequence in `crates/bin/src/main.rs`: hydrate registry → (was: seed) → build GraphQL → serve.

## Section 1 — TOML format

One file may hold many types; or one type per file in a directory (keeps files
short, presets composable as `blog/`, `ecom/`).

```toml
[[content_type]]
name = "post"
display_name = "Post"
kind = "collection"               # collection | single
options = { draft_publish = true }

  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true

  [[content_type.field]]
  name = "author"
  kind = "relation"
  kind_meta = { target = "author", cardinality = "many_to_one", inverse = "posts" }
```

A thin `SchemaFile` struct deserializes to `Vec<NewContentType>` (field shape,
`kind_meta`, enum values all already expressible via existing derives).

## Section 2 — Sync engine

New module `crates/schema/src/sync.rs`. Pure diff logic + thin apply loop over
`SchemaService`.

1. **Load.** File → parse one. Dir → read all `*.toml`, parse + merge. Duplicate
   `name` across files = error.
2. **Validate.** Each via `NewContentType::validate()` + `validate_relation_cross_refs`.
3. **Diff** desired (TOML) vs current (registry) → `Vec<SyncAction>`:
   - type in TOML, not DB → **create**
   - type in both → field diff:
     - field in TOML not DB → add (PATCH `add_fields`)
     - field in DB not TOML → **drop** (PATCH `drop_fields`) — *full mode only*
     - field in both, kind/meta differ → **error** (no alter in v1; change = drop+add in full mode)
   - type in DB not TOML → **drop type** — *full mode only*
4. **Order.** Topologically sort creates by relation target (author before post).
5. **Apply.** Each action through `SchemaService` (transactional per type),
   sequential. First error propagates → abort boot.

Diff is a unit-testable pure function `(desired, current, mode) -> Vec<SyncAction>`;
apply is the thin DB loop.

## Section 3 — Boot wiring + config

`crates/bin/src/config.rs`:
- add `schema_path: Option<String>` (`RUSTAPI_SCHEMA_DIR` else `RUSTAPI_SCHEMA_FILE`, dir wins)
- add `schema_sync_mode: SyncMode` (`RUSTAPI_SCHEMA_SYNC`, default `additive`)
- **remove** `seed: bool` / `RUSTAPI_SEED`

`crates/bin/src/main.rs`, after registry hydrate, replace the `seed::seed_if_empty`
call with:

```rust
if let Some(path) = &cfg.schema_path {
    rustapi_schema::sync::sync_from_path(&schemas, path, cfg.schema_sync_mode)
        .await
        .context("schema sync")?;   // ? = fail fast, abort boot
}
```

No path → skip, boot empty. GraphQL rebuild already runs after sync → picks up
synced types.

**Seed removal.** Delete `crates/bin/src/seed.rs`, drop `mod seed` + wiring. The
seed-local helpers `body_to_binds` reuse / `insert_entry` go with it. CSV import
has its own write path (verify untouched during implementation).

## Section 4 — Managed-type lock (TOML owns)

**Mark managed.** Reuse the existing `options` jsonb on `_content_types` — no
migration. Sync writes `options.managed = true` (alongside `draft_publish`) on
every type it creates/patches. Add `ContentType::managed()` helper (mirrors
`draft_publish()`), serialized to the UI for free.

**Enforce in HTTP** (`crates/http/src/routes/schema.rs`): `patch_one` and
`delete_one` check `existing.managed()` → reject `409 Conflict` ("type managed by
schema file; edit the TOML"). `create` of an unmanaged name stays open. Guard at
the HTTP boundary, not in `SchemaService` — sync itself calls `SchemaService` and
must stay unblocked.

**UI** (SchemaEditor / content-type list): `options` already reaches the client.
If `managed`, disable edit/delete and show a "Managed by schema file" badge using
DESIGN.md tokens (ghost/disabled state). Read-only view stays.

**Lifecycle edge — type removed from TOML:**
- `additive`: type not dropped, but sync **clears the `managed` flag** for any DB
  type absent from the current TOML set → UI regains control (no orphaned locked
  type).
- `full`: type dropped entirely; moot.

The `managed` flag is recomputed every sync to exactly the current TOML set.

## Section 5 — Testing

**Unit** (`sync.rs`, no DB):
- diff: create, add-field, drop-field gated by mode, drop-type gated by mode,
  incompatible field-kind change errors, unknown relation target, duplicate name
  across files
- TOML parse → `NewContentType` roundtrip

**Integration** (testcontainers, existing harness):
- empty DB + `blog.toml` → types exist, columns correct, marked managed
- re-run → no-op (idempotent)
- `additive` ignores a DB-only field; `full` drops it
- bad TOML → error returned (boot would abort)
- relation order resolves (target before dependent)
- managed type → PATCH/DELETE via HTTP returns 409
- type dropped from TOML in additive → `managed` cleared

**Fixtures.** Ship `examples/schema/blog/` (and optionally `ecom/`) as real
presets — double as docs and integration-test input.

## Out of scope (v1)

- Field rename / kind change preserving data
- Row/data seeding via TOML
- Hot reload / file-watch (sync is startup-only)
- Disabling Draft & Publish via sync (engine already rejects the disable transition)
- Custom DSL or YAML format
