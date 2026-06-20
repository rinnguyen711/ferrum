# Localization Admin UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the localization backend in the React admin UI — a Locales settings page, a locale selector + Locale column on the content list, and a locale switcher + translation authoring in the entry editor.

**Architecture:** All changes live in `ui/`. An optional `locale` flows from the URL `?locale=` through the existing `api/endpoints.ts` helpers (appended as a query param only when present, so non-localized callers are byte-for-byte unchanged). Localized content types are detected via a new `localizedEnabled(ct)` helper mirroring `draftPublishEnabled`. For localized types the editor route `:id` is reinterpreted as the backend `document_id`.

**Tech Stack:** React 18 + TypeScript + Vite + React Router. Design system: `rs-` classes + CSS-variable tokens (DESIGN.md). No automated UI test infra exists — **verification per task is `pnpm typecheck` (and `pnpm build` at the end)** plus the stated manual check. There is no test runner to add; do NOT introduce one.

**Spec:** `docs/superpowers/specs/2026-06-20-localization-ui-design.md`

**Conventions (read before starting):**
- API helpers: `ui/src/api/`. `apiFetch<T>(path, { method, body })` (see `api/client.ts`). Errors throw `ApiError` with `.status`, `.message`, `.fieldErrors`.
- Screens: `ui/src/screens/`, one file per screen. Routes in `ui/src/App.tsx`. Settings nav in `ui/src/components/shell.tsx` `SettingsPanel` (an "Internationalization" placeholder item already exists at ~line 263 with no `to`).
- Design tokens/classes only (DESIGN.md). `select.rs-input`, `rs-btn--primary/--ghost`, `rs-modal`, `rs-table`, `rs-status`, `rs-mono`, `Notice`, `LoadingState`, `EmptyState`, `StatusBadge` (type `Status = "published" | "draft" | "review"`), `EditorBar` (props `onBack`, `title`, `status`, `actions`).
- Run all `pnpm` commands from `ui/` (`cd ui` first).

---

## File Structure

- `ui/src/api/types.ts` — add `localized?` option key, `localizedEnabled()`, `Entry.document_id?/locale?`, `ListResponse.meta.locale?`.
- `ui/src/api/endpoints.ts` — thread `locale?` through entry CRUD + publish.
- `ui/src/api/locales.ts` (new) — `Locale` type + `/admin/locales` client fns.
- `ui/src/screens/Locales.tsx` (new) — locales settings page.
- `ui/src/App.tsx` — `settings/locales` route.
- `ui/src/components/shell.tsx` — wire the "Internationalization" nav item to `/settings/locales`.
- `ui/src/screens/ContentList.tsx` — locale selector + Locale column + document_id links (localized only).
- `ui/src/screens/EntryEditor.tsx` — locale switcher, document_id addressing, create-translation flow.

---

## Task 1: API types — localized helpers + entry/list shape

**Files:**
- Modify: `ui/src/api/types.ts`

- [ ] **Step 1: Add the `localized` option key + helper + entry/list fields**

In `ui/src/api/types.ts`:

Change the `ContentType.options` type (currently `{ draft_publish?: boolean; managed?: boolean; [key: string]: unknown }`) to include `localized`:

```typescript
  options?: { draft_publish?: boolean; managed?: boolean; localized?: boolean; [key: string]: unknown };
```

Add to the `Entry` type (after `published_at?`):

```typescript
  document_id?: string;
  locale?: string;
```

Change `ListResponse<T>.meta` to:

```typescript
  meta: { page: number; pageSize: number; total: number; locale?: string };
```

Add the helper next to `draftPublishEnabled`:

```typescript
export function localizedEnabled(ct: ContentType): boolean {
  return ct.options?.localized === true;
}
```

- [ ] **Step 2: Verify typecheck passes**

