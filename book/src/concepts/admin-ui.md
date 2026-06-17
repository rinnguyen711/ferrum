# Admin UI architecture

The admin UI (codenamed *Studio*) is a single-page React app that talks to the
same HTTP API you use from your own code. It has no privileged backdoor: every
list, editor, and save goes through the public `/api` and `/admin` routes with a
bearer token, exactly like an external client. If you can do it in the UI, you
can do it over the API — and vice versa.

This page explains how the app is put together so you can find your way around
the source before you [add a custom field widget](../guides/custom-field-widget.md)
or [build and embed it](../guides/build-admin-ui.md).

## Stack

The UI lives in `ui/` and is built with:

- **React 18** with **TypeScript**, bundled by **Vite**.
- **react-router-dom v7** for client-side routing.
- **Tiptap** for the rich-text field editor.
- **lucide-react** for icons.
- **pnpm** as the package manager.

There is no global state library and no data-fetching framework. State is local
React state; fetching goes through a small typed client and one hook (see
below).

## Where it runs

In development you run the Vite dev server and the API separately:

```sh
cd ui
pnpm dev      # http://localhost:5173
```

The dev server proxies `/api`, `/admin`, and `/healthz` to the API on `:8080`,
so the SPA at `http://localhost:5173/` calls a real backend without CORS setup.

In production the app is a static bundle (`ui/dist`) that the server mounts at
`/studio`. See [Build & embed the admin UI](../guides/build-admin-ui.md) for how
that wiring works.

## Layer map

The source under `ui/src/` is organized as four layers. Data flows up; nothing
lower reaches into a screen.

| Layer | Directory | Responsibility |
|---|---|---|
| API client | `api/` | Typed `fetch` wrapper, endpoint functions, wire types. |
| Data hook | `hooks/useResource.ts` | Runs an async loader, exposes `{ data, loading, error }`. |
| Screens | `screens/`, `builder/` | One component per route — lists, editors, the schema builder. |
| Shell & routing | `App.tsx`, `Layout.tsx`, `components/shell.tsx` | Router, auth gate, theming, navigation chrome. |

### API client (`api/`)

`api/client.ts` owns the single `apiFetch` helper. It attaches the bearer token,
sets JSON headers, and normalizes server errors into an `ApiError` (with
per-field validation messages) or an `AuthError` on `401`. A registered handler
clears the token and redirects to `/login` whenever a stored-token request comes
back `401`.

`api/endpoints.ts` builds on `apiFetch` to give one typed function per
operation — `listContentTypes()`, `listEntries(type, opts)`, `getContentType(name)`,
and so on. `api/types.ts` holds the wire types and small accessors
(`relationMeta`, `mediaMeta`, `enumValues`, …) that read field metadata.

Screens never call `fetch` directly. They call an endpoint function.

### Data hook (`hooks/useResource.ts`)

`useResource(loader, deps)` runs an async loader and returns `{ data, loading,
error }`, re-running when `deps` change. It is the standard way a screen loads
data:

```tsx
const { data, loading, error } = useResource(
  () => listEntries("article", { pageSize: 50 }),
  ["article"],
);
```

### Screens & the builder

`screens/` holds one component per route: `ContentList`, `EntryEditor`,
`MediaLibrary`, `Users`, `Roles`, `ApiTokens`, `Webhooks`, and so on. `builder/`
holds the schema editor — the UI for creating and editing content types and
components (`SchemaEditor`, `FieldPicker`, `FieldConfigModal`, the draft model).

The editors render fields through `components/FieldInput.tsx`, which switches on
a field's `kind` to pick the right input. That switch is the main extension
point — see [Add a custom field widget](../guides/custom-field-widget.md).

### Shell, routing & theming

`App.tsx` defines every route, guards authenticated routes behind `RequireAuth`,
and sets the theme. Theme and density are global: `App.tsx` writes `--accent`,
`data-density`, and `data-theme` onto the document root, and components read the
[design tokens](../../DESIGN.md) from there. Never style a component with a
hard-coded color a token already covers.

`Layout.tsx` and `components/shell.tsx` render the navigation chrome around the
routed screen.

## The `rs-` component library

Shared UI primitives live in `components/ui.tsx` and carry `rs-`-prefixed class
names (`rs-input`, `rs-btn`, `rs-notice`, `rs-modal`, …). Reusable pieces —
`Notice`, `ConfirmDialog`, `EditorBar`, `Checkbox`, `TableSkeleton` — are
exported from there. When you build new UI, reuse these instead of inventing
markup, so spacing, focus states, and theming stay consistent. `DESIGN.md` at
the repo root is the source of truth for the tokens and component conventions.

## There is no plugin API

The admin UI is not extended through a registry or a plugin hook. You customize
it by editing the source in `ui/` and rebuilding the bundle. The guides that
follow show the concrete tasks — adding a field widget, and building and
embedding the result.
