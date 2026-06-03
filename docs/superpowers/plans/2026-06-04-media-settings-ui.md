# Media Provider Settings UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `/settings/media` screen to select, configure, test, and save the active media storage provider, driven by the backend's self-describing provider descriptors. No backend change.

**Architecture:** Add media-settings types + 4 endpoint functions to the API layer, a schema-driven `ProviderForm`, a `MediaSettings` screen, a route, and a gear entry-point on the Media Library header. The form renders fields from each provider descriptor; secret fields use "leave blank to keep" semantics matching the backend's `••••` mask. Test and Save share one request builder.

**Tech Stack:** TypeScript, React 18, React Router, Vite. No test harness in `ui/` — verify with `pnpm typecheck` + `pnpm build` + manual.

**Reference files (read before starting):**
- Endpoints pattern: `ui/src/api/endpoints.ts` (already has media fns like `listFolders`, `uploadAsset`). Client: `ui/src/api/client.ts` (`apiFetch`, `ApiError`). Types: `ui/src/api/types.ts`.
- Existing form/field markup: `ui/src/screens/media/FolderModal.tsx` and `ui/src/builder/CreateTypeModal.tsx` (use `rs-field`, `rs-field-label` + `<label>`, `rs-field-hint`, `rs-input`, `rs-login-error`).
- Screen shell pattern: `ui/src/screens/Settings.tsx` / `ui/src/screens/MediaLibrary.tsx` (`rs-cm`, `rs-cm-head` with `<h1>` + `rs-cm-sub`, `rs-editor-actions`, `rs-btn`).
- Routing: `ui/src/App.tsx` (routes under the authed layout; `/settings` already present).
- Icons: `ui/src/components/icons.tsx` (has `gear`, `check`, `bolt`; uses `Icons.<name> size=`).

**Backend contract (already shipped, do NOT change):**
- `GET /admin/media/providers` → `{ id, label, fields: { name, label, type, required, secret }[] }[]`.
- `GET /admin/media/settings` → `{ provider, config } | null` (secrets masked as `"••••"`).
- `PUT /admin/media/settings` ← `{ provider, config }` (secret `"••••"` = keep stored; else encrypt; 422 with `details.fields` on validation error).
- `POST /admin/media/settings/test` ← `{ provider, config }` → 200 ok / 4xx with message.
- Mask string is exactly `"••••"`.

---

## File Structure

- Modify: `ui/src/api/types.ts` (Task 1) — settings types.
- Modify: `ui/src/api/endpoints.ts` (Task 2) — 4 endpoint fns.
- Create: `ui/src/screens/media/ProviderForm.tsx` (Task 3) — schema-driven field renderer.
- Create: `ui/src/screens/MediaSettings.tsx` (Task 4) — screen (load/select/test/save).
- Modify: `ui/src/App.tsx` (Task 5) — `/settings/media` route.
- Modify: `ui/src/screens/MediaLibrary.tsx` (Task 5) — gear entry link.
- Modify: `ui/src/styles.css` (Task 4, only if needed) — success banner / minor layout.

---

## Task 1: Settings types

**Files:**
- Modify: `ui/src/api/types.ts`

- [ ] **Step 1: Append the types**

```typescript
export interface MediaProviderField {
  name: string;
  label: string;
  type: string; // "string"
  required: boolean;
  secret: boolean;
}

export interface MediaProviderDescriptor {
  id: string;
  label: string;
  fields: MediaProviderField[];
}

export interface MediaSettings {
  provider: string;
  config: Record<string, string>;
}
```

- [ ] **Step 2: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add ui/src/api/types.ts
git commit -m "feat(media-settings): provider descriptor + settings types"
```

---

## Task 2: Settings endpoints

**Files:**
- Modify: `ui/src/api/endpoints.ts`

- [ ] **Step 1: Add the type imports**

In the `import type { ... } from "./types";` block, add `MediaProviderDescriptor, MediaSettings`.

- [ ] **Step 2: Append the endpoint functions**

```typescript
export function listMediaProviders(): Promise<MediaProviderDescriptor[]> {
  return apiFetch<MediaProviderDescriptor[]>("/admin/media/providers");
}

export function getMediaSettings(): Promise<MediaSettings | null> {
  return apiFetch<MediaSettings | null>("/admin/media/settings");
}

export function putMediaSettings(body: MediaSettings): Promise<void> {
  return apiFetch<void>("/admin/media/settings", { method: "PUT", body });
}