Run: `cd ui && pnpm typecheck`
Expected: no errors (additive optional fields don't break existing code).

- [ ] **Step 3: Commit**

```bash
git add ui/src/api/types.ts
git commit -m "feat(ui): localized() helper + locale fields on Entry/ListResponse"
```

---

## Task 2: API endpoints — thread `locale` through entry CRUD

**Files:**
- Modify: `ui/src/api/endpoints.ts`

- [ ] **Step 1: Add `locale` to `ListOpts` and the list query**

In `ui/src/api/endpoints.ts`, add to the `ListOpts` interface:

```typescript
  locale?: string;
```

In `listEntries`, after the other `q.set(...)` lines and before `for (const [k, v] of opts.filters ...)`, add:

```typescript
  if (opts.locale) q.set("locale", opts.locale);
```

- [ ] **Step 2: Thread `locale` through get/create/update/delete/publish/unpublish**

Replace `getEntry` with:

```typescript
export function getEntry(
  type: string,
  id: string,
  opts: { populate?: string; locale?: string } = {},
): Promise<Entry> {
  const q = new URLSearchParams();
  if (opts.populate) q.set("populate", opts.populate);
  if (opts.locale) q.set("locale", opts.locale);
  const qs = q.toString();
  return apiFetch<Entry>(`/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}${qs ? `?${qs}` : ""}`);
}
```

Replace `createEntry`:

```typescript
export function createEntry(
  type: string,
  body: Record<string, unknown>,
  opts: { locale?: string } = {},
): Promise<Entry> {
  const qs = opts.locale ? `?locale=${encodeURIComponent(opts.locale)}` : "";
  return apiFetch<Entry>(`/api/${encodeURIComponent(type)}${qs}`, { method: "POST", body });
}
```

Replace `updateEntry`:

```typescript
export function updateEntry(
  type: string,
  id: string,
  body: Record<string, unknown>,
  opts: { locale?: string } = {},
): Promise<Entry> {
  const qs = opts.locale ? `?locale=${encodeURIComponent(opts.locale)}` : "";
  return apiFetch<Entry>(`/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}${qs}`, {
    method: "PUT",
    body,
  });
}
```

Replace `deleteEntry`:

```typescript
export function deleteEntry(
  type: string,
  id: string,
  opts: { locale?: string } = {},
): Promise<void> {
  const qs = opts.locale ? `?locale=${encodeURIComponent(opts.locale)}` : "";
  return apiFetch<void>(`/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}${qs}`, {
    method: "DELETE",
  });
}
```

Replace `publishEntry` and `unpublishEntry`:

```typescript
export function publishEntry(type: string, id: string, opts: { locale?: string } = {}): Promise<Entry> {
  const qs = opts.locale ? `?locale=${encodeURIComponent(opts.locale)}` : "";
  return apiFetch<Entry>(
    `/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}/publish${qs}`,
    { method: "POST" },
  );
}

export function unpublishEntry(type: string, id: string, opts: { locale?: string } = {}): Promise<Entry> {
  const qs = opts.locale ? `?locale=${encodeURIComponent(opts.locale)}` : "";
  return apiFetch<Entry>(
    `/api/${encodeURIComponent(type)}/${encodeURIComponent(id)}/unpublish${qs}`,
    { method: "POST" },
  );
}
```

