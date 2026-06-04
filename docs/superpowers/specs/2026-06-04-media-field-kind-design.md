# Media Field Kind — Design

**Date:** 2026-06-04
**Scope:** Add a `Media` content-type field kind so content types can reference
assets in the standalone Media Library. End-to-end: core `FieldKind::Media` +
validation, storage/DDL, HTTP write/read with always-embedded asset metadata,
schema-builder UI, and an entry-editor asset picker.
**Status:** Approved (brainstorm)
**Builds on:** `docs/superpowers/specs/2026-06-03-media-library-backend-design.md`
(which deferred this field kind) and the shipped Media Library backend + UI
(`crates/media`, `crates/http/src/routes/media.rs`, `_media_assets` table,
`ui/src/screens/MediaLibrary.tsx`).

## Goal

Let a content type declare a field that points at one or many Media Library
assets. On read, the entry returns the full asset metadata inline (file name,
mime type, dimensions, alt text) so the admin UI can render thumbnails without a
second request. On write, the client sends bare asset id(s). Deleting an asset
from the Media Library never blocks; entries that referenced it simply lose the
reference.

## Key Decisions

- **Single and multiple.** A field-level `multiple` flag in `kind_meta`. Single =
  one asset; multiple = an ordered gallery.
- **Separate media path, mirroring relation patterns.** Media targets the system
  table `_media_assets`, not a `ct_*` table, and reads embed-always (not
  `?populate`-gated). So media gets its own parallel branches alongside relation
  in each layer rather than being forced through the relation machinery. Relation
  code (`table_name() -> ct_<x>`, `j_<owner>_<field>` join without ordering,
  `?populate` gating) does not fit media; contorting it would muddy both paths.
- **Single = nullable FK column** `<field>_id uuid REFERENCES _media_assets(id)
  ON DELETE SET NULL`. Multiple = dedicated **ordered** join table
  `j_media_<ct>_<field>` with a `position` column, FK `ON DELETE CASCADE`.
- **Delete is non-destructive to the library.** Single `ON DELETE SET NULL`,
  multiple join row `ON DELETE CASCADE`. Removing an asset always succeeds; the
  entry's media field becomes `null` (single) or drops that item (multiple). This
  matches the m2m tag behavior, not relation's `ON DELETE RESTRICT`.
- **Media fields are never `required`.** A `required` single-media column would be
  `NOT NULL`, which `ON DELETE SET NULL` cannot satisfy — deleting a referenced
  asset would then fail and block the library, breaking the invariant above. So
  validation rejects `required` for **both** single and multiple media. The
  single FK column is therefore always nullable.
- **Read shape: always embed.** Media fields always return the asset object(s)
  inline — no `?populate` needed. Refs are small and the metadata is needed for
  display anyway. (Deliberate divergence from relation.)
- **Write shape: bare id(s).** Single = uuid string or `null`. Multiple = array of
  uuid strings, order preserved, empty array = clear all. Mirrors relation's write
  format; simplest server parse. Object-with-id form is **not** accepted (no gain).
- **No cap on multiple-media embed.** Relation inverse caps at 25 children per
  parent; galleries should render in full. Galleries are small and admin-scoped,
  so the embed pass returns all linked assets ordered by `position`.
- **No target selector in the builder.** Media always targets the library, so the
  only config is the `multiple` toggle. Simpler than the relation config block.

## Architecture & File Placement

Parallel to the relation machinery. Touched layers:

- `crates/core/src/field.rs` — `FieldKind::Media`, `MediaMeta`, validation,
  `media_meta()` accessor, `physical_column()` / `is_stored_column()` updates,
  new `FieldError` variants.
- `crates/sql/src/ident.rs` — `media_join_table_name(ct, field)`.
- `crates/sql/src/ddl.rs` — media single column def, `create_media_join_table` /
  `drop_media_join_table`.
- `crates/schema/src/service.rs` — wire single columns + multi join tables into
  create-type / add-field / drop-field (mirrors existing m2m handling).
