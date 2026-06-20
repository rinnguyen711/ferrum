# Localization (i18n) — Admin UI Design

**Date:** 2026-06-20
**Branch:** `feat/localization-ui`
**Depends on:** localization backend (merged to main, spec `2026-06-19-localization-backend-design.md`).
**Scope:** React + TS admin UI (`ui/`). Full slice — Settings locales page, content-list locale UX, entry-editor locale switching.

## Goal

Expose the localization backend in the admin UI so an admin can: manage the
locale set, browse content per locale, and author/switch translations of an
entry. Match the existing `rs-` design system (DESIGN.md) exactly — no new
tokens, no new visual language.

## Design-system constraints (DESIGN.md)

- Use only existing CSS-variable tokens and `rs-` component classes. Never
  hard-code a hex a token covers.
- One primary action per view; everything else ghost.
- Theme + density are global; don't override locally.
- Every data view designs its loading / empty / error states. Localization
  adds a fourth: **missing translation** (editor) and **fallback row** (list).

## Decisions (locked during brainstorming)

- **Surfaces:** full slice — Settings + Editor + List.
- **Editor switcher:** locale dropdown in the `EditorBar` (next to status),
  each locale showing a status dot (current / draft / not-translated).
- **Routing:** `/content/:type/:id` unchanged, but for localized types `:id`
  is the **document_id**; `?locale=<code>` query selects the row. Non-localized
  types keep row-id addressing.
- **Add translation:** switching to a missing locale shows an empty form + a
  "Create <Locale> translation" primary action (no copy-from-default prefill).

## Architecture — four units

### Unit 1: API layer

Files: `ui/src/api/types.ts`, `ui/src/api/endpoints.ts`, new
`ui/src/api/locales.ts`.

- `types.ts`:
  - `ContentType.options` gains `localized?: boolean` (the `[key: string]:
    unknown` index already permits it; add the explicit key for typing).
  - `localizedEnabled(ct: ContentType): boolean` helper, mirroring
    `draftPublishEnabled` (returns `ct.options?.localized === true`).
  - `Entry` gains optional `document_id?: string` and `locale?: string`.
  - `ListResponse<T>.meta` gains optional `locale?: string`.
- `endpoints.ts`: thread an optional `locale?: string` through `listEntries`
  (into the existing `ListOpts`), `getEntry` (into its opts), `createEntry`,
  `updateEntry`, `deleteEntry`, `publishEntry`, `unpublishEntry`. When present,
  append `?locale=<code>` (merged with existing query params). When absent,
  the URL is byte-for-byte unchanged → non-localized callers unaffected.
- `locales.ts` (new): `Locale` type (`code`, `name`, `is_default`,
  `position`), `listLocales()`, `upsertLocale(body)`, `deleteLocale(code)` →
  `/admin/locales` (GET, POST, DELETE). Mirrors `api/webhooks.ts` style.

### Unit 2: Locales settings page

Files: new `ui/src/screens/Locales.tsx`; route `settings/locales` in
`App.tsx`; nav entry wherever settings sub-pages are listed (mirror the
Webhooks/Audit nav entry).

- `rs-cm` screen mirroring a settings sub-page (e.g. Webhooks). `rs-cm-head`
  with title "Locales" + count; one primary "Add locale" button.
- `rs-table` in `rs-table-wrap`: columns Code (`.rs-mono`), Name, Default
  (`.rs-status --ok` pill only on the default row), actions (`.rs-row-btn`
  delete; hidden/disabled on the default row).
