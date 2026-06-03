# Media Provider Settings UI — Design

**Date:** 2026-06-04
**Scope:** Admin UI page to view and configure the active media storage provider,
wired to the existing backend settings/providers API (shipped 2026-06-03).
No backend change.
**Status:** Approved (brainstorm)

## Goal

A settings screen where an admin selects the active media storage provider
(local filesystem or S3), edits its configuration, tests the connection, and
saves. The form is **schema-driven** from the backend's self-describing provider
descriptors, so adding a future provider requires zero UI changes. Secret fields
(e.g. S3 secret key) are never shown decrypted and use "leave blank to keep"
semantics that match the backend's mask handling.

## Backend Contract (already shipped — no change)

- `GET /admin/media/providers` → `ProviderDescriptor[]`:
  `{ id, label, fields: { name, label, type, required, secret }[] }`
  (`type` is `"string"` for all current fields).
- `GET /admin/media/settings` → `{ provider, config } | null`. `null` when no
  settings row exists (fresh DB). Secret fields in `config` are masked as `"••••"`.
- `PUT /admin/media/settings` ← `{ provider, config }`. If a secret field equals
  `"••••"` (or is absent), the backend keeps the previously-stored encrypted value;
  otherwise it encrypts the new value. Validates config against the descriptor
  (422 with field errors on failure). Hot-swaps the active provider on success.
- `POST /admin/media/settings/test` ← `{ provider, config }` → 200 on success,
  4xx with a message on failure. Masked secrets are filled from stored values.

The mask constant is `"••••"` (matches backend `MASK`).

## Route & Entry Points

- New route `/settings/media` → `MediaSettings` screen (in `ui/src/App.tsx`,
  alongside the existing `/settings` tokens route, which is unchanged).
- A gear icon button on the Media Library header (`MediaLibrary.tsx`) links to
  `/settings/media` (uses `useNavigate`). Uses the existing `gear` icon.

## Frontend Files

- **`ui/src/api/types.ts`** — add:
  - `MediaProviderField { name; label; type: string; required: boolean; secret: boolean }`
  - `MediaProviderDescriptor { id: string; label: string; fields: MediaProviderField[] }`
  - `MediaSettings { provider: string; config: Record<string, string> }`
- **`ui/src/api/endpoints.ts`** — add:
  - `listMediaProviders(): Promise<MediaProviderDescriptor[]>` → `GET /admin/media/providers`
  - `getMediaSettings(): Promise<MediaSettings | null>` → `GET /admin/media/settings`
  - `putMediaSettings(body: MediaSettings): Promise<void>` → `PUT /admin/media/settings`
  - `testMediaSettings(body: MediaSettings): Promise<void>` → `POST /admin/media/settings/test`
- **`ui/src/screens/MediaSettings.tsx`** — the screen (load, provider select,
  test, save, status banners).
- **`ui/src/screens/media/ProviderForm.tsx`** — renders a descriptor's fields as
  inputs bound to a `config` value map; reports changes up.
- **`ui/src/App.tsx`** — add the `/settings/media` route.
- **`ui/src/screens/MediaLibrary.tsx`** — add the gear link to the header actions.
- **`ui/src/styles.css`** — minimal additions only if needed (reuse `rs-field(s)`,
  `rs-input`, `rs-btn`, `rs-login-error`, `rs-cm`/`rs-cm-head`); add a status-banner
  success style if one doesn't exist.

## Data Flow & State

`MediaSettings` owns:
- `providers: MediaProviderDescriptor[]` (from `listMediaProviders`)
- `provider: string` — selected provider id
- `config: Record<string, string>` — current field values for the selected provider
- `stored: MediaSettings | null` — the loaded settings (to know which provider is
  active and to prefill)
- `status: { kind: "idle" | "testing" | "saving" | "ok" | "error"; message?: string }`
- `fieldErrors: Record<string, string>` — per-field validation from a 422

On mount: `Promise.all([listMediaProviders(), getMediaSettings()])`.
- Initial `provider` = `stored?.provider ?? "local"` (default local; the descriptor
  list always contains `local`).
- Initial `config` = if `stored` and `stored.provider === provider`, use
  `stored.config` (secrets arrive masked as `"••••"`); else `{}` (empty fields).

Changing the provider `<select>`:
- Set `provider`; recompute `config` = (stored matches new provider ? stored.config
  : `{}`). Clear `fieldErrors` and reset `status` to idle.

`ProviderForm` renders the selected descriptor's fields:
- `secret: true` → `<input type="password">`, rendered EMPTY, placeholder
  `"•••• (leave blank to keep)"`. The form tracks whether the user typed; a blank
  secret field means "unchanged".
- non-secret `string` → `<input type="text">` bound to `config[name]`.
- `required` → label asterisk; on submit, required non-secret fields must be
  non-empty (client check) — secrets are exempt (blank = keep existing).

## Building the Request Body

A shared `buildBody()` produces `{ provider, config }` for both Test and Save:
- For each descriptor field of the selected provider:
  - secret field: if the user left it blank → send `"••••"` (backend keeps stored
    value); if typed → send the typed value.
  - non-secret: send `config[name] ?? ""`.
- Result `config` includes exactly the descriptor's field names.

This makes Test and Save behave identically with respect to masked secrets, so a
user can test an already-saved S3 config without re-entering the secret.

## Actions

- **Test connection** (`testMediaSettings(buildBody())`): set `status=testing`;
  on success `status=ok, "Connection OK"`; on `ApiError` `status=error, e.message`.
- **Save** (`putMediaSettings(buildBody())`): client-validate required non-secret
  fields first (populate `fieldErrors`, abort if any). Set `status=saving`; on
  success → refetch `getMediaSettings()` (re-mask secrets), set `stored`, reset the
  form to the refetched values, `status=ok, "Settings saved"`. On `ApiError`: if
  422 with `fieldErrors`, map them onto inputs; else `status=error, e.message`.

## Error Handling & Edge Cases

- `getMediaSettings()` returns `null` (fresh DB) → default to `local`, empty config.
- 422 on save → field-level errors shown under the relevant inputs.
- Test/save transport or 5xx errors → inline error banner with the message.
- 401 → existing global auth handler (redirect to login); no change.
- Switching providers clears stale errors and status.
- Unknown provider id in `stored` (shouldn't happen) → fall back to `local`.

## Out of Scope

- Env-override detection UI (backend `GET /settings` does not signal whether an
  env override is active; env override remains an ops concern). The page shows the
  stored/default DB config.
- Validation beyond "required string present" (backend is the source of truth and
  returns 422s the UI surfaces).
- Reworking the existing `/settings` API-tokens preview.
- Automated UI tests (no harness in `ui/`).

## Testing

- No backend change → no new Rust tests.
- UI: `cd ui && pnpm typecheck` and `pnpm build` must pass.
- Manual against a running backend: open `/settings/media`; with no settings, see
  `local` selected with empty fields; switch to S3 → S3 fields render; fill bucket/
  region/keys; Test connection (expect failure without a real bucket, with a clear
  message; or success against MinIO); Save → "Settings saved"; reload → S3 selected,
  secret field empty with the "leave blank to keep" placeholder; switch back to
  local and Save; confirm the gear icon on the Media Library header opens the page.