> Note: the publish/unpublish backend endpoints are keyed by row id and do not currently read `?locale=` (per the backend spec's v1 deferral). The optional `opts.locale` is plumbed for forward-compatibility and is harmless (ignored server-side); existing callers pass no opts and are unchanged.

- [ ] **Step 3: Verify typecheck passes**

Run: `cd ui && pnpm typecheck`
Expected: no errors. All existing call sites (ContentList, EntryEditor) pass no `opts.locale`, so they still compile.

- [ ] **Step 4: Commit**

```bash
git add ui/src/api/endpoints.ts
git commit -m "feat(ui): thread optional locale through entry CRUD endpoints"
```

---

## Task 3: API locales client module

**Files:**
- Create: `ui/src/api/locales.ts`

- [ ] **Step 1: Create the module**

Create `ui/src/api/locales.ts` (mirrors `api/webhooks.ts` style):

```typescript
import { apiFetch } from "./client";

export interface Locale {
  code: string;
  name: string;
  is_default: boolean;
  position: number;
}

export interface UpsertLocaleBody {
  code: string;
  name: string;
  position?: number;
  is_default?: boolean;
}

/** GET /admin/locales → { data: Locale[] }. */
export function listLocales(): Promise<Locale[]> {
  return apiFetch<{ data: Locale[] }>("/admin/locales").then((r) => r.data);
}

export function upsertLocale(body: UpsertLocaleBody): Promise<Locale> {
  return apiFetch<Locale>("/admin/locales", { method: "POST", body });
}

export function deleteLocale(code: string): Promise<void> {
  return apiFetch<void>(`/admin/locales/${encodeURIComponent(code)}`, { method: "DELETE" });
}
```

> The backend `GET /admin/locales` returns `{ "data": [...] }` (see `routes/locales.rs::list`). `listLocales` unwraps `.data`. `POST` returns the `Locale` JSON; `DELETE` returns 204 (→ `undefined`).

- [ ] **Step 2: Verify typecheck passes**

Run: `cd ui && pnpm typecheck`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add ui/src/api/locales.ts
git commit -m "feat(ui): /admin/locales API client module"
```

---

## Task 4: Locales settings page

**Files:**
- Create: `ui/src/screens/Locales.tsx`
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/components/shell.tsx`

- [ ] **Step 1: Create the screen**

Create `ui/src/screens/Locales.tsx`:

```tsx
import { useState } from "react";
import { Icons } from "../components/icons";
import { LoadingState, EmptyState, Notice } from "../components/ui";
import { useResource } from "../hooks/useResource";
import { ApiError } from "../api/client";
import {
  listLocales,
  upsertLocale,
  deleteLocale,
  type Locale,
} from "../api/locales";

export function Locales() {
  const locales = useResource(() => listLocales(), []);
  const [adding, setAdding] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  if (locales.loading) return <LoadingState />;
  if (locales.error)
    return (
      <EmptyState>
        {locales.error.message}{" "}
        <button className="rs-link-btn" onClick={locales.refetch}>Retry</button>
      </EmptyState>
    );

  const rows = locales.data ?? [];

  const onDelete = async (code: string) => {
    setNotice(null);
    try {
      await deleteLocale(code);
      locales.refetch();
    } catch (e) {
      setNotice(
        e instanceof ApiError ? e.message : "Couldn't delete locale.",
      );
    }
  };

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Locales</h1>
          <p className="rs-cm-sub">{rows.length} configured</p>
        </div>
        <button className="rs-btn rs-btn--primary" onClick={() => setAdding(true)}>
          <Icons.plus size={16} /> Add locale
        </button>
      </div>

      {notice && (
        <div style={{ margin: "0 24px" }}>
          <Notice>{notice}</Notice>
        </div>
      )}

      <div className="rs-table-wrap">
        <table className="rs-table">
          <thead>
            <tr>
              <th>Code</th>
              <th>Name</th>
              <th>Default</th>
              <th className="rs-col-act"></th>
            </tr>
          </thead>
          <tbody>
            {rows.map((l) => (
              <tr key={l.code}>
                <td className="rs-mono">{l.code}</td>
                <td>{l.name}</td>
                <td>
                  {l.is_default ? (
                    <span className="rs-status rs-status--ok">Default</span>
                  ) : (
                    <span className="rs-cell-muted">—</span>
                  )}
                </td>
                <td className="rs-col-act">
                  {!l.is_default && (
                    <button
                      className="rs-row-btn rs-danger"
                      title="Delete locale"
                      onClick={() => onDelete(l.code)}
                    >
                      <Icons.trash size={15} />
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {rows.length === 0 && <div className="rs-empty">No locales configured.</div>}
      </div>

      {adding && (
        <AddLocaleModal
          onClose={() => setAdding(false)}
          onSaved={() => {
            setAdding(false);
            locales.refetch();
          }}
        />
      )}
    </div>
  );
}

function AddLocaleModal({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved: (l: Locale) => void;
}) {
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [isDefault, setIsDefault] = useState(false);
  const [saving, setSaving] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const submit = async () => {
    setSaving(true);
    setErr(null);
    try {
      const l = await upsertLocale({ code: code.trim(), name: name.trim(), is_default: isDefault });
      onSaved(l);
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : "Couldn't add locale.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rs-modal-overlay" onClick={onClose}>
      <div className="rs-modal" onClick={(e) => e.stopPropagation()}>
        <div className="rs-modal-head">
          <div className="rs-modal-titles">
            <h2>Add locale</h2>
          </div>
          <button className="rs-icon-btn" onClick={onClose} aria-label="Close">
            <Icons.x size={16} />
          </button>
        </div>
        <div className="rs-modal-body">
          <div className="rs-field">
            <label className="rs-field-label">Code</label>
            <input
              className="rs-input"
              placeholder="fr, pt-br"
              value={code}
              onChange={(e) => setCode(e.target.value)}
            />
            <span className="rs-field-hint">Lowercase language tag, e.g. fr or pt-br.</span>
          </div>
          <div className="rs-field">
            <label className="rs-field-label">Name</label>
            <input
              className="rs-input"
              placeholder="French"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <label className="rs-check-row">
            <span
              className={"rs-toggle" + (isDefault ? " is-on" : "")}
              onClick={() => setIsDefault((v) => !v)}
            />
            <span>Set as default locale</span>
          </label>
          {err && <div className="rs-err-msg">{err}</div>}
        </div>
        <div className="rs-modal-foot">
          <button className="rs-btn rs-btn--ghost" onClick={onClose} disabled={saving}>
            Cancel
          </button>
          <button
            className="rs-btn rs-btn--primary"
            onClick={submit}
            disabled={saving || !code.trim() || !name.trim()}
          >
            {saving ? "Adding…" : "Add locale"}
          </button>
        </div>
      </div>
    </div>
  );
}
```

> If the `rs-check-row` / `rs-toggle` markup differs from how toggles are used elsewhere (check `ui/src/screens/UserEditor.tsx` or `WebhookEditor.tsx` for the exact toggle markup), match the existing usage. The toggle visual is `.rs-toggle.is-on` per DESIGN.md §Toggles.

- [ ] **Step 2: Add the route**

In `ui/src/App.tsx`: import `Locales` (`import { Locales } from "./screens/Locales";` with the other screen imports) and add a route next to the other `settings/*` routes:

```tsx
          <Route path="settings/locales" element={<Locales />} />
```

- [ ] **Step 3: Wire the nav item**

In `ui/src/components/shell.tsx` `SettingsPanel`, change the existing placeholder item

```tsx
        { label: "Internationalization" },
```

to:

```tsx
        { label: "Internationalization", to: "/settings/locales" },
```

(The panel already renders items with a `to` as enabled links; adding `to` activates it.)

- [ ] **Step 4: Verify typecheck + build**

Run: `cd ui && pnpm typecheck`
Expected: no errors.

- [ ] **Step 5: Manual check (note for the reviewer/user)**

Manual: navigate to Settings → Internationalization → see the seeded `en` row marked Default; "Add locale" opens the modal; adding `fr` shows it in the table; deleting `en` surfaces a notice (server 422); deleting `fr` removes it. (Backend must be running; agent may defer to user for browser verification.)

- [ ] **Step 6: Commit**

```bash
git add ui/src/screens/Locales.tsx ui/src/App.tsx ui/src/components/shell.tsx
git commit -m "feat(ui): Locales settings page + nav + route"
```

---

## Task 5: Content list — locale selector, Locale column, document_id links

**Files:**
- Modify: `ui/src/screens/ContentList.tsx`

- [ ] **Step 1: Add imports + localized detection + locale state**

In `ui/src/screens/ContentList.tsx`:

Add to the type imports from `../api/types`:

```typescript
import { draftPublishEnabled, localizedEnabled, relationMeta } from "../api/types";
```

Add a locales fetch + URL-driven locale state. After the existing `const allTypes = useResource(...)` line, add:

```typescript
import { listLocales } from "../api/locales";
```
(place with the other api imports at the top), and after `const dp = ...`:

```typescript
  const loc = ct ? localizedEnabled(ct) : false;
  const localesRes = useResource(() => (loc ? listLocales() : Promise.resolve([])), [loc]);
  const [searchParams, setSearchParams] = useSearchParams();
  const selectedLocale = searchParams.get("locale") ?? "";
```

Add `useSearchParams` to the existing `react-router-dom` import:

```typescript
import { Link, useLocation, useNavigate, useParams, useSearchParams } from "react-router-dom";
```

Seed the selected locale to the default once locales load (add an effect near the other effects):

```typescript
  useEffect(() => {
    if (!loc) return;
    if (selectedLocale) return;
    const def = localesRes.data?.find((l) => l.is_default) ?? localesRes.data?.[0];
    if (def) setSearchParams((p) => { p.set("locale", def.code); return p; }, { replace: true });
  }, [loc, selectedLocale, localesRes.data, setSearchParams]);
```

- [ ] **Step 2: Pass locale into the list query**

In the `listEntries(type, { ... })` options object inside the `entries` resource, add `locale`:

```typescript
        status: dp ? publishFilter : undefined,
        locale: loc ? selectedLocale || undefined : undefined,
        filters: JSON.parse(debouncedPairs) as [string, string][],
```

Add `selectedLocale` and `loc` to that `useResource` dependency array:

```typescript
    [type, populate, dp, publishFilter, debouncedPairs, page, pageSize, sort, loc, selectedLocale],
```

- [ ] **Step 3: Render the locale selector in the toolbar**

In the `rs-cm-toolbar` block, after the sort button and before `<div className="rs-spacer" />`, add (only when localized):

```tsx
        {loc && localesRes.data && localesRes.data.length > 0 && (
          <select
            className="rs-input rs-input--sm"
            value={selectedLocale}
            onChange={(e) =>
              setSearchParams((p) => { p.set("locale", e.target.value); return p; })
            }
            aria-label="Locale"
          >
            {localesRes.data.map((l) => (
              <option key={l.code} value={l.code}>
                {l.code} — {l.name}
              </option>
            ))}
          </select>
        )}
```

- [ ] **Step 4: Add the Locale column + fallback hint**

In the `<thead>` row, after the `{dp && <th>Status</th>}` line, add:

```tsx
              {loc && <th>Locale</th>}
```

In the `<tbody>` row mapping, after the `{dp && (<td>…</td>)}` block and before `{cols.map(...)}`, add:

```tsx
                {loc && (
                  <td className="rs-mono">
                    {String(e.locale ?? "")}
                    {e.locale && selectedLocale && e.locale !== selectedLocale && (
                      <span className="rs-cell-muted"> (fallback)</span>
                    )}
                  </td>
                )}
```

- [ ] **Step 5: Link rows by document_id for localized types**

Compute the editor target per row. Add a helper near the top of the component body (after `rows` is defined, before the return) — but since `rows` is used in JSX, add this inline helper above the `return`:

```typescript
  const editorPath = (e: Entry): string => {
    if (loc && e.document_id) {
      const q = selectedLocale ? `?locale=${encodeURIComponent(selectedLocale)}` : "";
      return `/content/${type}/${e.document_id}${q}`;
    }
    return `/content/${type}/${e.id}`;
  };
```

Change the row `onClick` from:

```tsx
                onClick={() => navigate(`/content/${type}/${e.id}`)}
```

to:

```tsx
                onClick={() => navigate(editorPath(e))}
```

> The flash-link and "Create new entry" button keep using `id`/`new` — they're not per-locale-row navigations. Leave them. (Create-new for a localized type lands on `/content/:type/new`; Task 6 handles seeding its locale from `?locale=` if present — but the create button doesn't pass one, so new entries default to the default locale, which is correct.)

