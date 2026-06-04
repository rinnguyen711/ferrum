# Draft & Publish — Design

Date: 2026-06-04
Status: approved, pre-implementation

## Goal

Add a Draft & Publish (D&P) capability to content types. When enabled on a type,
each entry has a system-managed publish state. Editors work on a draft and
explicitly publish it; consumers can request only published content. Modeled on
Strapi's "lite" approach: one row per entry, a nullable `published_at` timestamp
distinguishes draft (NULL) from published (set).

Enabled by default when creating a collection type. User cannot directly modify
the publish field; the system manages it via dedicated publish/unpublish actions.

## Storage model (Model A — single row)

One row per entry (unchanged). Draft vs published is a property of that row, not
a separate copy.

- `published_at TIMESTAMPTZ NULL` on `ct_<name>`, present **only** when D&P is on.
  - `NULL` → draft
  - set → published (timestamp records when)
- It is a **system column** like `created_at` / `updated_at` — NOT a member of
  `ct.fields`. It is never user-defined, never patched through the field list.
- `"published_at"` added to `RESERVED_FIELD_NAMES` so a user field cannot collide.

Explicitly NOT model B: there is no separate editable draft copy that leaves the
published copy frozen. The editor's Draft/Published tabs reflect the draft-state
vs live-state of the *same* row. True version separation is a future project.

## Content type flag

`ContentType.options` — new jsonb field, shape `{ "draft_publish": bool }`.

- Stored in `_content_types` via new column `options jsonb NOT NULL DEFAULT '{}'`.
- `NewContentType` accepts `options`; when omitted on create, `draft_publish`
  defaults to `true`.
- Helper `ContentType::draft_publish() -> bool` reads `options.draft_publish`
  (default false if absent, so existing types pre-migration read as off).
- `options` is the extension point for future per-type settings.

## Toggle rules

- **Create:** `draft_publish` true (default) emits the `published_at` column.
- **Patch enable (false→true):** `ALTER TABLE ct_<name> ADD COLUMN published_at
  TIMESTAMPTZ` (nullable). Existing rows get NULL → all become drafts. Allowed.
- **Patch disable (true→false):** rejected in v1 with `Error::Validation`
  ("disabling Draft & Publish is not supported"). Would orphan publish state /
  drop a column with data.

## Backend changes by layer

### core (`crates/core`)
- `content_type.rs`: add `options: serde_json::Value` (or typed `TypeOptions`)
  to `ContentType` and `NewContentType`; `draft_publish()` helper; default-true
  logic on create. Validate `options` shape.
- `reserved.rs`: add `"published_at"` to `RESERVED_FIELD_NAMES`.

### sql (`crates/sql`)
- `ddl.rs`: `create_table` appends `"published_at" TIMESTAMPTZ` when
  `ct.draft_publish()`. New `add_published_at_column(ct_name) -> String` for the
  patch-enable path.
- `dml.rs`:
  - `publish(ct_name, id)` → `UPDATE ct_<name> SET published_at = now(),
    updated_at = now() WHERE id = $1 RETURNING *`.
  - `unpublish(ct_name, id)` → `SET published_at = NULL, updated_at = now()`.
  - `select_list` / `select_by_id` gain a published filter mode:
    `published` → `WHERE published_at IS NOT NULL`; `draft` → `IS NULL`;
    `all` → no clause. Only applied for D&P types.

### schema service (`crates/schema`)
- `create`: persist `options`; emit published_at column inside the txn.
- `patch`: detect options change. enable → add column. disable → reject.
  Persist updated `options` to `_content_types`.
- Migration: add `options` column to `_content_types` (DEFAULT '{}').

### http (`crates/http`)
- `routes/content.rs`:
  - Routes: add `POST /api/:type/:id/publish` and
    `POST /api/:type/:id/unpublish`. 404 / 422 if type is not D&P. Return the
    updated entry. Authz consistent with update.
  - `list` / `get_one`: parse `?status=published|draft|all`. Default
    `published` for D&P types. Non-D&P types ignore the param (return all).
  - `create` / `update`: reject or strip `published_at` in the request body
    (system-managed, read-only to clients).
  - Serialize `published_at` into the entry JSON response for D&P types.

## Frontend changes

### api (`ui/src/api`)
- `types.ts`: `options` on `ContentType`; derive `draftPublish` helper.
- `endpoints.ts`: `publishEntry(type, id)`, `unpublishEntry(type, id)`;
  `status` param on `listEntries`.

### Builder (`SchemaEditor`)
- "Enable Draft & Publish" toggle in type settings (default ON for new types).
  Sends `options.draft_publish`.
- Existing type: toggling ON allowed; OFF disabled with explanatory tooltip
  (not supported in v1).

### Content list (`ContentList.tsx`)
- D&P types force-show a **Status** column derived from `published_at`
  (Published badge if set, Draft if NULL). This column is locked/always-visible
  in the Fields menu (reuses the locked-column mechanism from the Fields feature).
- The existing status tabs become Draft / Published / All for D&P types, driving
  `?status=`. A user-defined `status` enum field, if present, coexists as a
  normal column — system publish state is separate.

### Entry editor (`EntryEditor.tsx`)
- Draft / Published tabs reflecting the row's draft-state vs live-state.
- Publish / Unpublish button calling the new endpoints.
- `published_at` shown read-only.

## Testing

- **core:** options serde round-trip; default-true on create; reserved name.
- **sql/ddl:** published_at emitted only when D&P on; add-column builder.
- **sql/dml:** publish/unpublish SQL; status filter clauses.
- **schema service:** create-with-D&P adds column; patch-enable adds column;
  patch-disable rejected; options persisted.
- **http integration:** publish → entry becomes published; unpublish → draft;
  `?status` filtering (published default, draft, all); body `published_at`
  rejected; non-D&P type ignores status + has no publish endpoint.
- **ui:** tab/badge wiring; locked status column; publish button calls endpoint.

## Out of scope (v1)

- Disabling D&P on a type.
- Model B (separate editable draft copy with a frozen published copy).
- Scheduled / timed publishing.
- Per-locale or per-revision publish state.
- Publish audit history beyond the single `published_at` timestamp.
