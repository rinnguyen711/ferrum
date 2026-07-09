# Media Library UI — Design

**Date:** 2026-06-04
**Scope:** Admin UI for the Media Library, wired to the real backend media API
(shipped 2026-06-03). One small backend addition (`?scope=all` on list-folders).
**Status:** Approved (brainstorm)

## Goal

Replace the static preview `ui/src/screens/MediaLibrary.tsx` with a fully
functional, API-wired Media Library screen matching the reference design in
`design/ferrum/media.jsx` and the provided mockup. Users browse nested folders
and assets, create/edit/delete folders, upload assets (real multipart), move
assets between folders (drag-drop and modal), multi-select for bulk actions, and
edit per-asset metadata (file name, alt text, caption). Real image thumbnails
render from the backend; non-images fall back to a gradient + extension badge.

The reference `media.jsx` is a client-only prototype with mock state. This spec
ports its layout and interactions to the real app's conventions (TypeScript,
React Router, the `api/` client + `useResource`) and wires every action to the
shipped backend endpoints.

## Key Decisions

- **Full wire-up.** Every action hits the real media API; mock data removed.
- **Real thumbnails.** Image assets render an `<img>` from an authed blob fetched
  from `GET /admin/media/assets/:id/raw`; non-images use the gradient + ext badge.
- **Honor backend folder-delete rule.** Delete only when empty; non-empty returns
  409 → clear message, folder stays. The prototype's reparent-on-delete is dropped.
- **Drop the "From URL" upload tab.** Backend only accepts multipart file upload;
  "From computer" only. Server-side URL fetch is out of scope.
- **Add an asset detail/edit panel.** Surfaces the metadata the backend stores
  (`file_name`, `alt_text`, `caption`) — the reason the feature exists. The
  prototype had none.
- **One small backend change:** `GET /admin/media/folders?scope=all` returns every
  folder flat, so the UI can build the full tree (breadcrumb, move/parent pickers)
  in one request. Existing `?parent_id=` behavior is unchanged.
- **No new UI test framework.** The UI has no test harness today; this pass adds
  none (YAGNI). Verification is manual against the running backend. The backend
  `?scope=all` change gets a Rust integration-test assertion in the existing harness.

## Backend Change (the only one)

`GET /admin/media/folders` currently filters by `parent_id` (omitted = root level
via `IS NOT DISTINCT FROM NULL`). Add an explicit `?scope=all` that returns every
folder, name-sorted, ignoring `parent_id`.

- `crates/http/src/media/store.rs` — add:
  ```rust
  pub async fn list_all_folders(pool: &PgPool) -> Result<Vec<FolderRow>, sqlx::Error> {
      let rows = sqlx::query_as::<_, FolderTuple>(&format!(
          "SELECT {FOLDER_COLS} FROM _media_folders ORDER BY name"
      ))
      .fetch_all(pool)
      .await?;
      Ok(rows.into_iter().map(folder_from).collect())
  }
  ```
- `crates/http/src/routes/media.rs` — extend `FolderQuery` with
  `scope: Option<String>`; in `list_folders`, if `scope.as_deref() == Some("all")`
  call `list_all_folders`, else the existing `list_folders(parent_id)`.
- Integration test (in `crates/bin/tests/media.rs`): create nested folders, assert
  `GET /admin/media/folders?scope=all` returns all of them while `?parent_id=` still
  returns one level.

## Frontend Architecture & Files

- **`ui/src/api/types.ts`** — add:
  - `MediaFolder { id, parent_id, name, created_at, updated_at }`
  - `MediaAsset { id, folder_id, file_name, alt_text, caption, mime_type, size_bytes, width, height, original_filename, created_at, updated_at }`
  - `NewFolder { name; parent_id?: string | null }`
  - `PatchFolder { name?; parent_id?: string | null }`
  - `PatchAsset { file_name?; alt_text?; caption?; folder_id?: string | null }`
- **`ui/src/api/client.ts`** — add two helpers alongside `apiFetch`:
  - `apiUpload<T>(path, formData)` — POST `FormData`; do **not** set
    `Content-Type` (browser sets the multipart boundary). Reuse the auth header +
    error parsing from `apiFetch`.
  - `fetchBlob(path)` — authed GET returning a `Blob` (for thumbnails/preview).