- [ ] **Step 6: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: no errors.

- [ ] **Step 7: Manual check (note)**

Manual: open a localized type's list → locale dropdown appears, seeded to default; Locale column shows each row's code; switching locale refetches; a document with no translation in the selected locale shows its default row with "(fallback)"; clicking a row opens the editor at `/content/<type>/<documentId>?locale=<code>`. Non-localized types: no dropdown, no Locale column, links unchanged.

- [ ] **Step 8: Commit**

```bash
git add ui/src/screens/ContentList.tsx
git commit -m "feat(ui): content list locale selector, Locale column, document_id links"
```

---

## Task 6: Entry editor — locale switcher + translation authoring

**Files:**
- Modify: `ui/src/screens/EntryEditor.tsx`

- [ ] **Step 1: Read locale from the URL + detect localized + load per locale**

In `ui/src/screens/EntryEditor.tsx`:

Add imports:

```typescript
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { localizedEnabled } from "../api/types";
import { listLocales, type Locale } from "../api/locales";
```

(merge `useSearchParams` into the existing `react-router-dom` import; add `localizedEnabled` to the existing `../api/types` import line which currently imports `draftPublishEnabled, coerceFieldValue`).

After the existing `const schema = useResource(...)` and before `existing`, add locale plumbing:

```typescript
  const [searchParams, setSearchParams] = useSearchParams();
  const loc = schema.data ? localizedEnabled(schema.data) : false;
  const localesRes = useResource(() => (loc ? listLocales() : Promise.resolve([] as Locale[])), [loc]);
  const requestedLocale = searchParams.get("locale") ?? "";
```

