# Localization (i18n) — Backend Design

**Date:** 2026-06-19
**Branch:** `feat/localization-backend`
**Scope:** Backend only. Admin UI deferred to a follow-up spec.

## Goal

Multi-language content. Each content type can opt into localization. A
localized entry exists as one row per locale, all rows sharing a stable
`document_id`. Reads accept a `?locale=` selector and fall back to the
default locale when a translation is missing. Per-locale publish state comes
free from the existing per-row `published_at`.

## Decisions (locked during brainstorming)

- **Model:** per-entry locale rows. One row = one `(document_id, locale)`.
- **Fallback:** missing translation → serve the default-locale row; response
  `meta.locale` reports the code actually served.
- **Locale config:** global locale set in a DB table; localization is
  **per-content-type opt-in** via a `localized` option.
- **v1 scope:** data model + locales registry + locale-aware read/write across
  REST and GraphQL + default-locale fallback. **Out:** admin UI, copy-from-
  default prefill, auto-translate.

## Non-goals (deferred)

- Admin UI (locale switcher, settings page) — follow-up spec.
- "Create `fr` from `en`" prefill / clone-to-locale.
- Automatic/machine translation.
- Locale-aware nuance in webhooks/audit beyond recording the locale.

## Data model

### Locales registry

New table (migration `0016_locales.sql`):

```
_locales (
  code        text PRIMARY KEY,      -- e.g. "en", "fr", "de"
  name        text NOT NULL,         -- display label
  is_default  boolean NOT NULL DEFAULT false,
  position    int NOT NULL DEFAULT 0
)
```

- Migration seeds one default row: `('en', 'English', true, 0)`.
- Invariant: **exactly one** row has `is_default = true`. Enforced in the
  CRUD layer (setting a new default clears the old one in the same tx;
  deleting the default is rejected).
- `code` validated as a locale tag (lowercase letters, optional `-REGION`,
  e.g. `en`, `pt-br`). Reject anything else 422.

`LocaleRegistry` — `RwLock`-backed cache mirroring `RoleRegistry` /
`SchemaRegistry`. Holds the locale list + default code. Hydrated at boot,
`reload_from_db` on every mutate. Lives in `crates/http/src/`.

### Per-type localization columns

`localized: bool` option on `ContentType` (parallels `draft_publish`). When
true, `ddl::create_table` emits, in addition to the existing columns:

```
"document_id" uuid NOT NULL,
"locale"      text NOT NULL,
UNIQUE ("document_id", "locale")
```

plus `CREATE INDEX ON ct_<name> ("document_id")`.

- `id` stays the per-row primary key (each locale row is its own row, own id).
- `document_id` is the stable cross-locale handle exposed in the public API.
- **Unique field columns** (`slug`/`email`/`string unique`/relation
  one-to-one) become **scoped** when the type is localized: the column-level
  `UNIQUE` is replaced by a table-level `UNIQUE ("document_id", "locale",
  "<col>")`. Without this, two locales of the same document cannot share a
  slug. (This is the one real schema wrinkle — global uniqueness is wrong for
  localized content.)

### Localizing an existing type (ALTER path)

`schema::sync` gains a localize transition (mirrors the `draft_publish`
add-column path):

1. `ALTER TABLE ct_<name> ADD COLUMN "document_id" uuid` (nullable first).
2. `ALTER TABLE ct_<name> ADD COLUMN "locale" text`.
3. Backfill: `UPDATE ct_<name> SET document_id = id, locale = '<default>'`
   (each existing row becomes the default-locale row of its own document).
4. `SET NOT NULL` on both; add `UNIQUE(document_id, locale)` + index.
5. Rewrite each unique column constraint to the scoped form.

De-localizing (localized → not) is **rejected** in v1 (ambiguous: which locale
survives?). Documented as a known limitation; revisit if needed.

## Read path

`?locale=<code>` accepted on REST list + get and as a GraphQL field argument.

- **No `?locale`** → default locale.
- **Unknown code** (not in registry) → 422 before touching the DB.
- **Non-localized type + `?locale`** → ignored (param is a no-op), not an error.

### Get (by document_id)

Public get path param is the **`document_id`** (the stable handle), not the
row id, for localized types:

1. Look up row `WHERE document_id = $1 AND locale = $2`.
2. Missing → look up `WHERE document_id = $1 AND locale = <default>`
   (fallback).