export function testMediaSettings(body: MediaSettings): Promise<void> {
  return apiFetch<void>("/admin/media/settings/test", { method: "POST", body });
}
```

- [ ] **Step 3: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/api/endpoints.ts
git commit -m "feat(media-settings): providers + settings endpoint functions"
```

---

## Task 3: ProviderForm (schema-driven fields)

**Files:**
- Create: `ui/src/screens/media/ProviderForm.tsx`

- [ ] **Step 1: Create the component**

`ui/src/screens/media/ProviderForm.tsx`:

```tsx
import type { MediaProviderDescriptor } from "../../api/types";

const MASK = "••••";

export function ProviderForm({
  descriptor, values, onChange, fieldErrors,
}: {
  descriptor: MediaProviderDescriptor;
  values: Record<string, string>;
  onChange: (name: string, value: string) => void;
  fieldErrors: Record<string, string>;
}) {
  return (
    <div className="rs-fields">
      {descriptor.fields.map((f) => {
        const err = fieldErrors[f.name];
        // For secret fields, the input is rendered empty; the stored value is
        // masked server-side and kept on save unless the user types a new value.
        const isSecret = f.secret;
        const shown = isSecret ? (values[f.name] === MASK ? "" : values[f.name] ?? "") : (values[f.name] ?? "");
        return (
          <div className="rs-field" key={f.name}>
            <div className="rs-field-label">
              <label>{f.label}{f.required ? " *" : ""}</label>
              {isSecret && <span className="rs-field-hint">Leave blank to keep current</span>}
            </div>
            <input
              className="rs-input"
              type={isSecret ? "password" : "text"}
              value={shown}
              placeholder={isSecret ? "•••• (leave blank to keep)" : ""}
              autoComplete={isSecret ? "new-password" : "off"}
              onChange={(e) => onChange(f.name, e.target.value)}
            />
            {err && <span className="rs-field-error">{err}</span>}
          </div>
        );
      })}
    </div>
  );
}
```

> NOTE: `.rs-field-error` may not exist in `ui/src/styles.css`. Check; if missing, add a minimal rule in Task 4's styles step (small red text). For now the component references it; it renders harmlessly even unstyled.

- [ ] **Step 2: Typecheck**