Change the `existing` resource so localized loads pass the locale (note: `id` is the document_id for localized types). Replace:

```typescript
  const existing = useResource(
    () => (isNew ? Promise.resolve(null) : getEntry(type, id)),
    [type, id, isNew],
  );
```

with:

```typescript
  const existing = useResource(
    () =>
      isNew
        ? Promise.resolve(null)
        : getEntry(type, id, loc ? { locale: requestedLocale || undefined } : {}),
    [type, id, isNew, loc, requestedLocale],
  );
```

- [ ] **Step 2: Derive the served locale + missing-translation state**

After the guards (`if (!ct) return ...`), add:

```typescript
  // For localized types: the locale actually served (may differ from requested
  // when the backend fell back to the default-locale row).
  const servedLocale = (existing.data?.locale as string | undefined) ?? requestedLocale;
  // A translation for the requested locale is "missing" when the server served
  // a different locale (fallback) — the user is about to create a new one.
  const missingTranslation =
    loc && !isNew && requestedLocale !== "" && servedLocale !== requestedLocale;
```

- [ ] **Step 3: Add the locale dropdown to the EditorBar**

Build a status-dotted dropdown. Add, before the `return`:

```typescript
  const switchLocale = (code: string) =>
    setSearchParams((p) => { p.set("locale", code); return p; });

  const localeSwitcher =
    loc && localesRes.data && localesRes.data.length > 0 ? (
      <select
        className="rs-input rs-input--sm"
        value={requestedLocale || servedLocale}
        onChange={(e) => switchLocale(e.target.value)}
        aria-label="Locale"
      >
        {localesRes.data.map((l) => (
          <option key={l.code} value={l.code}>
            {l.code} — {l.name}
          </option>
        ))}
      </select>
    ) : null;
```