3. Still missing → 404.
4. `meta.locale` = the code of the row actually served (so the caller knows a
   fallback happened).

Non-localized types keep row-id `:id` lookups unchanged.

### List

- Base filter `WHERE locale = $1`.
- Fallback applied **per document**: documents with no row in the requested
  locale are represented by their default-locale row. Implementation: select
  rows where `locale = $requested OR (locale = $default AND document_id NOT IN
  (select document_id where locale = $requested))`. Keeps one row per document.
- Pagination/sort/filter (keyset + offset) compose on top of the locale
  filter unchanged.

## Write path

- **Create:** assign a fresh `document_id` (new document) unless the body
  supplies one (adding a translation to an existing document). `locale` from
  body or `?locale`, else default. Reject creating a `(document_id, locale)`
  that already exists (409, surfaces the unique violation cleanly).
- **Update / delete:** target the specific row. Locate by `document_id` +
  `locale` (public) — exact row resolution, **no fallback on writes** (writing
  must hit the intended locale or 404).
- **Publish/unpublish:** per row, reusing existing `published_at`. Locales
  publish independently for free.

## Shared content fns

The REST+GraphQL shared cores in `routes/content.rs`
(`get_entry/create_entry/update_entry/delete_entry/list_entries`) gain a
`locale: Option<&str>` parameter (or a small `LocaleSelector` carrying
requested + resolved). Fallback resolution and the unknown-code check live
**once** in these fns so REST and GraphQL behave identically. This extends the
existing pattern where both surfaces already delegate to these cores.

## Crate boundaries (no backward edges)

| Crate | Changes |
|---|---|
| `core` | `ContentType::localized()` option accessor; locale-tag validation helper; `localized` surfaced on the type. |
| `sql` | `ddl`: emit `document_id`/`locale`/scoped-unique/index; ALTER localize path. `dml`: locale-aware select (get/list w/ fallback) + insert. New `locales.rs` (list/get/upsert/delete/set_default/load_all). |
| `schema` | `sync`: localize transition (add cols + backfill + constraints); reject de-localize. |
| `http` | `LocaleRegistry` cache; `?locale` parse + unknown-code 422; fallback resolve in shared content fns; `/admin/locales` CRUD (admin-gated); GraphQL `locale` arg + schema rebuild already covers new arg. |
| `bin` | Hydrate `LocaleRegistry` at boot; wire into `AppState`. |

## API surface (additive, non-breaking)

- `GET/POST/PATCH/DELETE /admin/locales` — admin-gated CRUD.
- `?locale=<code>` on `GET /api/<type>` and `GET /api/<type>/:document_id`.
- `locale` field argument on GraphQL collection queries.
- `meta.locale` on responses for localized types (served code).
- Existing non-localized behavior is byte-for-byte unchanged.

## Error handling

- Unknown locale code → 422.
- Create duplicate `(document_id, locale)` → 409.
- Update/delete a missing `(document_id, locale)` → 404 (no fallback on write).
- Delete the default locale from `_locales` → 422 (must reassign default
  first).
- Localize a type that has a column whose existing data would violate the
  scoped unique → surfaced as the DB error, mapped to 409 with context.

## Testing (integration, `crates/bin/tests/localization.rs`)

1. Create same document in 2 locales; both readable by `document_id` + locale.
2. Read missing locale → default-locale row, `meta.locale` reflects fallback.
3. Unknown locale code → 422.
4. Slug unique **per locale** (same slug across locales OK; dup within a
   locale rejected).
5. List with `?locale=fr` → fr rows where present, default-locale rows
   otherwise, one row per document.
6. Localize an existing type → existing rows backfilled to default locale,
   still readable.
7. Per-locale publish independence (publish `fr`, `en` stays draft).
8. GraphQL `locale` argument matches REST behavior incl. fallback.
9. Create duplicate `(document_id, locale)` → 409.
10. `_locales` CRUD: set-default flips old default; delete-default rejected.

## Known limitations (documented, not bugs)

- De-localizing a type is rejected (would lose locale rows ambiguously).
- Relations target a specific locale **row** (`ct_<target>(id)`), not a
  document — cross-locale relation resolution is out of v1 scope.
- No prefill/clone-to-locale; each translation is authored from empty.