Run: `cd ui && pnpm typecheck`
Expected: PASS (unused until Task 4 imports it — that's fine).

- [ ] **Step 3: Commit**

```bash
git add ui/src/screens/media/ProviderForm.tsx
git commit -m "feat(media-settings): schema-driven provider form"
```

---

## Task 4: MediaSettings screen

**Files:**
- Create: `ui/src/screens/MediaSettings.tsx`
- Modify: `ui/src/styles.css` (only if `.rs-field-error` / success banner missing)

- [ ] **Step 1: Create the screen**

`ui/src/screens/MediaSettings.tsx`:

```tsx
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import { ApiError } from "../api/client";
import {
  listMediaProviders, getMediaSettings, putMediaSettings, testMediaSettings,
} from "../api/endpoints";
import type { MediaProviderDescriptor, MediaSettings as MediaSettingsT } from "../api/types";
import { ProviderForm } from "./media/ProviderForm";

const MASK = "••••";
type Status = { kind: "idle" | "testing" | "saving" | "ok" | "error"; message?: string };

export function MediaSettings() {
  const navigate = useNavigate();
  const [providers, setProviders] = useState<MediaProviderDescriptor[]>([]);
  const [stored, setStored] = useState<MediaSettingsT | null>(null);
  const [provider, setProvider] = useState<string>("local");
  const [config, setConfig] = useState<Record<string, string>>({});
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([listMediaProviders(), getMediaSettings()])
      .then(([provs, settings]) => {
        setProviders(provs);
        setStored(settings);
        const initial = settings?.provider ?? "local";
        setProvider(initial);
        setConfig(settings && settings.provider === initial ? { ...settings.config } : {});
      })
      .catch((e) => setStatus({ kind: "error", message: e instanceof Error ? e.message : "Failed to load." }))
      .finally(() => setLoading(false));
  }, []);

  const descriptor = useMemo(
    () => providers.find((p) => p.id === provider),
    [providers, provider],
  );

  const selectProvider = (id: string) => {
    setProvider(id);
    setConfig(stored && stored.provider === id ? { ...stored.config } : {});
    setFieldErrors({});
    setStatus({ kind: "idle" });
  };

  const setField = (name: string, value: string) => {
    setConfig((c) => ({ ...c, [name]: value }));
    setFieldErrors((fe) => { const next = { ...fe }; delete next[name]; return next; });
  };

  // Build the request body: blank secret → send MASK (keep stored); else value.
  const buildBody = (): MediaSettingsT => {
    const out: Record<string, string> = {};
    for (const f of descriptor?.fields ?? []) {
      if (f.secret) {
        const v = config[f.name];
        out[f.name] = v && v !== "" ? v : MASK;
      } else {
        out[f.name] = config[f.name] ?? "";
      }
    }
    return { provider, config: out };
  };

  const validateRequired = (): boolean => {
    const errs: Record<string, string> = {};
    for (const f of descriptor?.fields ?? []) {
      if (f.required && !f.secret && !(config[f.name] ?? "").trim()) {
        errs[f.name] = "Required";
      }
    }
    setFieldErrors(errs);
    return Object.keys(errs).length === 0;
  };

  const onTest = async () => {
    setStatus({ kind: "testing" });
    try {
      await testMediaSettings(buildBody());
      setStatus({ kind: "ok", message: "Connection OK" });
    } catch (e) {
      setStatus({ kind: "error", message: e instanceof Error ? e.message : "Connection failed." });
    }
  };

  const onSave = async () => {
    if (!validateRequired()) { setStatus({ kind: "idle" }); return; }
    setStatus({ kind: "saving" });
    try {
      await putMediaSettings(buildBody());
      const fresh = await getMediaSettings();
      setStored(fresh);
      if (fresh) setConfig({ ...fresh.config });
      setStatus({ kind: "ok", message: "Settings saved" });
    } catch (e) {
      if (e instanceof ApiError && e.fieldErrors.length) {
        const errs: Record<string, string> = {};
        for (const fe of e.fieldErrors) errs[fe.field] = fe.message ?? "Invalid";
        setFieldErrors(errs);
        setStatus({ kind: "error", message: "Please fix the highlighted fields." });
      } else {
        setStatus({ kind: "error", message: e instanceof Error ? e.message : "Could not save." });
      }
    }
  };

  const busy = status.kind === "testing" || status.kind === "saving";
  const activeLabel = providers.find((p) => p.id === stored?.provider)?.label;

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Media Storage</h1>
          <p className="rs-cm-sub">
            {loading ? "Loading…"
              : stored ? `Active provider: ${activeLabel ?? stored.provider}`
              : "No provider configured — defaults to local filesystem."}
          </p>
        </div>
        <button className="rs-btn rs-btn--ghost" type="button" onClick={() => navigate("/media")}>
          <Icons.arrowLeft size={16} /> Back to Media
        </button>
      </div>

      {!loading && (
        <div className="rs-settings-card">
          <div className="rs-field">
            <div className="rs-field-label"><label>Storage provider</label></div>
            <select className="rs-input" value={provider} onChange={(e) => selectProvider(e.target.value)}>
              {providers.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
            </select>
          </div>

          {descriptor && (
            <ProviderForm descriptor={descriptor} values={config} onChange={setField} fieldErrors={fieldErrors} />
          )}

          {status.kind === "ok" && <div className="rs-settings-ok">{status.message}</div>}
          {status.kind === "error" && <div className="rs-login-error">{status.message}</div>}

          <div className="rs-editor-actions" style={{ marginTop: 16 }}>
            <button className="rs-btn rs-btn--ghost" type="button" disabled={busy} onClick={onTest}>
              <Icons.bolt size={15} /> {status.kind === "testing" ? "Testing…" : "Test connection"}
            </button>
            <div className="rs-spacer" />
            <button className="rs-btn rs-btn--primary" type="button" disabled={busy} onClick={onSave}>
              <Icons.check size={15} /> {status.kind === "saving" ? "Saving…" : "Save settings"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
```

> Uses `Icons.arrowLeft`, `Icons.bolt`, `Icons.check` — all present in `icons.tsx`. If `bolt` is somehow absent, substitute `gear`.

- [ ] **Step 2: Add the small CSS rules if missing**

Check `ui/src/styles.css` for `.rs-field-error`, `.rs-settings-ok`, `.rs-settings-card`. Append any that are missing under a `/* Media settings */` comment, consistent with existing tokens:

```css
.rs-settings-card { max-width: 560px; background: var(--surface); border: 1px solid var(--border); border-radius: 12px; padding: 20px; }
.rs-field-error { display: block; margin-top: 4px; font-size: 12px; color: var(--danger, #c0392b); }
.rs-settings-ok { margin-top: 4px; padding: 8px 12px; border-radius: 8px; font-size: 13px; color: #1a7f4b; background: color-mix(in srgb, #1a7f4b 12%, transparent); }
```

> Only add the rules that don't already exist. If `--danger` / `--surface` / `--border` variables aren't defined, use the nearest existing variable (grep the file's `:root`).

- [ ] **Step 3: Typecheck + build**

Run: `cd ui && pnpm typecheck && pnpm build`
Expected: both PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/screens/MediaSettings.tsx ui/src/styles.css
git commit -m "feat(media-settings): media storage settings screen"
```

---

## Task 5: Route + Media Library entry point

**Files:**
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/screens/MediaLibrary.tsx`

- [ ] **Step 1: Add the route**

In `ui/src/App.tsx`: add the import near the other screen imports:

```tsx
import { MediaSettings } from "./screens/MediaSettings";
```

Add the route alongside the existing `/settings` route (inside the same authed `<Route>` group; match the surrounding style):

```tsx
<Route path="settings/media" element={<MediaSettings />} />
```

> Confirm the existing routes use relative paths (e.g. `path="settings"`); add `path="settings/media"` in the same group so it inherits the layout + auth guard.

- [ ] **Step 2: Add the gear entry on the Media Library header**

In `ui/src/screens/MediaLibrary.tsx`, the header actions block is:

```tsx
        <div className="rs-editor-actions">
          <button className="rs-btn rs-btn--ghost" onClick={() => setModal("folder")} type="button"><Icons.folderPlus size={16} /> Add new folder</button>
          <button className="rs-btn rs-btn--primary" onClick={() => setModal("upload")} type="button"><Icons.upload size={16} /> Add new assets</button>
        </div>
```

Add `useNavigate` and a gear button. At the top of the file, ensure the import exists:

```tsx
import { useNavigate } from "react-router-dom";
```

Inside the component, near the other hooks:

```tsx
  const navigate = useNavigate();
```

Add the gear button as the FIRST child of the `rs-editor-actions` div in the header:

```tsx
          <button className="rs-btn rs-btn--ghost" title="Media storage settings" onClick={() => navigate("/settings/media")} type="button"><Icons.gear size={16} /> Settings</button>
```

> If `useNavigate` is already imported / `navigate` already defined in this file, don't duplicate. (As of now MediaLibrary does not use the router, so both additions are needed.)

- [ ] **Step 3: Typecheck + build**

Run: `cd ui && pnpm typecheck && pnpm build`
Expected: both PASS.

- [ ] **Step 4: Manual verification (against running backend)**

Start backend + `cd ui && pnpm dev`, log in:
- Media Library header now has a "Settings" gear → click → lands on `/settings/media`.
- With no settings: heading says "defaults to local filesystem", provider = Local Filesystem, fields empty.
- Switch provider to Amazon S3 → S3 fields render (bucket, region, endpoint, access key, secret key as password with "leave blank to keep").
- Fill bucket/region/keys → "Test connection" → expect a clear error without a real bucket (or success against MinIO).
- "Save settings" → "Settings saved"; reload → S3 selected, secret field empty with placeholder.
- Switch back to Local, fill base_dir, Save → succeeds.
- Leaving a required non-secret field blank on Save → field shows "Required".

Record results.

- [ ] **Step 5: Commit**

```bash
git add ui/src/App.tsx ui/src/screens/MediaLibrary.tsx
git commit -m "feat(media-settings): route + Media Library settings entry"
```

---

## Self-Review Notes (addressed)

- **Spec coverage:** types (T1); 4 endpoints (T2); schema-driven form w/ secret "leave blank to keep" (T3); screen with load/null-default/provider-switch/test/save/422-mapping/refetch-remask (T4); route `/settings/media` + gear entry (T5). All spec sections mapped. Out-of-scope items (env-detection, tokens rework, UI tests) correctly omitted.
- **Type consistency:** `MediaProviderDescriptor`/`MediaProviderField`/`MediaSettings` defined T1, used T2–T4. Endpoint fns `listMediaProviders`/`getMediaSettings`/`putMediaSettings`/`testMediaSettings` defined T2, used T4. `ProviderForm` props (`descriptor`, `values`, `onChange`, `fieldErrors`) defined T3, used T4. `MASK = "••••"` consistent T3/T4 and with backend. `ApiError.fieldErrors` (`{field, message?}`) matches `client.ts`.
- **Placeholders:** none — all code complete. CSS step guards against duplicating existing rules; icon fallbacks noted.
- **Testing posture:** no backend change → no Rust tests; UI verified via typecheck + build + manual checklist (consistent with the repo's UI approach).
- **Deviation note:** "Back to Media" uses `Icons.arrowLeft`; gear entry uses `Icons.gear` — both confirmed present in `icons.tsx`.
