# Docker image distribution — design

Date: 2026-06-16
Status: approved (design)

## Problem

Today the only way to run ferrum is to clone the repo and
`docker compose up --build`, which builds the image from source. That's a
contributor workflow, not a user one. Headless-CMS users expect "pull an image,
point it at Postgres, run" (Strapi/Directus/Payload all ship images). We have no
published image, no published binary, and no crates.io packages (blocked by the
pending name collision anyway).

## Goal

Devs run ferrum without cloning:

```sh
docker run -p 8080:8080 \
  -e DATABASE_URL=postgres://... \
  -e FERRUM_JWT_SECRET=$(openssl rand -hex 32) \
  ghcr.io/<owner>/ferrum:latest
```

…or via a standalone compose file that pulls the published image.

## Decisions

- **Registry: GHCR** (`ghcr.io/<owner>/ferrum`). Free for public, native to
  GitHub Actions, auth via built-in `GITHUB_TOKEN` — no extra secrets.
- **Owner is parameterized.** No git remote is configured yet. CI uses
  `${{ github.repository_owner }}`, so the image path self-resolves once the
  repo is pushed to GitHub. Docs use a `<owner>` placeholder until the real
  owner is known.
- **Trigger: `workflow_dispatch` only.** No push/tag auto-build for now. Every
  publish is a manual "Run workflow" click. Push/`v*`-tag triggers can be wired
  later; the workflow is written so that's a small change.
- **JWT secret stays required.** The binary refuses to boot without
  `FERRUM_JWT_SECRET` (min 32 chars) — see `crates/bin/src/config.rs:32`. We do
  NOT add a static default to the binary: a hardcoded default secret in
  open-source code is an auth-bypass vulnerability (anyone could forge admin
  JWTs against un-overridden deployments). Fail-closed is correct. The prod
  compose enforces the var with a `:?` guard.

## Architecture / what ships

The existing `Dockerfile` already produces a standalone, runnable image and
needs no functional changes:

- distroless `cc-debian12:nonroot` runtime
- UI bundle embedded at `/app/studio`
- defaults: `FERRUM_BIND=0.0.0.0:8080`, `FERRUM_STUDIO_DIR=/app/studio`,
  `FERRUM_LOG=info`
- no schema baked in — schema is opt-in via a `FERRUM_SCHEMA_DIR` mount

The only Dockerfile change is **additive OCI labels** for provenance
(`org.opencontainers.image.source`, `.revision`, `.version`), populated from
build args in CI. These do not affect runtime.

New artifacts:

1. `.github/workflows/docker-publish.yml` — manual multi-arch build + push.
2. `docker-compose.prod.yml` — pulls the published image, no `build:`.
3. Doc updates — `book/src/getting-started/installation.md` and `README.md`.

## Component 1 — Publish workflow

File: `.github/workflows/docker-publish.yml`

- **Trigger:** `workflow_dispatch` with inputs:
  - `tag` (string, default `edge`) — the image tag to publish, e.g. `edge`,
    `0.2.0`, `latest`.
  - `multiarch` (boolean, default `false`) — when true, also build `linux/arm64`.
- **Permissions:** `contents: read`, `packages: write`.
- **Concurrency:** group by workflow + ref, cancel in-flight.
- **Steps:**
  1. `actions/checkout`
  2. `docker/setup-qemu-action` (only needed for arm64; harmless otherwise)
  3. `docker/setup-buildx-action`
  4. `docker/login-action` → `ghcr.io`, user `${{ github.actor }}`, password
     `${{ secrets.GITHUB_TOKEN }}`
  5. `docker/metadata-action` → image
     `ghcr.io/${{ github.repository_owner }}/ferrum`, tag from input
  6. `docker/build-push-action`:
     - `platforms`: `linux/amd64` by default; `linux/amd64,linux/arm64` when
       `multiarch == true`
     - `push: true`
     - `cache-from: type=gha`, `cache-to: type=gha,mode=max`
     - `build-args` / `labels`: OCI source/revision/version

**Multi-arch rationale.** arm64 builds Rust under QEMU emulation and are slow
(cold builds can run 20–40 min). Making `multiarch` an opt-in input keeps routine
`edge` publishes fast (amd64-only) while letting releases (`v*` tags published
manually) build the full `linux/amd64,linux/arm64` set for M-series Macs and ARM
cloud. GHA layer cache mitigates repeat-build cost.

## Component 2 — Standalone prod compose

File: `docker-compose.prod.yml`

Mirrors the dev `docker-compose.yml` structure, with these differences:

- `ferrum` service uses `image: ghcr.io/<owner>/ferrum:latest`, not `build: .`.
- **No schema volume / `FERRUM_SCHEMA_DIR`.** The image runs standalone; schema
  is opt-in and the user adds a mount + env var if they want TOML schema sync. A
  commented stub shows how.
- `FERRUM_JWT_SECRET` is a **required** var via
  `${FERRUM_JWT_SECRET:?set FERRUM_JWT_SECRET to a 32+ char secret}` — no demo
  default leaks into a prod-named file.
- Postgres service stays the same: `postgres:16-alpine`, healthcheck, named
  `pgdata` volume.

## Component 3 — Docs

- `book/src/getting-started/installation.md`: add a **"Run the published image"**
  section as the first/fastest path, with the `docker run` one-liner and the
  prod compose. Keep "Run with Docker (from source)" and "Run with cargo" below
  as the contributor paths. Follows `book/CONTRIBUTING.md` (second person,
  imperative, language-tagged fences, link README for the full env-var list).
- `README.md`: short published-image note near the Docker quickstart, pointing
  at the prod compose and the docs page.
- Use the `<owner>` placeholder consistently until the real owner is known.

## Testing / verification

The published image doesn't exist yet, so we verify the *command and env shape*
against a **locally-built** image instead of a pulled one:

1. `docker build -t ferrum:local .`
2. Run it with the documented `docker run` flags against a throwaway Postgres
   (own container, NOT the user's dev DB), confirm `/healthz` is ok and
   `/auth/setup` works.
3. Validate `docker-compose.prod.yml` with `image: ferrum:local` swapped in:
   `docker compose -f docker-compose.prod.yml config` (lint) and a real
   `up`/healthz round trip.
4. Confirm the `:?` guard: `up` with `FERRUM_JWT_SECRET` unset must fail with a
   clear message.
5. `cd book && mdbook build` passes clean (no broken links, no leftover TODO).

The GitHub Actions workflow itself can't be run locally end-to-end (needs GHCR
push perms). We validate its YAML with `actionlint` if available and rely on the
first manual `workflow_dispatch` run on GitHub to confirm push.

## Out of scope

- Auto-build triggers on push/tag (deferred; workflow is structured to add them).
- Docker Hub publishing.
- crates.io / prebuilt standalone binaries / Helm chart.
- Opt-in `FERRUM_JWT_SECRET=generate` auto-secret mode (noted as a possible
  future DX improvement; not part of this work).
- Any change to the binary's secret-handling behavior.