- **`ui/src/api/endpoints.ts`** — add:
  - `listFolders(opts?: { parentId?: string | null; all?: boolean })` →
    `?scope=all` when `all`, else `?parent_id=` when `parentId` set.
  - `createFolder(body: NewFolder)`, `updateFolder(id, body: PatchFolder)`,
    `deleteFolder(id)`.
  - `listAssets(folderId?: string | null)`, `getAsset(id)`,
    `updateAsset(id, body: PatchAsset)`, `deleteAsset(id)`.
  - `uploadAsset(file: File, folderId?: string | null)` — builds `FormData`
    (`file`, optional `folder_id`) and calls `apiUpload`.
  - `fetchAssetBlob(id)` → `fetchBlob('/admin/media/assets/:id/raw')`.
- **`ui/src/screens/MediaLibrary.tsx`** — main screen.
- **`ui/src/screens/media/Modal.tsx`** — shared modal shell (eyebrow, title, icon,
  footer, close, portal + overlay) + `ModalTabs`. Ports the prototype look.
- **`ui/src/screens/media/FolderModal.tsx`** — create/edit folder.
- **`ui/src/screens/media/UploadModal.tsx`** — drag/drop + browse, staged list,
  destination, multipart upload.
- **`ui/src/screens/media/MoveModal.tsx`** — folder picker for bulk move.
- **`ui/src/screens/media/AssetDetail.tsx`** — preview + edit metadata + delete.
- **`ui/src/screens/media/AssetThumb.tsx`** — authed-blob `<img>` with gradient
  fallback; revokes the object URL on unmount.
- **`ui/src/screens/media/Checkbox.tsx`** — local checkbox (port the `rs-check`
  pattern already in `ContentList.tsx`).
- **`ui/src/styles.css`** — port the missing `rs-*` classes from
  `design/ferrum/styles.css`: `rs-dropzone`, `rs-folder-grid`, `rs-folder-card`,
  `rs-folder-ico`, `rs-folder-meta`, `rs-folder-menu`, `rs-media-bc`,
  `rs-media-sectionhead`, `rs-count-pill`, `rs-foldpick` (+ items/radio),
  `rs-stage-list`/`rs-stage-row`/`rs-stage-thumb`/`rs-stage-meta`,
  `rs-media-empty`, `rs-media-check`, `rs-media-cover`/`rs-media-ext`,
  `rs-row-btn`, `rs-link-btn`, `rs-mono`, and any asset-detail panel classes.
  Reuse existing `rs-modal`, `rs-btn`, `rs-input`, `rs-field(s)`, `rs-search`,
  `rs-bulkbar`, `rs-media-grid`, `rs-media-card`, `rs-check` (already present).

Rationale: each modal/thumb/detail is its own file with one responsibility, keeping
the main screen readable and the units independently understandable.

## Data Flow & State

`MediaLibrary` owns: `cur` (current folder id, `null` = root), `query`, `sort`
(`newest | oldest | name`), `selected: string[]`, `modal` (`null | 'folder' |
'upload' | 'move' | { editFolder }`), `detailAsset: MediaAsset | null`,
`dropTarget`, and drag state.

- **Load:** on mount, fetch the full folder tree once via `listFolders({ all: true })`.
  Fetch assets for the current folder via `listAssets(cur)` on mount and whenever
  `cur` changes (`useResource` with `[cur]`).
- **Derive:** breadcrumb = walk `parent_id` from `cur` up to root using the tree;
  folder grid = tree folders with `parent_id === cur`; folder/parent pickers +
  move picker = the whole tree; header counts = assets length / folders length /
  summed `size_bytes` (MB).
- **Search/sort:** client-side over the current folder's assets (matches the
  prototype): filter by `file_name`, sort by created/ name.
