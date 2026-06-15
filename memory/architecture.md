# Architecture

System design, services, data flow.

<!-- Keep in sync with the crate-boundary table in CLAUDE.md. -->

## Builder draft flow (since 2026-06-12)
- `BuilderDraftContext` is the single source of truth for whatever the Builder edits:
  `BuilderDraft = Draft ("new" | "existing" content type) | ComponentDraft ("component")`
- Editors seed it (`loadExisting` / `loadExistingComponent`), mutate via `setDraft` with
  per-editor narrowing helpers (`setTypeDraft` / `setCompDraft`), save/discard via context
- `SaveBar` (floating, dirty-only) is the only save entry point; field-drop confirm modal lives
  inside it. `dirty` drives `beforeunload` + `guardedNavigate` in sidebar
- `saveNonce` bump → sidebar type/component lists refetch

## Content list query flow (since 2026-06-12)
- Everything server-side through `listEntries` params: `page`/`pageSize`/`sort` + `status`
  (draft-publish) + `filters` pairs (`filters[col][$op]=value`, parsed by
  `crates/http/src/filter.rs`)
- UI folds user filter rules + title search (`$containsi`) + enum-status tab (`$eq`) into one
  pair list, debounced 300ms as JSON string; page resets to 1 and selection clears on any
  query-shape change
- `FiltersMenu` owns rule UI + `serializeFilters`; ops-per-kind table lives there