In the `EditorBar`'s `status` prop, combine the existing status badge with the switcher. Replace the current `status={...}` with:

```tsx
        status={
          <>
            {dp && !isNew && <StatusBadge status={isPublished ? "published" : "draft"} />}
            {localeSwitcher}
          </>
        }
```

(Remove the now-duplicated `dp && !isNew` status expression that was previously the sole `status` value.)

- [ ] **Step 4: Missing-translation banner + create action**

When `missingTranslation`, the loaded form holds the fallback row's values — clear them so the translator starts empty, and show a create action.

Add an effect to blank the form when a translation is missing (after the existing form-seed effect):

```typescript
  useEffect(() => {
    if (missingTranslation && schema.data) {
      const blank: Record<string, unknown> = {};
      for (const f of schema.data.fields) blank[f.name] = "";
      setForm(blank);
    }
  }, [missingTranslation, schema.data]);
```

Add a create-translation handler (next to `save`):

```typescript
  const createTranslation = async () => {
    if (!ct) return;
    setSaving(true);
    setFieldErrors({});
    setBanner(null);
    const body: Record<string, unknown> = { document_id: id };
    for (const f of ct.fields) {
      const v = form[f.name];
      if (v === "" || v === undefined) continue;
      if (f.kind === "integer" || f.kind === "float") body[f.name] = Number(v);
      else body[f.name] = v;
    }
    try {
      await createEntry(type, body, { locale: requestedLocale });
      navigate(`/content/${type}?locale=${encodeURIComponent(requestedLocale)}`, {
        state: { flash: "created", flashId: id },
      });
    } catch (e) {
      setBanner(e instanceof ApiError ? e.message : "Couldn't create translation.");
    } finally {
      setSaving(false);
    }
  };
```

> This is a simplified body builder (scalars + numbers). Complex field kinds (json/component/media) for a brand-new translation are handled by the normal `save` path once the translation row exists; the create-translation action seeds the row, then the user edits normally. Keeping the create body minimal avoids duplicating the full coercion logic. If the type has required non-scalar fields, the backend 422s and the banner surfaces it — acceptable for v1.