- `crates/http/src/entry.rs` — write coercion (`MediaCheck`, `MediaLinkPlan`),
  `decode_field` for media single.
- `crates/http/src/media_embed.rs` — **new** always-on embed pass (parallel to
  `populate.rs`).
- `crates/http/src/routes/content.rs` — run media existence checks, apply link
  plans in the write txn, invoke the embed pass on read.
- UI: `ui/src/api/types.ts`, `ui/src/builder/draftModel.ts`,
  `ui/src/builder/FieldConfigModal.tsx`, `ui/src/screens/EntryEditor.tsx`,
  **new** `ui/src/screens/media/AssetPicker.tsx`.

No new migration: `_media_assets` already exists (migration `0003_media.sql`).
Per-type media join tables are created by the schema service as DDL at type
create / field add time, exactly like relation join tables.

## Core — `crates/core/src/field.rs`

New enum variant (serde `"media"`):

```rust
/// Phase 2.6: references one or many Media Library assets (_media_assets).
/// Configuration lives in `Field.kind_meta`; see `MediaMeta`.
Media,
```

`kind_meta` shape:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct MediaMeta {
    pub multiple: bool,
}

impl MediaMeta {
    pub fn from_value(v: &serde_json::Value) -> Result<Self, FieldError> {
        // Accept {} (multiple defaults false) or {"multiple": bool}.
        // Reject any other key. Mirrors EnumMeta::from_value strictness.
    }
}
```

`Field::media_meta()` returns `Option<MediaMeta>` when `kind == Media`.

`BoundValue::from_json(Media, _)` → `Err(CoerceError::TypeMismatch)` — media is
never coerced as a scalar bind; `entry.rs` handles it before reaching the binder
(same pattern as relation).

Validation (in `Field::validate`, new branch before the primitive fallback):

- Parse `MediaMeta::from_value(&kind_meta)`.
- `unique == true` → `FieldError::MediaFieldUniqueUnsupported`.
- `default` not null → `FieldError::MediaFieldDefaultUnsupported`.
- `required == true` (single **or** multiple) → `FieldError::MediaFieldRequiredUnsupported`.
  Required is rejected for all media so the single FK column stays nullable and
  `ON DELETE SET NULL` can never block an asset delete.

New `FieldError` variants: `MediaMetaShape`, `MediaFieldUniqueUnsupported`,
`MediaFieldDefaultUnsupported`, `MediaFieldRequiredUnsupported`.

Helper updates:

- `physical_column()`: media single → `<name>_id`; media multiple → `<name>`
  (no row column, but keep the name for messages).
- `is_stored_column()`: media single → `true`; media multiple → `false` (lives in
  the join table).

## Storage / DDL

### Single media — column on the entry table

```sql
"<field>_id" uuid REFERENCES "_media_assets"("id") ON DELETE SET NULL
```

Always nullable (media is never `required`; see Key Decisions). No `UNIQUE`. New
branch in `ddl.rs::column_def` for media single (parallel to
`relation_column_def`). The referenced table is the literal `"_media_assets"`,
**not** routed through `table_name()` (which would produce `ct_<x>`).

### Multiple media — ordered join table

```sql
CREATE TABLE "j_media_<ct>_<field>" (
  "<ct>_id"  uuid NOT NULL REFERENCES "ct_<ct>"("id")       ON DELETE CASCADE,
  "asset_id" uuid NOT NULL REFERENCES "_media_assets"("id") ON DELETE CASCADE,
  "position" int  NOT NULL,
  PRIMARY KEY ("<ct>_id", "asset_id")
);
CREATE INDEX ON "j_media_<ct>_<field>" ("<ct>_id", "position");
```

- New `ddl.rs::create_media_join_table(ct, field)` returning
  `(create_sql, index_sql)`, and `drop_media_join_table(ct, field)`.
- New `ident.rs::media_join_table_name(ct, field)` → `"j_media_<ct>_<field>"`.
  The `j_media_` prefix keeps it distinct from relation's `j_<owner>_<field>`, so
  a relation field and a media field with the same name never collide.
- `PRIMARY KEY (<ct>_id, asset_id)` enforces no duplicate asset in one field;
  `position` is a plain ordering int (gaps allowed; rewritten on each replace).

### Schema service wiring (`crates/schema/src/service.rs`)

Mirror the existing m2m flow:

- **Create type:** emit single-media columns inline in `CREATE TABLE`; after the
  table exists, create each multiple-media join table.
- **Add field:** single → `ALTER TABLE ... ADD COLUMN`; multiple → create join
  table.
- **Drop field:** single → `DROP COLUMN`; multiple → drop join table.

## HTTP — Write

In `entry.rs::body_to_binds`, add a media branch (before the generic coerce):

- **Single** (uuid string | null):
  - `null` → `BoundValue::Null(FieldKind::Uuid)` under `<field>` (DML maps to
    `<field>_id`). No check.
  - uuid string → `BoundValue::Uuid(id)` + push `MediaCheck { field, id }`.
  - other → validation error.
- **Multiple** (array of uuid strings):
  - Parse to ordered `Vec<Uuid>`, dedup (PK rejects dupes anyway), preserving
    first-seen order. Empty array = explicit clear.
  - Push `MediaLinkPlan { field, ids, present: true }`. `position` = index in
    `ids`.

New types in `entry.rs`:

```rust
pub struct MediaCheck { pub field: String, pub id: Uuid }      // target = _media_assets
pub struct MediaLinkPlan { pub field: String, pub ids: Vec<Uuid>, pub present: bool }
```

`BodyBinds` return tuple extends to include `Vec<MediaCheck>` and
`Vec<MediaLinkPlan>`.

Handler (`routes/content.rs`), inside the existing write transaction:

- **Existence checks:** one batched `SELECT id FROM "_media_assets" WHERE id =
  ANY($1)` over all `MediaCheck` ids; any missing id → field validation error
  under that field's JSON key. (Single query — all media checks share one target.)
- **Link plans:** for each `MediaLinkPlan`, `DELETE FROM j_media_<ct>_<field>
  WHERE <ct>_id = $1` then bulk-insert `(<ct>_id, asset_id, position)` rows in
  array order. Runs in the same txn as the row write, after the row exists.

## HTTP — Read (always embed)

New module `crates/http/src/media_embed.rs`, parallel to `populate.rs`. Runs
unconditionally on every entry read (single GET and list), after `row_to_json`,
for every media field of the content type. Not gated by `?populate`.

`row_to_json` / `decode_field` produce the bare scalar first:

- Single media: read `<field>_id` uuid → bare id string (or `null`). The embed
  pass overwrites it with the asset object.
- Multiple media: `is_stored_column()` is false, so it is omitted from the base
  row; the embed pass inserts the array.

Embed pass algorithm:

1. Walk the content type's media fields. For single fields, collect the bare ids
   present on the result rows. For multiple fields, one batched query per field
   against its join table: `SELECT <ct>_id, asset_id, position FROM
   j_media_<ct>_<field> WHERE <ct>_id = ANY($1) ORDER BY <ct>_id, position`.
2. Collect the union of all referenced asset ids and run **one** batched
   `SELECT * FROM "_media_assets" WHERE id = ANY($1)`. Build a
   `HashMap<Uuid, Value>` of `AssetView`-shaped JSON objects.
3. For each row:
   - Single field → replace the bare id with the asset object, or `null` if the
     id didn't resolve (asset deleted concurrently).
   - Multiple field → ordered array of asset objects (join `position` order),
     `[]` when none. Missing-id rows are skipped (defensive; FK cascade normally
     prevents dangling join rows).

Embedded asset object shape = the same fields `AssetView` exposes from the media
routes: `id, folder_id, file_name, alt_text, caption, mime_type, size_bytes,
width, height, original_filename, created_at, updated_at`. Raw bytes are fetched
separately by the UI via the existing `GET /admin/media/assets/:id/raw`.

No per-parent cap on multiple-media embed (galleries render in full).

## UI — Schema Builder

`api/types.ts`:

- Add `"media"` to the `FieldKind` union.
- Add `MediaMeta { multiple: boolean }` and a `mediaMeta(f): MediaMeta | null`
  accessor (mirrors `relationMeta`).

`builder/draftModel.ts`:

- Add `"media"` to `KINDS`.
- `DraftField` gains `mediaMultiple: boolean`.
- `blankField()` sets `mediaMultiple: false`.
- `seedFromContentType()` reads it from `mediaMeta(f)?.multiple ?? false`.
- `draftFieldToField()`: when `kind === "media"`, `kind_meta = { multiple:
  d.mediaMultiple }`.

`builder/FieldConfigModal.tsx`:

- When `field.kind === "media"`, render a config block with a single toggle
  "Allow multiple assets" → sets `mediaMultiple`.
- Media is never required: hide/disable the "Required field" toggle for media
  fields (force `required = false`), reusing the existing `m2mRequiredBlocked`
  hint pattern generalized to also cover any media field.
- Field-type icon: use the existing `Icons.image` for media.

## UI — Entry Editor Asset Picker

`screens/EntryEditor.tsx`:

- `FieldInput` switch gains `case "media"` → `<MediaField>`.
- `MediaField` (single): shows the current asset thumbnail + name (or a "No asset"
  placeholder) with "Choose" / "Remove" buttons. Wire value = `asset.id` | null.
- `MediaField` (multiple): an ordered thumbnail strip; each item removable with
  up/down reordering (drag is out of scope for v1). "Add assets" opens the picker.
  Wire value = ordered `[id, ...]`.
- Seeds from the embedded asset object(s) the API returns, so thumbnails render
  immediately on load. The form stores ids (single) / id array (multiple); the
  `save()` body-builder passes media values through as-is and skips the
  empty-string coercion used for primitives.

`screens/media/AssetPicker.tsx` (**new** modal, reuses MediaLibrary browse bits):

- Browse folders + assets using the existing `listFolders` / `listAssets`,
  breadcrumb navigation, and `AssetThumb` for thumbnails.
- Single mode: click an asset → select and close.
- Multiple mode: checkbox multi-select + "Add selected", preserving pick order.
- Returns the chosen `MediaAsset[]` to `MediaField`.

No new endpoints — the picker reuses `listFolders`, `listAssets`,
`fetchAssetBlob`.

## Testing

- **core (`field.rs`):** `MediaMeta::from_value` accepts `{}` and `{multiple}`,
  rejects extra keys / non-bool; validate rejects unique, default, and required
  (single and multiple); `physical_column` / `is_stored_column` matrix for media
  single vs multiple; `BoundValue::from_json(Media, _)` is a type mismatch.
- **sql (`ddl.rs`):** single column def emits the nullable `_media_assets` FK with
  `ON DELETE SET NULL`; `create_media_join_table` emits the ordered table + index
  with the right names and CASCADE; ident builder produces `j_media_<ct>_<field>`.
- **schema service:** create-type / add-field / drop-field create and drop the
  media join tables.
- **http write:** single uuid coerces + registers a `MediaCheck`; null writes a
  typed null; bad uuid / non-string rejected; multiple builds an ordered
  `MediaLinkPlan`, empty array clears, dupes de-dup; missing asset id → field
  validation error.
- **http read/embed:** single field embeds the asset object (or null when the id
  no longer resolves); multiple field embeds an ordered array; no `?populate`
  required; deleting an asset SET-NULLs the single field and drops the gallery
  item on the next read. Match the existing http test style.
- **UI:** lightweight — covered by manual verification of the builder toggle and
  the picker (single + multiple) against a running instance.

## Out of Scope (future)

- Drag-to-reorder in the multiple-media editor (up/down only in v1).
- Cropping / focal point / image variants.
- Public-URL embedding of asset bytes in entry responses (UI fetches via `/raw`).
- Filtering or sorting entries by media field.
- A per-field allowed-mime / max-count constraint.
