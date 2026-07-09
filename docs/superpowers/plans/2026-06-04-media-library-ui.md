# Media Library UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the static `MediaLibrary.tsx` preview with a fully API-wired Media Library: browse nested folders + assets, create/edit/delete folders, multipart upload, drag/modal move, multi-select bulk actions, per-asset metadata editing, and real image thumbnails — matching `design/ferrum/media.jsx`.

**Architecture:** One small backend addition (`GET /admin/media/folders?scope=all` returns the whole folder tree flat). The frontend adds media types + endpoints, a `FormData` upload helper and an authed blob fetch helper to the API client, then a `MediaLibrary` screen composed of focused sub-components under `ui/src/screens/media/` (Modal shell, FolderModal, UploadModal, MoveModal, AssetDetail, AssetThumb, Checkbox). Missing `rs-*` CSS is ported from `design/ferrum/styles.css`.

**Tech Stack:** Backend: Rust, Axum, sqlx (existing testcontainers harness). Frontend: TypeScript, React 18, React Router, Vite. No new UI test framework — verify via `pnpm typecheck`/`pnpm build` + manual run.

**Reference files (read before starting):**
- Design source (port layout + interactions): `design/ferrum/media.jsx`; CSS to port: `design/ferrum/styles.css` (media block ~line 432+, plus `rs-dropzone`, `rs-foldpick`, `rs-stage-*`).
- API client (extend): `ui/src/api/client.ts` (`apiFetch`, `ApiError`, `getToken`).
- Endpoints pattern: `ui/src/api/endpoints.ts`. Types: `ui/src/api/types.ts`.
- Real modal pattern to match: `ui/src/builder/CreateTypeModal.tsx` (`rs-modal-backdrop` > `rs-modal` with `rs-modal-head/body/foot`; backdrop-click + Esc close; NOT the prototype's `Modal` component).
- Checkbox + select/drag pattern: `ui/src/screens/ContentList.tsx` (`Checkbox` at ~line 223, `rs-check`).
- Data fetching: `ui/src/hooks/useResource.ts`. Routing: `ui/src/App.tsx` (route `media` already mounted → `MediaLibrary`).
- Backend media: `crates/http/src/routes/media.rs` (`FolderQuery`, `list_folders`), `crates/http/src/media/store.rs` (`list_folders`, `FolderRow`, `FOLDER_COLS`), tests `crates/bin/tests/media.rs`, harness `crates/bin/tests/common/mod.rs`.

**Icons:** `ui/src/components/icons.tsx` already has `check, edit, home, image, plus, search, sort, trash, x`. It is MISSING `folder, folderPlus, folderInput, upload` — Task 4 adds them.

---

## File Structure

**Backend (Task 1):**
- Modify: `crates/http/src/media/store.rs` — add `list_all_folders`.
- Modify: `crates/http/src/routes/media.rs` — `FolderQuery.scope`, branch in `list_folders`.
- Modify: `crates/bin/tests/media.rs` — `?scope=all` assertion.

**Frontend:**
- Modify: `ui/src/api/types.ts` (Task 2) — media types.
- Modify: `ui/src/api/client.ts` (Task 3) — `apiUpload`, `fetchBlob`.
- Modify: `ui/src/api/endpoints.ts` (Task 3) — media endpoint fns.
- Modify: `ui/src/components/icons.tsx` (Task 4) — 4 icons.
- Modify: `ui/src/styles.css` (Task 5) — port missing media CSS.
- Create: `ui/src/screens/media/Checkbox.tsx` (Task 6)
- Create: `ui/src/screens/media/Modal.tsx` (Task 6)
- Create: `ui/src/screens/media/AssetThumb.tsx` (Task 7)
- Create: `ui/src/screens/media/FolderModal.tsx` (Task 8)
- Create: `ui/src/screens/media/MoveModal.tsx` (Task 9)
- Create: `ui/src/screens/media/UploadModal.tsx` (Task 10)
- Create: `ui/src/screens/media/AssetDetail.tsx` (Task 11)
- Rewrite: `ui/src/screens/MediaLibrary.tsx` (Task 12)

Each frontend file is one concern. The main screen orchestrates; modals/thumb/detail are independent.

---

## Task 1: Backend — `?scope=all` folder listing (TDD)

**Files:**
- Modify: `crates/http/src/media/store.rs`
- Modify: `crates/http/src/routes/media.rs`
- Test: `crates/bin/tests/media.rs`

- [ ] **Step 1: Write the failing integration test**

Append to `crates/bin/tests/media.rs`:

```rust
#[tokio::test]
async fn folders_scope_all_returns_full_tree() {
    let app = TestApp::spawn().await;

    // root folder
    let root: serde_json::Value = app.admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "covers" }))
        .send().await.unwrap().json().await.unwrap();
    let rid = root["id"].as_str().unwrap().to_string();

    // nested child
    app.admin(app.client.post(app.url("/admin/media/folders")))
        .json(&serde_json::json!({ "name": "2026", "parent_id": rid }))
        .send().await.unwrap();

    // ?parent_id= (root level) returns only the root folder
    let level: serde_json::Value = app.admin(app.client.get(app.url("/admin/media/folders")))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(level.as_array().unwrap().len(), 1, "root level shows one folder");

    // ?scope=all returns both
    let all: serde_json::Value = app.admin(app.client.get(app.url("/admin/media/folders?scope=all")))
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(all.as_array().unwrap().len(), 2, "scope=all shows every folder");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p ferrum-bin --test media folders_scope_all_returns_full_tree`
Expected: FAIL — `?scope=all` is currently ignored, so the last assert sees 1 (only root level), not 2. (Docker required; available.)

- [ ] **Step 3: Add `list_all_folders` to the DAL**

In `crates/http/src/media/store.rs`, after `list_folders`, add:

```rust
/// Every folder, name-sorted, ignoring hierarchy. Backs the UI tree builder.
pub async fn list_all_folders(pool: &PgPool) -> Result<Vec<FolderRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, FolderTuple>(&format!(
        "SELECT {FOLDER_COLS} FROM _media_folders ORDER BY name"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(folder_from).collect())
}
```

- [ ] **Step 4: Branch in the handler**

In `crates/http/src/routes/media.rs`, change `FolderQuery` and `list_folders`:

```rust
#[derive(Deserialize)]
struct FolderQuery {
    parent_id: Option<Uuid>,
    scope: Option<String>,
}

async fn list_folders(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(q): Query<FolderQuery>,
) -> Result<Json<Vec<FolderView>>, ApiError> {
    ensure(&state, &principal, Action::ContentRead).await?;
    let rows = if q.scope.as_deref() == Some("all") {
        store::list_all_folders(&state.pool).await.map_err(internal)?
    } else {
        store::list_folders(&state.pool, q.parent_id).await.map_err(internal)?
    };
    Ok(Json(rows.into_iter().map(FolderView::from).collect()))
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p ferrum-bin --test media folders_scope_all_returns_full_tree`
Expected: PASS. Then `cargo test -p ferrum-bin --test media` — all media tests still PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/http/src/media/store.rs crates/http/src/routes/media.rs crates/bin/tests/media.rs
git commit -m "feat(media): list all folders via ?scope=all"
```

---

## Task 2: Frontend media types

**Files:**
- Modify: `ui/src/api/types.ts`

- [ ] **Step 1: Append the media types**

Add to `ui/src/api/types.ts`:

```typescript
export interface MediaFolder {
  id: string;
  parent_id: string | null;
  name: string;
  created_at: string;
  updated_at: string;
}

export interface MediaAsset {
  id: string;
  folder_id: string | null;
  file_name: string;
  alt_text: string | null;
  caption: string | null;
  mime_type: string;
  size_bytes: number;
  width: number | null;
  height: number | null;
  original_filename: string;
  created_at: string;
  updated_at: string;
}

export interface NewFolder {
  name: string;
  parent_id?: string | null;
}

export interface PatchFolder {
  name?: string;
  parent_id?: string | null;
}

export interface PatchAsset {
  file_name?: string;
  alt_text?: string;
  caption?: string;
  folder_id?: string | null;
}
```

- [ ] **Step 2: Verify it typechecks**

Run: `cd ui && pnpm typecheck`
Expected: PASS (no errors introduced).

- [ ] **Step 3: Commit**

```bash
git add ui/src/api/types.ts
git commit -m "feat(media-ui): media folder + asset types"
```

---

## Task 3: API client upload/blob helpers + media endpoints

**Files:**
- Modify: `ui/src/api/client.ts`
- Modify: `ui/src/api/endpoints.ts`

- [ ] **Step 1: Add `apiUpload` and `fetchBlob` to the client**

In `ui/src/api/client.ts`, after `apiFetch`, add (reuse the same auth + 401 + error-parsing shape):

```typescript
/** POST multipart FormData. Browser sets Content-Type (with boundary). */
export async function apiUpload<T>(path: string, form: FormData): Promise<T> {
  const token = getToken();
  const headers: Record<string, string> = { Accept: "application/json" };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  let resp: Response;
  try {
    resp = await fetch(path, { method: "POST", headers, body: form });
  } catch {
    throw new ApiError(0, "network", "Can't reach the API.");
  }

  if (resp.status === 401) {
    if (onAuthError) onAuthError();
    throw new AuthError("Invalid or missing credentials.");
  }
  if (resp.status === 204) return undefined as T;

  let payload: unknown = null;
  const text = await resp.text();
  if (text) { try { payload = JSON.parse(text); } catch { payload = null; } }

  if (!resp.ok) {
    type WireField = { field: string; reason?: string; message?: string };
    const env = (payload as { error?: { code?: string; message?: string; details?: { fields?: WireField[] } } } | null)?.error;
    const code = env?.code ?? "error";
    const message = env?.message ?? `Request failed (${resp.status}).`;
    const fieldErrors: FieldError[] = (env?.details?.fields ?? []).map((f) => ({
      field: f.field, message: f.reason ?? f.message,
    }));
    throw new ApiError(resp.status, code, message, fieldErrors);
  }
  return payload as T;
}

/** Authed GET returning the raw bytes as a Blob (for thumbnails/preview). */
export async function fetchBlob(path: string): Promise<Blob> {
  const token = getToken();
  const headers: Record<string, string> = {};
  if (token) headers["Authorization"] = `Bearer ${token}`;
  let resp: Response;
  try {
    resp = await fetch(path, { headers });
  } catch {
    throw new ApiError(0, "network", "Can't reach the API.");
  }
  if (resp.status === 401) {
    if (onAuthError) onAuthError();
    throw new AuthError("Invalid or missing credentials.");
  }
  if (!resp.ok) throw new ApiError(resp.status, "error", `Request failed (${resp.status}).`);
  return resp.blob();
}
```

- [ ] **Step 2: Add media endpoint functions**

In `ui/src/api/endpoints.ts`: extend the imports from `./client` to include `apiUpload` and `fetchBlob`; extend the type import to include the media types:

```typescript
import { apiFetch, apiUpload, fetchBlob } from "./client";
```
```typescript
import type {
  // ...existing...
  MediaFolder,
  MediaAsset,
  NewFolder,
  PatchFolder,
  PatchAsset,
} from "./types";
```

Append the functions:

```typescript
export function listFolders(opts: { parentId?: string | null; all?: boolean } = {}): Promise<MediaFolder[]> {
  if (opts.all) return apiFetch<MediaFolder[]>("/admin/media/folders?scope=all");
  const q = opts.parentId != null ? `?parent_id=${encodeURIComponent(opts.parentId)}` : "";
  return apiFetch<MediaFolder[]>(`/admin/media/folders${q}`);
}

export function createFolder(body: NewFolder): Promise<MediaFolder> {
  return apiFetch<MediaFolder>("/admin/media/folders", { method: "POST", body });
}

export function updateFolder(id: string, body: PatchFolder): Promise<MediaFolder> {
  return apiFetch<MediaFolder>(`/admin/media/folders/${id}`, { method: "PATCH", body });
}

export function deleteFolder(id: string): Promise<void> {
  return apiFetch<void>(`/admin/media/folders/${id}`, { method: "DELETE" });
}

export function listAssets(folderId?: string | null): Promise<MediaAsset[]> {
  const q = folderId != null ? `?folder_id=${encodeURIComponent(folderId)}` : "";
  return apiFetch<MediaAsset[]>(`/admin/media/assets${q}`);
}

export function getAsset(id: string): Promise<MediaAsset> {
  return apiFetch<MediaAsset>(`/admin/media/assets/${id}`);
}

export function updateAsset(id: string, body: PatchAsset): Promise<MediaAsset> {
  return apiFetch<MediaAsset>(`/admin/media/assets/${id}`, { method: "PATCH", body });
}

export function deleteAsset(id: string): Promise<void> {
  return apiFetch<void>(`/admin/media/assets/${id}`, { method: "DELETE" });
}

export function uploadAsset(file: File, folderId?: string | null): Promise<MediaAsset> {
  const form = new FormData();
  form.append("file", file);
  if (folderId != null) form.append("folder_id", folderId);
  return apiUpload<MediaAsset>("/admin/media/assets", form);
}

export function fetchAssetBlob(id: string): Promise<Blob> {
  return fetchBlob(`/admin/media/assets/${id}/raw`);
}
```

> NOTE: confirm the backend list endpoints accept these query params. Per the backend, folders use `?parent_id=` / `?scope=all`, assets use `?folder_id=`. Omitting the param lists root-level.

- [ ] **Step 3: Verify it typechecks**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/api/client.ts ui/src/api/endpoints.ts
git commit -m "feat(media-ui): upload + blob client helpers and media endpoints"
```

---

## Task 4: Add missing icons

**Files:**
- Modify: `ui/src/components/icons.tsx`

- [ ] **Step 1: Add the four missing icons**

`ui/src/components/icons.tsx` exports an `Icons` object mapping names to components that take `{ size }`. Read an existing entry (e.g. `home`, `image`) to match the exact prop signature and SVG wrapper style, then add `folder`, `folderPlus`, `folderInput`, `upload` in the same style. Use these path bodies inside the project's standard `<svg>` wrapper (stroke-based, `currentColor`, `strokeWidth={1.8}` or whatever the existing icons use — match them):

- `folder`: `<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z"/>`
- `folderPlus`: folder path above plus `<path d="M12 11v4M10 13h4"/>`
- `folderInput`: folder path above plus an arrow `<path d="M12 11v5M9.5 13.5 12 16l2.5-2.5"/>`
- `upload`: `<path d="M12 16V4M8 8l4-4 4 4"/><path d="M4 16v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/>`

> Match the existing icons' exact wrapper (viewBox, fill/stroke, linecap). Copy the structure of a neighboring icon and swap the inner paths. The visual exactness matters less than: the four names exist and render an SVG sized by `size`.

- [ ] **Step 2: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add ui/src/components/icons.tsx
git commit -m "feat(media-ui): add folder/folderPlus/folderInput/upload icons"
```

---

## Task 5: Port missing media CSS

**Files:**
- Modify: `ui/src/styles.css`

- [ ] **Step 1: Identify and copy the missing rules**

Open `design/ferrum/styles.css` and copy the rule blocks for these selectors into `ui/src/styles.css` (append at the end, under a `/* Media Library */` comment). Copy verbatim — they use the same CSS variables (`--text`, `--surface-2`, `--border`, etc.) that `ui/src/styles.css` already defines:

- `.rs-media-bc`, `.rs-media-bc-sep`, `.rs-media-bc-here` (breadcrumb)
- `.rs-media-sectionhead` (+ its `h2`, `.rs-count-pill`, `.rs-spacer`)
- `.rs-folder-grid`, `.rs-folder-card`, `.rs-folder-ico`, `.rs-folder-meta`, `.rs-folder-menu`, `.rs-folder-card.is-drop`
- `.rs-media-cover`, `.rs-media-ext`, `.rs-media-check`, `.rs-media-card.is-selected`, `.rs-media-card-meta`, `.rs-media-card-text`
- `.rs-dropzone`, `.rs-dropzone.is-over`, `.rs-dropzone-ico`
- `.rs-foldpick`, `.rs-foldpick-item`, `.is-nested`, `.is-on`, `.rs-radio-dot`
- `.rs-stage-list`, `.rs-stage-row`, `.rs-stage-thumb`, `.rs-stage-meta`, `.rs-row-btn`
- `.rs-media-empty`, `.rs-media-empty-ico`
- `.rs-link-btn`, `.rs-danger` (if not already present), `.rs-mono` (if not already present), `.rs-count-pill` (if not already present)

> DO NOT duplicate selectors already in `ui/src/styles.css` (it already has `rs-media-grid`, `rs-media-card`, `rs-modal`, `rs-btn`, `rs-input`, `rs-field(s)`, `rs-search`, `rs-bulkbar`, `rs-check`). Before adding each block, grep `ui/src/styles.css` for the selector; skip ones that already exist. If a needed `.rs-modal-backdrop`/`.rs-modal-head/body/foot` is already present (used by CreateTypeModal), reuse it — do not redefine.

- [ ] **Step 2: Verify the build still compiles CSS**

Run: `cd ui && pnpm build`
Expected: build succeeds (Vite bundles CSS; no syntax errors).

- [ ] **Step 3: Commit**

```bash
git add ui/src/styles.css
git commit -m "feat(media-ui): port folder/upload/picker styles from design"
```

---

## Task 6: Shared Checkbox + Modal components

**Files:**
- Create: `ui/src/screens/media/Checkbox.tsx`
- Create: `ui/src/screens/media/Modal.tsx`

- [ ] **Step 1: Create the Checkbox** (port from `ContentList.tsx`)

`ui/src/screens/media/Checkbox.tsx`:

```tsx
import { Icons } from "../../components/icons";

export function Checkbox({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <button
      className={"rs-check" + (checked ? " is-on" : "")}
      onClick={onChange}
      role="checkbox"
      aria-checked={checked}
      type="button"
    >
      {checked && <Icons.check size={13} />}
    </button>
  );
}
```

- [ ] **Step 2: Create the Modal shell** (match `CreateTypeModal`'s `rs-modal-backdrop`/`rs-modal` structure, generalized)

`ui/src/screens/media/Modal.tsx`:

```tsx
import { ReactNode } from "react";
import { Icons } from "../../components/icons";

type IconName = keyof typeof Icons;

export function Modal({
  eyebrow, title, icon, wide, footer, onClose, children,
}: {
  eyebrow?: string;
  title: string;
  icon?: IconName;
  wide?: boolean;
  footer?: ReactNode;
  onClose: () => void;
  children: ReactNode;
}) {
  const IconCmp = icon ? Icons[icon] : null;
  return (
    <div className="rs-modal-backdrop" onClick={onClose}>
      <div
        className={"rs-modal" + (wide ? " rs-modal--wide" : "")}
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => { if (e.key === "Escape") onClose(); }}
      >
        <div className="rs-modal-head">
          {IconCmp && <span className="rs-modal-ico"><IconCmp size={18} /></span>}
          <div>
            {eyebrow && <span className="rs-modal-eyebrow">{eyebrow}</span>}
            <h2>{title}</h2>
          </div>
          <button className="rs-modal-x" onClick={onClose} aria-label="Close" type="button">
            <Icons.x size={18} />
          </button>
        </div>
        <div className="rs-modal-body">{children}</div>
        {footer && <div className="rs-modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

export function ModalTabs({
  tab, setTab, tabs,
}: {
  tab: string;
  setTab: (t: string) => void;
  tabs: [string, string][];
}) {
  return (
    <div className="rs-modal-tabs">
      {tabs.map(([key, label]) => (
        <button
          key={key}
          type="button"
          className={"rs-modal-tab" + (tab === key ? " is-on" : "")}
          onClick={() => setTab(key)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
```

> If `.rs-modal-eyebrow`, `.rs-modal-ico`, `.rs-modal-x`, `.rs-modal--wide`, `.rs-modal-tabs`, `.rs-modal-tab` are not in `ui/src/styles.css` after Task 5, add minimal rules for them in this task's CSS (append to `ui/src/styles.css`): eyebrow = small uppercase muted text; `rs-modal-x` = ghost icon button top-right; `rs-modal--wide` = wider max-width (~620px); tabs = a row of underline-style buttons with `.is-on` active. Keep consistent with existing modal styling.

- [ ] **Step 3: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS (unused-export warnings are fine; these are consumed in later tasks).

- [ ] **Step 4: Commit**

```bash
git add ui/src/screens/media/Checkbox.tsx ui/src/screens/media/Modal.tsx ui/src/styles.css
git commit -m "feat(media-ui): shared Checkbox + Modal shell"
```

---

## Task 7: AssetThumb (authed blob image + gradient fallback)

**Files:**
- Create: `ui/src/screens/media/AssetThumb.tsx`

- [ ] **Step 1: Create the thumbnail component**

`ui/src/screens/media/AssetThumb.tsx`:

```tsx
import { useEffect, useState } from "react";
import { fetchAssetBlob } from "../../api/endpoints";
import type { MediaAsset } from "../../api/types";

/** Stable gradient from the asset id (fallback for non-images / load errors). */
function coverBg(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return `linear-gradient(135deg, hsl(${h} 50% 80%), hsl(${(h + 18) % 360} 45% 62%))`;
}

function extOf(a: MediaAsset): string {
  const m = a.original_filename.match(/\.([a-z0-9]+)$/i);
  if (m) return m[1].toUpperCase();
  const sub = a.mime_type.split("/")[1];
  return (sub || "file").toUpperCase();
}

export function AssetThumb({ asset, className }: { asset: MediaAsset; className?: string }) {
  const isImage = asset.mime_type.startsWith("image/");
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!isImage) return;
    let revoked = false;
    let objectUrl: string | null = null;
    fetchAssetBlob(asset.id)
      .then((blob) => {
        if (revoked) return;
        objectUrl = URL.createObjectURL(blob);
        setUrl(objectUrl);
      })
      .catch(() => setFailed(true));
    return () => {
      revoked = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [asset.id, isImage]);

  const cls = "rs-media-cover" + (className ? " " + className : "");
  if (isImage && url && !failed) {
    return (
      <div className={cls} style={{ backgroundImage: `url(${url})`, backgroundSize: "cover", backgroundPosition: "center" }}>
        <span className="rs-media-ext rs-mono">{extOf(asset)}</span>
      </div>
    );
  }
  return (
    <div className={cls} style={{ background: coverBg(asset.id) }}>
      <span className="rs-media-ext rs-mono">{extOf(asset)}</span>
    </div>
  );
}
```

- [ ] **Step 2: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/media/AssetThumb.tsx
git commit -m "feat(media-ui): AssetThumb with authed image + gradient fallback"
```

---

## Task 8: FolderModal (create / edit)

**Files:**
- Create: `ui/src/screens/media/FolderModal.tsx`

- [ ] **Step 1: Create the folder modal**

`ui/src/screens/media/FolderModal.tsx`:

```tsx
import { useState, useEffect, useRef } from "react";
import { Modal } from "./Modal";
import { Icons } from "../../components/icons";
import type { MediaFolder } from "../../api/types";

export function FolderModal({
  folders, currentFolder, editing, onClose, onSubmit,
}: {
  folders: MediaFolder[];
  currentFolder: string | null;
  editing?: MediaFolder;
  onClose: () => void;
  onSubmit: (data: { name: string; parent_id: string | null }) => Promise<void>;
}) {
  const [name, setName] = useState(editing ? editing.name : "");
  const [parent, setParent] = useState<string | null>(editing ? editing.parent_id : currentFolder);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => { ref.current?.focus(); }, []);

  const valid = name.trim().length > 0;
  // cannot reparent a folder into itself
  const parentOptions = folders.filter((f) => !editing || f.id !== editing.id);

  const submit = async () => {
    if (!valid || busy) return;
    setBusy(true); setError(null);
    try {
      await onSubmit({ name: name.trim(), parent_id: parent });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save folder.");
      setBusy(false);
    }
  };

  return (
    <Modal
      eyebrow={editing ? "Edit folder" : "New folder"}
      title={name.trim() || "Untitled folder"}
      icon="folderPlus"
      onClose={onClose}
      footer={<>
        <button className="rs-btn rs-btn--ghost" onClick={onClose} type="button">Cancel</button>
        <button className="rs-btn rs-btn--primary" disabled={!valid || busy} onClick={submit} type="button">
          <Icons.check size={15} /> {editing ? "Save folder" : "Create folder"}
        </button>
      </>}
    >
      {error && <div className="rs-login-error" style={{ marginBottom: 12 }}>{error}</div>}
      <div className="rs-fields">
        <div className="rs-field">
          <div className="rs-field-label"><label>Name</label><span className="rs-field-hint">Shown in the media library</span></div>
          <input
            ref={ref}
            className="rs-input"
            value={name}
            placeholder="e.g. Article covers"
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
          />
        </div>
        <div className="rs-field">
          <div className="rs-field-label"><label>Location</label></div>
          <select
            className="rs-input"
            value={parent ?? ""}
            onChange={(e) => setParent(e.target.value === "" ? null : e.target.value)}
          >
            <option value="">Media Library (root)</option>
            {parentOptions.map((f) => <option key={f.id} value={f.id}>{f.name}</option>)}
          </select>
        </div>
      </div>
    </Modal>
  );
}
```

- [ ] **Step 2: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/media/FolderModal.tsx
git commit -m "feat(media-ui): create/edit folder modal"
```

---

## Task 9: MoveModal (bulk move picker)

**Files:**
- Create: `ui/src/screens/media/MoveModal.tsx`

- [ ] **Step 1: Create the move modal**

`ui/src/screens/media/MoveModal.tsx`:

```tsx
import { useState } from "react";
import { Modal } from "./Modal";
import { Icons } from "../../components/icons";
import type { MediaFolder } from "../../api/types";

export function MoveModal({
  folders, count, onClose, onMove,
}: {
  folders: MediaFolder[];
  count: number;
  onClose: () => void;
  onMove: (dest: string | null) => Promise<void>;
}) {
  // `undefined` = nothing picked; `null` = root chosen; string = folder id
  const [dest, setDest] = useState<string | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const roots = folders.filter((f) => f.parent_id == null);
  const childrenOf = (id: string) => folders.filter((f) => f.parent_id === id);

  const Row = ({ f, nested }: { f: MediaFolder; nested?: boolean }) => (
    <button
      type="button"
      className={"rs-foldpick-item" + (nested ? " is-nested" : "") + (dest === f.id ? " is-on" : "")}
      onClick={() => setDest(f.id)}
    >
      <span className="rs-folder-ico"><Icons.folder size={17} /></span>
      <strong>{f.name}</strong>
      <span className="rs-radio-dot" />
    </button>
  );

  const confirm = async () => {
    if (dest === undefined || busy) return;
    setBusy(true);
    try { await onMove(dest); } finally { setBusy(false); }
  };

  return (
    <Modal
      eyebrow={count + " asset" + (count === 1 ? "" : "s") + " selected"}
      title="Move to folder"
      icon="folderInput"
      onClose={onClose}
      footer={<>
        <button className="rs-btn rs-btn--ghost" onClick={onClose} type="button">Cancel</button>
        <button className="rs-btn rs-btn--primary" disabled={dest === undefined || busy} onClick={confirm} type="button">
          <Icons.folderInput size={15} /> Move here
        </button>
      </>}
    >
      <div className="rs-foldpick">
        <button
          type="button"
          className={"rs-foldpick-item" + (dest === null ? " is-on" : "")}
          onClick={() => setDest(null)}
        >
          <span className="rs-folder-ico"><Icons.home size={17} /></span>
          <strong>Media Library (root)</strong>
          <span className="rs-radio-dot" />
        </button>
        {roots.map((f) => (
          <div key={f.id}>
            <Row f={f} />
            {childrenOf(f.id).map((c) => <Row key={c.id} f={c} nested />)}
          </div>
        ))}
      </div>
    </Modal>
  );
}
```

- [ ] **Step 2: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/media/MoveModal.tsx
git commit -m "feat(media-ui): move-to-folder modal"
```

---

## Task 10: UploadModal (multipart upload)

**Files:**
- Create: `ui/src/screens/media/UploadModal.tsx`

- [ ] **Step 1: Create the upload modal**

`ui/src/screens/media/UploadModal.tsx`:

```tsx
import { useState, useRef } from "react";
import { Modal } from "./Modal";
import { Icons } from "../../components/icons";
import type { MediaFolder } from "../../api/types";

interface Staged { file: File; sid: string; status: "ready" | "failed"; }

let _seq = 0;

export function UploadModal({
  folders, currentFolder, onClose, onUpload,
}: {
  folders: MediaFolder[];
  currentFolder: string | null;
  onClose: () => void;
  onUpload: (files: File[], dest: string | null) => Promise<{ ok: number; failed: number }>;
}) {
  const [staged, setStaged] = useState<Staged[]>([]);
  const [dest, setDest] = useState<string | null>(currentFolder);
  const [over, setOver] = useState(false);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const addFiles = (files: FileList | File[]) => {
    const next = Array.from(files).map((file) => ({ file, sid: "s" + (++_seq), status: "ready" as const }));
    setStaged((s) => [...s, ...next]);
  };
  const removeStaged = (sid: string) => setStaged((s) => s.filter((x) => x.sid !== sid));

  const folderName = (id: string | null) =>
    id == null ? "Media Library" : (folders.find((f) => f.id === id)?.name ?? "Media Library");

  const upload = async () => {
    if (!staged.length || busy) return;
    setBusy(true);
    try {
      await onUpload(staged.map((s) => s.file), dest);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      wide
      eyebrow={"Upload to " + folderName(dest)}
      title="Add new assets"
      icon="upload"
      onClose={onClose}
      footer={<>
        <button className="rs-btn rs-btn--ghost" onClick={onClose} type="button">Cancel</button>
        <div className="rs-spacer" />
        <label style={{ display: "flex", alignItems: "center", marginRight: 8 }}>
          <span className="rs-cell-muted" style={{ fontSize: 12.5, marginRight: 8 }}>Destination</span>
          <select
            className="rs-input rs-input--sm"
            value={dest ?? ""}
            onChange={(e) => setDest(e.target.value === "" ? null : e.target.value)}
          >
            <option value="">Media Library</option>
            {folders.map((f) => <option key={f.id} value={f.id}>{f.name}</option>)}
          </select>
        </label>
        <button className="rs-btn rs-btn--primary" disabled={!staged.length || busy} onClick={upload} type="button">
          <Icons.check size={15} /> Upload {staged.length || ""} asset{staged.length === 1 ? "" : "s"}
        </button>
      </>}
    >
      <input
        ref={inputRef}
        type="file"
        multiple
        style={{ display: "none" }}
        onChange={(e) => { if (e.target.files) addFiles(e.target.files); e.target.value = ""; }}
      />
      <div
        className={"rs-dropzone" + (over ? " is-over" : "")}
        onClick={() => inputRef.current?.click()}
        onDragOver={(e) => { e.preventDefault(); setOver(true); }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => { e.preventDefault(); setOver(false); if (e.dataTransfer.files) addFiles(e.dataTransfer.files); }}
      >
        <div className="rs-dropzone-ico"><Icons.upload size={22} /></div>
        <strong>Drag &amp; drop files here</strong>
        <span>or click to browse</span>
      </div>

      {staged.length > 0 && (
        <>
          <div className="rs-media-sectionhead" style={{ marginTop: 18 }}>
            <h2>Ready to upload</h2><span className="rs-count-pill">{staged.length}</span>
            <div className="rs-spacer" />
            <button className="rs-link-btn rs-danger" onClick={() => setStaged([])} type="button">Clear all</button>
          </div>
          <div className="rs-stage-list">
            {staged.map((s) => (
              <div className="rs-stage-row" key={s.sid}>
                <div className="rs-stage-thumb"><span className="rs-mono">{(s.file.name.split(".").pop() || "file").toUpperCase()}</span></div>
                <div className="rs-stage-meta">
                  <strong title={s.file.name}>{s.file.name}{s.status === "failed" ? " — failed" : ""}</strong>
                  <span className="rs-mono">{(s.file.size / 1048576).toFixed(1)} MB</span>
                </div>
                <button className="rs-row-btn" onClick={() => removeStaged(s.sid)} type="button"><Icons.x size={16} /></button>
              </div>
            ))}
          </div>
        </>
      )}
    </Modal>
  );
}
```

> The parent (MediaLibrary) implements `onUpload`: iterate files, call `uploadAsset(file, dest)` for each, count successes/failures, refetch, and navigate to `dest` if it differs from the current folder.

- [ ] **Step 2: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/media/UploadModal.tsx
git commit -m "feat(media-ui): multipart upload modal"
```

---

## Task 11: AssetDetail (preview + edit metadata + delete)

**Files:**
- Create: `ui/src/screens/media/AssetDetail.tsx`

- [ ] **Step 1: Create the detail/edit modal**

`ui/src/screens/media/AssetDetail.tsx`:

```tsx
import { useState } from "react";
import { Modal } from "./Modal";
import { AssetThumb } from "./AssetThumb";
import { Icons } from "../../components/icons";
import type { MediaAsset, PatchAsset } from "../../api/types";

export function AssetDetail({
  asset, onClose, onSave, onDelete,
}: {
  asset: MediaAsset;
  onClose: () => void;
  onSave: (patch: PatchAsset) => Promise<void>;
  onDelete: () => Promise<void>;
}) {
  const [fileName, setFileName] = useState(asset.file_name);
  const [alt, setAlt] = useState(asset.alt_text ?? "");
  const [caption, setCaption] = useState(asset.caption ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const save = async () => {
    if (busy || !fileName.trim()) return;
    setBusy(true); setError(null);
    try {
      await onSave({ file_name: fileName.trim(), alt_text: alt, caption });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save.");
      setBusy(false);
    }
  };

  const remove = async () => {
    if (busy) return;
    if (!window.confirm(`Delete "${asset.file_name}"? This cannot be undone.`)) return;
    setBusy(true);
    try { await onDelete(); } catch (e) {
      setError(e instanceof Error ? e.message : "Could not delete.");
      setBusy(false);
    }
  };

  const dims = asset.width && asset.height ? `${asset.width}×${asset.height} · ` : "";
  const sizeMb = (asset.size_bytes / 1048576).toFixed(1) + " MB";

  return (
    <Modal
      wide
      eyebrow="Asset"
      title={asset.file_name}
      icon="image"
      onClose={onClose}
      footer={<>
        <button className="rs-btn rs-btn--ghost rs-danger" onClick={remove} type="button"><Icons.trash size={15} /> Delete</button>
        <div className="rs-spacer" />
        <button className="rs-btn rs-btn--ghost" onClick={onClose} type="button">Cancel</button>
        <button className="rs-btn rs-btn--primary" disabled={busy || !fileName.trim()} onClick={save} type="button">
          <Icons.check size={15} /> Save
        </button>
      </>}
    >
      {error && <div className="rs-login-error" style={{ marginBottom: 12 }}>{error}</div>}
      <div className="rs-asset-detail">
        <AssetThumb asset={asset} className="rs-asset-detail-preview" />
        <p className="rs-cell-muted rs-mono" style={{ marginTop: 8 }}>{asset.mime_type} · {dims}{sizeMb}</p>
        <div className="rs-fields" style={{ marginTop: 12 }}>
          <div className="rs-field">
            <div className="rs-field-label"><label>File name</label></div>
            <input className="rs-input" value={fileName} onChange={(e) => setFileName(e.target.value)} />
          </div>
          <div className="rs-field">
            <div className="rs-field-label"><label>Alternative text</label><span className="rs-field-hint">For accessibility</span></div>
            <input className="rs-input" value={alt} onChange={(e) => setAlt(e.target.value)} placeholder="Describe the image" />
          </div>
          <div className="rs-field">
            <div className="rs-field-label"><label>Caption</label></div>
            <textarea className="rs-input rs-textarea" rows={2} value={caption} onChange={(e) => setCaption(e.target.value)} />
          </div>
        </div>
      </div>
    </Modal>
  );
}
```

> If `.rs-asset-detail`, `.rs-asset-detail-preview`, `.rs-textarea` aren't styled, add minimal rules to `ui/src/styles.css`: preview = a ~220px tall cover block; textarea = the input style with `min-height`. (Check first; `rs-textarea` may already exist.)

- [ ] **Step 2: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/media/AssetDetail.tsx ui/src/styles.css
git commit -m "feat(media-ui): asset detail + metadata edit modal"
```

---

## Task 12: MediaLibrary screen (orchestration + wiring)

**Files:**
- Rewrite: `ui/src/screens/MediaLibrary.tsx`

- [ ] **Step 1: Rewrite the screen**

Replace the entire contents of `ui/src/screens/MediaLibrary.tsx`:

```tsx
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Icons } from "../components/icons";
import {
  listFolders, createFolder, updateFolder, deleteFolder,
  listAssets, updateAsset, deleteAsset, uploadAsset,
} from "../api/endpoints";
import { ApiError } from "../api/client";
import type { MediaFolder, MediaAsset } from "../api/types";
import { Checkbox } from "./media/Checkbox";
import { AssetThumb } from "./media/AssetThumb";
import { FolderModal } from "./media/FolderModal";
import { MoveModal } from "./media/MoveModal";
import { UploadModal } from "./media/UploadModal";
import { AssetDetail } from "./media/AssetDetail";

type ModalState = null | "folder" | "upload" | "move" | { editFolder: MediaFolder };
type Sort = "newest" | "oldest" | "name";

export function MediaLibrary() {
  const [folders, setFolders] = useState<MediaFolder[]>([]);
  const [assets, setAssets] = useState<MediaAsset[]>([]);
  const [cur, setCur] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("newest");
  const [selected, setSelected] = useState<string[]>([]);
  const [modal, setModal] = useState<ModalState>(null);
  const [detail, setDetail] = useState<MediaAsset | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const dragIds = useRef<string[]>([]);

  const reloadFolders = useCallback(async () => {
    setFolders(await listFolders({ all: true }));
  }, []);
  const reloadAssets = useCallback(async (folderId: string | null) => {
    setAssets(await listAssets(folderId));
  }, []);

  useEffect(() => { reloadFolders().catch(() => {}); }, [reloadFolders]);
  useEffect(() => { reloadAssets(cur).catch(() => {}); }, [cur, reloadAssets]);

  // breadcrumb path root→cur from the flat tree
  const path = useMemo(() => {
    const chain: MediaFolder[] = [];
    let id: string | null = cur;
    const byId = new Map(folders.map((f) => [f.id, f]));
    while (id != null) {
      const f = byId.get(id);
      if (!f) break;
      chain.unshift(f);
      id = f.parent_id;
    }
    return chain;
  }, [cur, folders]);

  const subFolders = folders.filter((f) => f.parent_id === cur);
  const folderCount = (id: string) => folders.filter((f) => f.parent_id === id).length;
  // asset counts for sub-folders are not separately fetched; show "—" is avoided by
  // counting only the loaded current-folder assets. Sub-folder asset counts are unknown
  // here; display folder count only when asset count is unknown.
  const totalMb = (assets.reduce((n, a) => n + a.size_bytes, 0) / 1048576).toFixed(1);

  let visible = assets.slice();
  if (query) visible = visible.filter((a) => a.file_name.toLowerCase().includes(query.toLowerCase()));
  visible.sort((a, b) =>
    sort === "name" ? a.file_name.localeCompare(b.file_name)
      : sort === "oldest" ? a.created_at.localeCompare(b.created_at)
        : b.created_at.localeCompare(a.created_at));

  const flash = (msg: string) => { setNotice(msg); setTimeout(() => setNotice(null), 4000); };

  // ---- mutations ----
  const onCreateFolder = async (data: { name: string; parent_id: string | null }) => {
    await createFolder({ name: data.name, parent_id: data.parent_id });
    await reloadFolders();
    setModal(null);
  };
  const onEditFolder = async (id: string, data: { name: string; parent_id: string | null }) => {
    await updateFolder(id, { name: data.name, parent_id: data.parent_id });
    await reloadFolders();
    setModal(null);
  };
  const onDeleteFolder = async (id: string) => {
    if (!window.confirm("Delete this folder?")) return;
    try {
      await deleteFolder(id);
      await reloadFolders();
      if (cur === id) setCur(folders.find((f) => f.id === id)?.parent_id ?? null);
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        flash("Folder not empty — move or delete its contents first.");
      } else {
        flash(e instanceof Error ? e.message : "Could not delete folder.");
      }
    }
  };
  const onUpload = async (files: File[], dest: string | null) => {
    let ok = 0, failed = 0;
    for (const f of files) {
      try { await uploadAsset(f, dest); ok++; } catch { failed++; }
    }
    setModal(null);
    if (dest !== cur) setCur(dest); else await reloadAssets(cur);
    if (failed) flash(`Uploaded ${ok}, ${failed} failed.`);
    return { ok, failed };
  };
  const moveAssets = async (ids: string[], dest: string | null) => {
    for (const id of ids) await updateAsset(id, { folder_id: dest });
    setSelected([]);
    setModal(null);
    await reloadAssets(cur);
  };
  const onDeleteAssets = async (ids: string[]) => {
    if (!window.confirm(`Delete ${ids.length} asset${ids.length === 1 ? "" : "s"}?`)) return;
    for (const id of ids) await deleteAsset(id);
    setSelected([]);
    await reloadAssets(cur);
  };
  const onSaveAsset = async (id: string, patch: Parameters<typeof updateAsset>[1]) => {
    await updateAsset(id, patch);
    setDetail(null);
    await reloadAssets(cur);
  };
  const onDeleteAsset = async (id: string) => {
    await deleteAsset(id);
    setDetail(null);
    await reloadAssets(cur);
  };

  const toggleSel = (id: string) =>
    setSelected((s) => s.includes(id) ? s.filter((x) => x !== id) : [...s, id]);

  const onAssetDragStart = (id: string) => { dragIds.current = selected.includes(id) ? selected : [id]; };
  const onFolderDrop = (folderId: string) => {
    if (dragIds.current.length && folderId !== cur) moveAssets(dragIds.current, folderId);
    setDropTarget(null);
    dragIds.current = [];
  };

  const isEmpty = subFolders.length === 0 && visible.length === 0;

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Media Library</h1>
          <p className="rs-cm-sub">{assets.length} assets · {folders.length} folders · {totalMb} MB</p>
        </div>
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost" onClick={() => setModal("folder")} type="button"><Icons.folderPlus size={16} /> Add new folder</button>
          <button className="rs-btn rs-btn--primary" onClick={() => setModal("upload")} type="button"><Icons.upload size={16} /> Add new assets</button>
        </div>
      </div>

      <div className="rs-media-bc">
        <button className={path.length === 0 ? "rs-media-bc-here" : ""} onClick={() => setCur(null)} type="button">Media Library</button>
        {path.map((f, i) => (
          <span key={f.id} style={{ display: "contents" }}>
            <span className="rs-media-bc-sep">/</span>
            <button className={i === path.length - 1 ? "rs-media-bc-here" : ""} onClick={() => setCur(f.id)} type="button">{f.name}</button>
          </span>
        ))}
      </div>

      <div className="rs-cm-toolbar">
        <div className="rs-search rs-search--inline">
          <Icons.search size={15} />
          <input placeholder="Search assets" value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <div className="rs-spacer" />
        <button className="rs-btn rs-btn--ghost" type="button"
          onClick={() => setSort(sort === "newest" ? "oldest" : sort === "oldest" ? "name" : "newest")}>
          <Icons.sort size={15} /> {sort === "name" ? "Name (A–Z)" : sort === "oldest" ? "Oldest first" : "Newest first"}
        </button>
      </div>

      {notice && <div className="rs-login-error" style={{ marginBottom: 12 }}>{notice}</div>}

      {selected.length > 0 && (
        <div className="rs-bulkbar">
          <span><strong>{selected.length}</strong> selected</span>
          <div className="rs-bulkbar-actions">
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setModal("move")} type="button"><Icons.folderInput size={14} /> Move to folder</button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm rs-danger" onClick={() => onDeleteAssets(selected)} type="button"><Icons.trash size={14} /> Delete</button>
            <button className="rs-btn rs-btn--ghost rs-btn--sm" onClick={() => setSelected([])} type="button">Clear</button>
          </div>
        </div>
      )}

      {isEmpty ? (
        <div className="rs-media-empty">
          <div className="rs-media-empty-ico"><Icons.image size={24} /></div>
          <h3>{query ? "No assets match your search" : "This folder is empty"}</h3>
          <p>{query ? "Try a different search term." : "Upload assets or create a folder to start organizing your media."}</p>
          {!query && <div className="rs-editor-actions" style={{ marginTop: 10 }}>
            <button className="rs-btn rs-btn--ghost" onClick={() => setModal("folder")} type="button"><Icons.folderPlus size={16} /> New folder</button>
            <button className="rs-btn rs-btn--primary" onClick={() => setModal("upload")} type="button"><Icons.upload size={16} /> Add assets</button>
          </div>}
        </div>
      ) : (
        <>
          {subFolders.length > 0 && (
            <>
              <div className="rs-media-sectionhead"><h2>Folders</h2><span className="rs-count-pill">{subFolders.length}</span></div>
              <div className="rs-folder-grid">
                {subFolders.map((f) => (
                  <div key={f.id}
                    className={"rs-folder-card" + (dropTarget === f.id ? " is-drop" : "")}
                    onClick={() => setCur(f.id)}
                    onDragOver={(e) => { if (dragIds.current.length) { e.preventDefault(); setDropTarget(f.id); } }}
                    onDragLeave={() => setDropTarget((d) => d === f.id ? null : d)}
                    onDrop={(e) => { e.preventDefault(); onFolderDrop(f.id); }}>
                    <span className="rs-folder-ico"><Icons.folder size={22} /></span>
                    <span className="rs-folder-meta">
                      <strong title={f.name}>{f.name}</strong>
                      <span>{folderCount(f.id)} folder{folderCount(f.id) === 1 ? "" : "s"}</span>
                    </span>
                    <span className="rs-folder-menu" title="Edit folder"
                      onClick={(e) => { e.stopPropagation(); setModal({ editFolder: f }); }}><Icons.edit size={16} /></span>
                    <span className="rs-folder-menu" title="Delete folder"
                      onClick={(e) => { e.stopPropagation(); onDeleteFolder(f.id); }}><Icons.trash size={16} /></span>
                  </div>
                ))}
              </div>
            </>
          )}

          {visible.length > 0 && (
            <>
              <div className="rs-media-sectionhead">
                <h2>Assets</h2><span className="rs-count-pill">{visible.length}</span>
                <div className="rs-spacer" />
                <button className="rs-link-btn" type="button"
                  onClick={() => setSelected(selected.length === visible.length ? [] : visible.map((a) => a.id))}>
                  {selected.length === visible.length ? "Deselect all" : "Select all"}
                </button>
              </div>
              <div className="rs-media-grid">
                {visible.map((m) => {
                  const sel = selected.includes(m.id);
                  return (
                    <div className={"rs-media-card" + (sel ? " is-selected" : "")} key={m.id}
                      draggable onDragStart={() => onAssetDragStart(m.id)}
                      onClick={() => setDetail(m)}>
                      <div className="rs-media-check" onClick={(e) => { e.stopPropagation(); toggleSel(m.id); }}>
                        <Checkbox checked={sel} onChange={() => toggleSel(m.id)} />
                      </div>
                      <AssetThumb asset={m} />
                      <div className="rs-media-card-meta">
                        <span className="rs-media-card-text">
                          <strong title={m.file_name}>{m.file_name}</strong>
                          <span className="rs-cell-muted rs-mono">
                            {m.width && m.height ? `${m.width}×${m.height} · ` : ""}{(m.size_bytes / 1048576).toFixed(1)} MB
                          </span>
                        </span>
                      </div>
                    </div>
                  );
                })}
              </div>
            </>
          )}
        </>
      )}

      {modal === "folder" && (
        <FolderModal folders={folders} currentFolder={cur} onClose={() => setModal(null)} onSubmit={onCreateFolder} />
      )}
      {modal && typeof modal === "object" && "editFolder" in modal && (
        <FolderModal folders={folders} currentFolder={cur} editing={modal.editFolder}
          onClose={() => setModal(null)} onSubmit={(d) => onEditFolder(modal.editFolder.id, d)} />
      )}
      {modal === "upload" && (
        <UploadModal folders={folders} currentFolder={cur} onClose={() => setModal(null)} onUpload={onUpload} />
      )}
      {modal === "move" && (
        <MoveModal folders={folders} count={selected.length} onClose={() => setModal(null)} onMove={(dest) => moveAssets(selected, dest)} />
      )}
      {detail && (
        <AssetDetail asset={detail} onClose={() => setDetail(null)}
          onSave={(patch) => onSaveAsset(detail.id, patch)} onDelete={() => onDeleteAsset(detail.id)} />
      )}
    </div>
  );
}
```

> NOTE on sub-folder asset counts: the design shows "N folders · M assets" per folder card. We fetch assets only for the current folder, so per-sub-folder asset counts aren't available without extra calls. This plan shows folder-count only (decided to avoid N extra requests). If the design fidelity requires the asset count, a follow-up can add a lightweight count endpoint; not in scope here.

- [ ] **Step 2: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS — no type errors. Resolve any (likely: an unused import, or the `ModalState` narrowing on `editFolder`).

- [ ] **Step 3: Build**

Run: `cd ui && pnpm build`
Expected: production build succeeds.

- [ ] **Step 4: Manual verification (against running backend)**

Start backend (`cargo run -p ferrum` with a Postgres + `FERRUM_STUDIO_DIR`, or `docker compose up`) and `cd ui && pnpm dev`. Log in, open Media Library, and verify:
- Empty root shows the empty state; "Add new folder" creates a folder (appears under Folders).
- Creating a duplicate-named folder at the same level surfaces an error.
- "Add new assets" → drop/browse an image → Upload → a real thumbnail renders in the grid; upload a PDF/non-image → gradient + ext badge.
- Click into a folder → breadcrumb updates; breadcrumb navigates back.
- Drag an asset onto a folder card → it moves (disappears from current view).
- Select assets (checkbox) → bulk bar → Move to folder (modal) works; Delete works.
- Click an asset (not the checkbox) → detail opens; edit file name/alt/caption → Save persists (reopen shows new values); Delete removes it.
- Delete a non-empty folder → "Folder not empty" notice; folder remains.

Record the results (what worked / any issues).

- [ ] **Step 5: Commit**

```bash
git add ui/src/screens/MediaLibrary.tsx
git commit -m "feat(media-ui): wire Media Library screen to the media API"
```

---

## Self-Review Notes (addressed)

- **Spec coverage:** backend `?scope=all` (T1); types (T2); upload/blob client + endpoints (T3); icons (T4); CSS port (T5); Checkbox+Modal (T6); real thumbnails w/ fallback (T7); folder create/edit (T8); move picker (T9); multipart upload (T10); asset detail/metadata edit + delete (T11); browse/breadcrumb/search/sort/select/drag-move/empty-state + honor-409 folder delete (T12). All spec sections mapped.
- **Type consistency:** `MediaFolder`/`MediaAsset`/`NewFolder`/`PatchFolder`/`PatchAsset` defined in T2, consumed everywhere with matching field names (`parent_id`, `folder_id`, `file_name`, `alt_text`, `size_bytes`). Endpoint fn names (`listFolders`,`createFolder`,`updateFolder`,`deleteFolder`,`listAssets`,`updateAsset`,`deleteAsset`,`uploadAsset`,`fetchAssetBlob`) defined T3, used T7/T12. `Modal`/`ModalTabs` props consistent T6→T8–T11. Icon names `folder/folderPlus/folderInput/upload` added T4, used T8–T12.
- **Placeholders:** none — every code step is complete. Two explicit scope notes (sub-folder asset counts deferred; CSS classes to verify-before-adding) are guidance, not missing code.
- **Known deviations from prototype (intentional, per spec):** folder delete honors backend 409 (no client reparent); URL upload tab dropped; asset card click opens detail (prototype toggled selection — selection moved to the checkbox) — this is a deliberate UX choice to expose the metadata editor; documented in the spec.
- **Testing posture:** UI has no automated harness; verification is typecheck + build + the manual checklist in T12. Backend change is TDD (T1). Consistent with the repo's current UI testing approach.