Add a banner + swap the primary action when `missingTranslation`. After the existing `{banner && ...}` block, add:

```tsx
      {missingTranslation && (
        <div style={{ margin: "0 24px" }}>
          <Notice>No translation for “{requestedLocale}” yet. Fill the fields and create one.</Notice>
        </div>
      )}
```

In the `EditorBar` `actions`, when `missingTranslation`, show a single primary "Create translation" instead of Save/Publish. Wrap the existing actions:

```tsx
        actions={
          missingTranslation ? (
            <button className="rs-btn rs-btn--primary" onClick={createTranslation} disabled={saving}>
              {saving ? "Creating…" : `Create ${requestedLocale} translation`}
            </button>
          ) : (
            <>
              {/* existing publish + save buttons unchanged */}
            </>
          )
        }
```

(Move the current `actions` JSX into the `: ( ... )` branch verbatim.)

- [ ] **Step 5: Locale-scope the save + publish calls**

In the existing `save` function, the update call must pass the locale for localized types. Change:

```typescript
        await updateEntry(type, id, body);
```

to:

```typescript
        await updateEntry(type, id, body, loc ? { locale: requestedLocale || undefined } : {});
```

And the create (new-entry) path — pass the locale so a brand-new localized entry is created in the active locale:

```typescript
        const created = await createEntry(type, body, loc ? { locale: requestedLocale || undefined } : {});
```

(publish/unpublish in `togglePublish` are keyed by the loaded row and the backend ignores `?locale` for publish — leave those calls as-is.)

- [ ] **Step 6: Verify typecheck**

Run: `cd ui && pnpm typecheck`
Expected: no errors.

- [ ] **Step 7: Manual check (note)**

Manual: from a localized list, open a row → editor loads that locale, dropdown shows it; switching to a translated locale reloads its values; switching to an untranslated locale blanks the form, shows the "No translation yet" notice and a "Create <code> translation" primary; creating it returns to the list; editing an existing translation saves locale-scoped. Non-localized editor: unchanged (no dropdown, no notice).

- [ ] **Step 8: Commit**

```bash
git add ui/src/screens/EntryEditor.tsx
git commit -m "feat(ui): entry editor locale switcher + translation authoring"
```

---

## Task 7: Final verification

**Files:** none (verification only).

- [ ] **Step 1: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: clean (no errors).

- [ ] **Step 2: Build**

Run: `cd ui && pnpm build`
Expected: builds to `ui/dist` with no type or bundler errors.

- [ ] **Step 3: Lint sweep (if the project lints)**

Run: `cd ui && pnpm lint` (only if a `lint` script exists in `ui/package.json`; skip otherwise).
Expected: clean, or fix any new warnings in the touched files.

- [ ] **Step 4: Report for manual browser verification**

Summarize the manual checks from Tasks 4–6 for the user to run against a live backend (locales page CRUD; list locale selector + column + fallback; editor switch + create-translation). The agent does not have dev admin creds and will not seed the user's dev DB.

---

## Self-Review notes (addressed)

- **Spec coverage:** API layer (T1–T3), Locales settings page + nav + route (T4), list locale selector + Locale column + fallback hint + document_id links (T5), editor locale dropdown with status + document_id addressing + missing-translation create flow + fallback awareness + locale-scoped save (T6), states (loading/empty/error/missing-translation/fallback covered across T4–T6), verification (T7). Out-of-scope items (copy-from-default, publish-by-locale UI, GraphQL playground, bulk translate) correctly absent.
- **Type consistency:** `localizedEnabled` (T1) used in T5/T6; `Locale`/`listLocales`/`upsertLocale`/`deleteLocale` (T3) used in T4/T5/T6; `getEntry`/`createEntry`/`updateEntry` opts `{locale?}` (T2) used in T6; `Entry.document_id?/locale?` + `meta.locale?` (T1) used in T5/T6. Consistent.
- **No automated UI tests** by design (no infra); per-task verification is `pnpm typecheck`, final `pnpm build`, plus manual browser checks flagged for the user.
- **Non-localized safety:** every localized branch gated on `localizedEnabled(ct)`; endpoint `locale` params optional and omitted by existing callers → non-localized behavior unchanged.