- **Mutations** (each: call endpoint → refetch affected data → clear transient UI):
  - create/edit folder → refetch tree, close modal.
  - delete folder → `deleteFolder` → on success refetch tree; on 409 show the
    "not empty" message and keep the folder.
  - upload → sequential `uploadAsset(file, dest)` per staged file → refetch assets;
    if `dest !== cur`, navigate to `dest`.
  - move (drag or modal) → `updateAsset(id, { folder_id: dest })` per selected →
    clear selection → refetch assets.
  - edit asset metadata → `updateAsset` → refetch assets (and close detail).
  - delete asset(s) → `deleteAsset` per id → clear selection → refetch assets.

Refetch-after-mutate (rather than optimistic local edits) keeps the client and
server in sync simply and avoids stale-state bugs; volumes are small (admin tool).

## Components (behavior summary)

- **MediaLibrary** — header + counts + actions; breadcrumb; search + sort toolbar;
  bulk-select bar (Move / Delete / Clear) shown when `selected.length > 0`; folder
  grid (click to enter, edit/delete menu icons, drop target for asset drag); asset
  grid (checkbox select, drag source, click opens detail); empty state.
- **AssetThumb** — `useEffect` fetches `fetchAssetBlob(id)` when `mime_type`
  starts with `image/`; creates an object URL, renders `<img>`; revokes on cleanup.
  Otherwise renders the gradient (`coverBg(hueFromId)`) + ext badge derived from
  `mime_type`/`original_filename`. A fetch error falls back to the gradient.
- **Modal / ModalTabs** — portal overlay; props `eyebrow, title, icon, wide,
  footer, onClose, children`. `ModalTabs` renders the tab row used by UploadModal.
- **FolderModal** — `name` (required) + `Location` parent select (root + tree,
  excluding self when editing). Save → `createFolder` / `updateFolder`. Maps a 409
  to an inline name error.
- **UploadModal** — "From computer" only: `<input type="file" multiple>` + drag/drop
  dropzone; staged rows (thumb, name, size, remove); destination select (defaults to
  current folder); Upload button uploads each staged file via `uploadAsset`. Failed
  files are marked; the rest proceed.
- **MoveModal** — radio-style folder picker (root + full tree). Move → `updateAsset`
  `folder_id` for each selected.
- **AssetDetail** — slide-over/modal: `AssetThumb` preview + read-only mime/size/
  dims + editable `file_name`, `alt_text`, `caption`; Save → `updateAsset`; Delete →
  `deleteAsset` (with confirm). Opened from an asset card's menu, distinct from the
  checkbox select used for bulk actions.
- **Checkbox** — local, `rs-check` styling.

## Error Handling & Edge Cases

- All calls surface `ApiError`. 409 on folder create/edit (dup name) → inline field
  error. 409 on folder delete (non-empty) → message "Folder not empty — move or
  delete its contents first"; folder remains.
- Upload: per-file failure marks that staged row failed and continues the others;
  report how many succeeded/failed.
- Asset metadata save: field-level validation errors mapped onto the inputs.
- 401 → existing global auth handler (redirect to login); no change here.
- Thumbnail fetch failure → gradient fallback (no broken-image icon).
- Object URLs revoked on unmount to avoid leaks.
- Drag-move onto the current folder, or move with no change, is a no-op.
- Empty folder/root → empty-state card; search with no matches → "no assets match".

## Testing

- **UI:** no automated test harness exists in `ui/`; none is added (YAGNI). Verify
  manually with `pnpm dev` (UI) against a running backend: browse folders/root,
  breadcrumb nav, search + sort, create/edit/delete folder (including a blocked
  non-empty delete), upload an image (confirm a real thumbnail renders) and a
  non-image (confirm gradient + ext badge), move an asset by drag onto a folder and
  via the Move modal, open an asset, edit file name/alt/caption and save, delete an
  asset.
- **Backend:** add a Rust integration-test assertion in `crates/bin/tests/media.rs`
  for `?scope=all` (returns all folders) vs the existing `?parent_id=` (one level).

## Out of Scope (future)

- "From URL" upload / server-side remote fetch.
- Image thumbnail/variant generation server-side (UI downloads full `/raw` and the
  browser scales it for now).
- Provider settings UI page (separate spec).
- Automated UI test framework.
- Pagination/virtualization of very large folders.