- "Add locale" → `rs-modal`: code input (`.rs-input`, hint "lowercase, e.g.
  `fr` or `pt-br`"), name input, "Set as default" `rs-toggle`. Submit →
  `upsertLocale`. 422 (invalid code) → `.rs-err-msg` under the code field.
- Delete → on the default locale the server returns 422; surface as a `Notice`
  ("Set another locale as default first"). Non-default delete → row removed.
- States: loading (`LoadingState`), empty (only the seeded default — show it),
  error (`EmptyState` with message).

### Unit 3: Content list locale UX

File: `ui/src/screens/ContentList.tsx` (localized branch only; non-localized
rendering untouched).

- When `localizedEnabled(ct)`:
  - A locale selector (`select.rs-input rs-input--sm`) in the `rs-cm-head`
    toolbar region, options from `listLocales()`, seeded to the default locale,
    value persisted in the URL `?locale=<code>` (so refresh/bookmark keeps it).
  - Pass `locale` into `listEntries({ ..., locale })`.
  - A **Locale** column rendering the row's `locale` as a small `.rs-mono`
    code chip. When a row's served `locale` differs from the selected locale
    (fallback), append a muted "(fallback)" hint via `.rs-cell-muted`.
  - Row click / title link → `/content/:type/:documentId?locale=<selected>`
    using the row's `document_id` (NOT `id`).
- Non-localized types: no selector, no Locale column, links use `id` as today.

### Unit 4: Entry editor locale switcher

File: `ui/src/screens/EntryEditor.tsx` (+ the route param reinterpretation).

- For localized types, `:id` from the route is the **document_id**; the
  selected locale comes from `?locale=` (default-locale when absent). Load via
  `getEntry(type, documentId, { locale })`.
- Locale dropdown in `EditorBar` (next to status). Built from `listLocales()`.
  Each option shows a status dot:
  - **current** — the locale being edited;
  - **draft / published** — a row exists in that locale (reuse `StatusBadge`
    semantics or a dot);
  - **not translated** — no row for that locale on this document.
  Determining per-locale existence: a lightweight call — list this document's
  locales. Implementation: `listEntries(type, { locale, filters:
  [["filters[document_id][$eq]", documentId]] })` per known locale is wasteful;
  instead fetch each locale's get once lazily, OR (preferred) call
  `getEntry(type, documentId, { locale })` for the selected locale only and
  mark the others "unknown→resolve on switch". To keep v1 simple and correct:
  the dropdown lists all registered locales; the status dot shows "current" for
  the active one and is otherwise neutral until visited. (No N+1 prefetch.)
- Switching locale → set `?locale=<code>` (React Router `setSearchParams`),
  which reloads the entry for that locale.
- **Missing translation** (get returns 404 / no row for that locale): show an
  empty field form + a `Notice` ("No <Locale> translation yet.") and a primary
  "Create <Locale> translation" button that calls `createEntry(type, body, {
  locale })` with `document_id: documentId` in the body. On success, the new
  row exists; stay on the editor for that locale.
- Existing translation: normal Save (`updateEntry(type, documentId, body, {
  locale })`) and Publish/Unpublish (locale-scoped via the loaded row).
- **Fallback awareness:** if the response `locale` ≠ requested locale, the
  editor is showing the fallback row; surface a small inline hint and treat the
  primary action as "Create <Locale> translation" (so the user doesn't
  accidentally edit the default-locale row thinking it's the requested one).
- Non-localized editor path: **unchanged** (`:id` is row id, no locale).

## Data flow

list (locale select → `?locale` → server collapses one row/document) → row
click (document_id + locale → editor URL) → editor (loads that locale, or
offers Create) → save (locale-scoped write) → back to list (same `?locale`).

## States (per the interface-design craft skill)

- **List:** loading, empty, error, + fallback-row hint, + locale selector
  seeded to default.
- **Editor:** loading, missing-translation (empty + Create), fallback-shown
  hint, save/validation error (existing handling reused).
- **Locales page:** loading, empty (seeded default present), error, validation
  (invalid code 422, delete-default 422).

## Error handling

- Invalid locale code on add → 422 → inline field error.
- Delete default locale → 422 → notice.
- Editor save to a missing (document_id, locale) row → backend 404; duplicate
  create → 409 → reuse the editor's existing `ApiError` banner/field-error
  handling.
- Unknown locale in `?locale=` (e.g. stale URL) → backend 422 on the read →
  editor/list shows the error notice; the locale selector lets the user pick a
  valid one.

## Testing

UI has no automated test infra (per project memory — no UI test infra). Verify
via `pnpm typecheck` and `pnpm build` (both must pass), plus manual browser
verification by the user. Each unit must keep `pnpm typecheck` green. No new
test framework introduced.

## Crate / module boundaries

All changes are within `ui/`. API helpers stay in `ui/src/api/`; screens in
`ui/src/screens/`; no business logic in components beyond view state. Follow
existing file-per-screen and api-module conventions.

## Out of scope (v1)

- Copy-from-default translation prefill.
- Publish-by-locale convenience endpoint (publish stays per-loaded-row, already
  locale-scoped).
- GraphQL playground locale arg UI.
- Bulk translate / translation-progress dashboard.
- Per-locale required-field completeness indicators beyond the status dot.
