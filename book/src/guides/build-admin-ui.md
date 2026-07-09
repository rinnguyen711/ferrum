# Build & embed the admin UI

The admin UI is a static React bundle. In development you run it from the Vite
dev server; in production you build it once and point the server at the output,
which serves it at `/studio`. This guide covers both, and the routing details
that make the two modes line up.

Read [Admin UI architecture](../concepts/admin-ui.md) for how the app is
structured.

## Develop against a running API

Start the API (for example with `docker compose up`, see the
[README](../../README.md)), then run the UI dev server:

```sh
cd ui
pnpm install      # first time only
pnpm dev          # http://localhost:5173
```

The dev server proxies `/api`, `/admin`, and `/healthz` to `http://localhost:8080`,
so the SPA calls the real API with no CORS configuration. Edit a file under
`ui/src/` and the page hot-reloads. In dev the app is served from the root path
(`/`), so you reach it at `http://localhost:5173/`.

Type-check while you work:

```sh
cd ui
pnpm typecheck
```

## Build the production bundle

```sh
cd ui
pnpm build        # static bundle in ui/dist
```

This type-checks (`tsc -b`) and then bundles with Vite into `ui/dist`. The build
sets the app's base path to `/studio/` (the dev server keeps `/`), so the
production bundle expects to be mounted under `/studio` — not the root. That
base path is fixed in `ui/vite.config.ts`; if you serve the bundle from a
different prefix, change the `base` there and rebuild.

## Serve the bundle from the server

The server mounts a built bundle at `/studio` when you point it at the output
directory with the `FERRUM_STUDIO_DIR` environment variable
(see [Environment variables](../reference/env-vars.md)):

```sh
export FERRUM_STUDIO_DIR=$PWD/ui/dist
cargo run -p ferrum-bin
# → http://localhost:8080/studio
```

When `FERRUM_STUDIO_DIR` is unset, no UI route is mounted and the server runs
API-only. That is the right default for a headless deployment that never serves
the admin UI.

### How the mount works

`mount_studio` (in `crates/http/src/routes/mod.rs`) registers three routes:

```text
GET /studio          → index.html
GET /studio/         → index.html
GET /studio/*rest    → the matching file if it exists, otherwise index.html
```

The fallback to `index.html` is what makes client-side routes work: a deep link
like `/studio/content/article/13` has no matching file on disk, so the server
returns `index.html` and react-router resolves the path in the browser. The
mount is a normal sub-router (not a nested service) so a missing asset does not
leak a bare `404` through the outer status — its own handler always answers.

## Verify the embedded UI

With `FERRUM_STUDIO_DIR` set and the server running, confirm both a top-level
and a deep route load:

```sh
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8080/studio
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8080/studio/content/article/13
```

Both should print `200` and return the SPA's HTML. Open
`http://localhost:8080/studio` in a browser and check that a custom change you
built is present — the bundle the server serves is the one in
`FERRUM_STUDIO_DIR`, so rebuild (`pnpm build`) after any UI edit.
