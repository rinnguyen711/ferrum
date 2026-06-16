# Docker Image Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let devs run rustapi from a published GHCR image instead of cloning the repo, via a manual multi-arch publish workflow, a standalone prod compose file, and updated docs.

**Architecture:** The existing `Dockerfile` already builds a standalone, runnable image (distroless, UI embedded, sane env defaults). We add: (1) a manually-triggered GitHub Actions workflow that builds and pushes to `ghcr.io/<owner>/rustapi`, (2) additive OCI labels on the Dockerfile, (3) a `docker-compose.prod.yml` that pulls the image, (4) docs showing `docker run`. No binary or runtime behavior changes.

**Tech Stack:** GitHub Actions, `docker/build-push-action` + buildx + QEMU, GHCR, Docker Compose, mdBook.

---

## File structure

- Create: `.github/workflows/docker-publish.yml` — manual multi-arch build + push to GHCR.
- Create: `docker-compose.prod.yml` — pulls published image, JWT secret required, no schema mount.
- Modify: `Dockerfile` — add OCI labels to the runtime stage (additive only).
- Modify: `book/src/getting-started/installation.md` — add "Run the published image" as the first path.
- Modify: `README.md` — short published-image note in the Docker section.

Owner is not yet known (no git remote). CI resolves it at runtime via `${{ github.repository_owner }}`; docs and the prod compose use a literal `<owner>` placeholder.

This plan has no automated unit tests (it is CI/YAML/docs/Dockerfile). Verification is done by building the image locally, running it, validating compose, and building the book. Each task ends in a commit.

---

### Task 1: Add OCI labels to the Dockerfile

**Files:**
- Modify: `Dockerfile` (runtime stage, after the `FROM ... AS runtime` block)

- [ ] **Step 1: Add label build-args and OCI labels to the runtime stage**

Open `Dockerfile`. The runtime stage currently is:

```dockerfile
# ─── Stage 3: runtime ──────────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=rust /tmp/rustapi /usr/local/bin/rustapi
COPY --from=ui /ui/dist /app/studio
ENV RUSTAPI_BIND=0.0.0.0:8080 \
    RUSTAPI_STUDIO_DIR=/app/studio \
    RUSTAPI_LOG=info
EXPOSE 8080
USER nonroot
ENTRYPOINT ["/usr/local/bin/rustapi"]
```

Replace it with (adds three `ARG`s and OCI `LABEL`s before `ENV`; everything else unchanged):

```dockerfile
# ─── Stage 3: runtime ──────────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=rust /tmp/rustapi /usr/local/bin/rustapi
COPY --from=ui /ui/dist /app/studio

# Provenance — populated by CI via --build-arg. Default to empty so local
# `docker build` still works without passing them.
ARG OCI_SOURCE=""
ARG OCI_REVISION=""
ARG OCI_VERSION=""
LABEL org.opencontainers.image.source=$OCI_SOURCE \
      org.opencontainers.image.revision=$OCI_REVISION \
      org.opencontainers.image.version=$OCI_VERSION \
      org.opencontainers.image.title="rustapi" \
      org.opencontainers.image.description="Headless CMS framework in Rust"

ENV RUSTAPI_BIND=0.0.0.0:8080 \
    RUSTAPI_STUDIO_DIR=/app/studio \
    RUSTAPI_LOG=info
EXPOSE 8080
USER nonroot
ENTRYPOINT ["/usr/local/bin/rustapi"]
```

- [ ] **Step 2: Verify the image still builds**

Run: `docker build -t rustapi:local .`
Expected: build completes; final line `=> => naming to docker.io/library/rustapi:local`.

- [ ] **Step 3: Verify labels are present**

Run: `docker inspect rustapi:local --format '{{json .Config.Labels}}'`
Expected: JSON containing `org.opencontainers.image.title":"rustapi"` (source/revision/version present but empty for a local build).

- [ ] **Step 4: Commit**

```bash
git add Dockerfile
git commit -m "build: add OCI provenance labels to runtime image"
```

---

### Task 2: Add the manual publish workflow

**Files:**
- Create: `.github/workflows/docker-publish.yml`

- [ ] **Step 1: Create the workflow file**

Create `.github/workflows/docker-publish.yml` with exactly:

```yaml
name: docker-publish

# Manual only for now. Add `push:` / `tags:` triggers later to auto-publish.
on:
  workflow_dispatch:
    inputs:
      tag:
        description: "Image tag to publish (e.g. edge, 0.2.0, latest)"
        type: string
        default: edge
        required: true
      multiarch:
        description: "Also build linux/arm64 (slower, via QEMU)"
        type: boolean
        default: false

concurrency:
  group: docker-publish-${{ github.ref }}
  cancel-in-progress: true

jobs:
  build-push:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up QEMU
        uses: docker/setup-qemu-action@v3

      - name: Set up Buildx
        uses: docker/setup-buildx-action@v3

      - name: Log in to GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Compute platforms
        id: plat
        run: |
          if [ "${{ inputs.multiarch }}" = "true" ]; then
            echo "platforms=linux/amd64,linux/arm64" >> "$GITHUB_OUTPUT"
          else
            echo "platforms=linux/amd64" >> "$GITHUB_OUTPUT"
          fi

      - name: Build and push
        uses: docker/build-push-action@v6
        with:
          context: .
          push: true
          platforms: ${{ steps.plat.outputs.platforms }}
          tags: ghcr.io/${{ github.repository_owner }}/rustapi:${{ inputs.tag }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
          build-args: |
            OCI_SOURCE=https://github.com/${{ github.repository }}
            OCI_REVISION=${{ github.sha }}
            OCI_VERSION=${{ inputs.tag }}
```

- [ ] **Step 2: Lint the workflow YAML**

Run: `actionlint .github/workflows/docker-publish.yml`
Expected: no output (clean). If `actionlint` is not installed, skip — fall back to: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/docker-publish.yml')); print('yaml ok')"` → expect `yaml ok`.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/docker-publish.yml
git commit -m "ci: add manual GHCR docker-publish workflow"
```

---

### Task 3: Add the standalone prod compose file

**Files:**
- Create: `docker-compose.prod.yml`

- [ ] **Step 1: Create the prod compose file**

Create `docker-compose.prod.yml` with exactly (mirrors `docker-compose.yml` but pulls the image, drops the schema mount, and makes the JWT secret required):

```yaml
# Production-style compose: pulls the published image instead of building from
# source. Replace <owner> with the GitHub owner once the image is published.
# Set a real secret before `up`:  export RUSTAPI_JWT_SECRET=$(openssl rand -hex 32)
services:
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: rustapi
      POSTGRES_PASSWORD: rustapi
      POSTGRES_DB: rustapi
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U rustapi"]
      interval: 2s
      timeout: 3s
      retries: 20
    volumes:
      - pgdata:/var/lib/postgresql/data

  rustapi:
    image: ghcr.io/<owner>/rustapi:latest
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      DATABASE_URL: postgres://rustapi:rustapi@postgres:5432/rustapi
      RUSTAPI_JWT_SECRET: ${RUSTAPI_JWT_SECRET:?set RUSTAPI_JWT_SECRET to a 32+ char secret}
      # Optional: ship TOML content types by mounting a dir and pointing here.
      # RUSTAPI_SCHEMA_DIR: /schema
    # volumes:
    #   - ./my-schema:/schema:ro
    ports:
      - "8080:8080"

volumes:
  pgdata:
```

- [ ] **Step 2: Validate compose parses and the required-var guard fires when secret is unset**

Run: `env -u RUSTAPI_JWT_SECRET docker compose -f docker-compose.prod.yml config`
Expected: command FAILS with an error mentioning `set RUSTAPI_JWT_SECRET to a 32+ char secret`.

- [ ] **Step 3: Validate compose parses when the secret is set**

Run: `RUSTAPI_JWT_SECRET=$(openssl rand -hex 32) docker compose -f docker-compose.prod.yml config >/dev/null && echo "config ok"`
Expected: `config ok`.

- [ ] **Step 4: Commit**

```bash
git add docker-compose.prod.yml
git commit -m "build: add standalone prod compose pulling published image"
```

---

### Task 4: End-to-end verify the locally-built image runs standalone

This task uses the `rustapi:local` image from Task 1. It does NOT touch the user's dev DB — it runs its own throwaway Postgres container on port 5433.

**Files:** none (verification only).

- [ ] **Step 1: Start a throwaway Postgres**

```bash
docker run -d --name rustapi-verify-pg \
  -e POSTGRES_USER=rustapi -e POSTGRES_PASSWORD=rustapi -e POSTGRES_DB=rustapi \
  -p 5433:5432 postgres:16-alpine
```
Expected: prints a container id. Wait ~3s for it to accept connections.

- [ ] **Step 2: Confirm the image refuses to boot without a JWT secret**

```bash
docker run --rm --network host \
  -e DATABASE_URL=postgres://rustapi:rustapi@localhost:5433/rustapi \
  rustapi:local
```
Expected: container exits non-zero; logs contain `RUSTAPI_JWT_SECRET must be set`.

- [ ] **Step 3: Run the image the way the docs tell users to**

```bash
docker run -d --name rustapi-verify --network host \
  -e DATABASE_URL=postgres://rustapi:rustapi@localhost:5433/rustapi \
  -e RUSTAPI_JWT_SECRET=$(openssl rand -hex 32) \
  rustapi:local
sleep 4
curl -s http://localhost:8080/healthz
```
Expected: `{"db_ms":...,"status":"ok","version":"0.1.0"}`.

- [ ] **Step 4: Confirm setup endpoint works through the image**

```bash
curl -s -w '\n[%{http_code}]\n' -X POST http://localhost:8080/auth/setup \
  -H 'Content-Type: application/json' \
  -d '{"email":"admin@example.com","password":"change-me-please"}'
```
Expected: `[201]` with a JSON body containing `"roles":["admin"]`.

- [ ] **Step 5: Tear down verify containers**

```bash
docker rm -f rustapi-verify rustapi-verify-pg
```
Expected: prints both container names. (Nothing to commit — verification only.)

---

### Task 5: Document the published-image path in installation.md

**Files:**
- Modify: `book/src/getting-started/installation.md`

- [ ] **Step 1: Replace the intro and insert the published-image section**

In `book/src/getting-started/installation.md`, replace the current intro (lines 3-5):

```markdown
You can run Rustapi two ways: with Docker Compose for a one-command demo, or
with cargo against your own Postgres. Start with Docker — it's the fastest path
to a running server.
```

with:

```markdown
The fastest way to run Rustapi is the published Docker image — no clone, no
build. You can also build from source with Docker Compose, or run the backend
directly with cargo.

## Run the published image

You need Docker and a Postgres database. Point the image at your database and
give it a JWT secret (required, at least 32 characters):

```sh
docker run -p 8080:8080 \
  -e DATABASE_URL=postgres://USER:PASS@HOST:5432/rustapi \
  -e RUSTAPI_JWT_SECRET=$(openssl rand -hex 32) \
  ghcr.io/<owner>/rustapi:latest
```

The server listens on `http://localhost:8080`, with the admin UI at `/studio`.
Confirm it's live:

```sh
curl http://localhost:8080/healthz
```

To run the image and a database together, use the standalone compose file. Set a
secret first, then start it:

```sh
export RUSTAPI_JWT_SECRET=$(openssl rand -hex 32)
docker compose -f docker-compose.prod.yml up
```

Continue to [First-run setup](first-run.md) to create your admin account.

> Replace `<owner>` with the GitHub owner the image is published under. For the
> full list of environment variables, see the [README](../../README.md).
```

> Note: the existing `## Run with Docker` heading directly below becomes the
> "from source" path. Leave it and the rest of the file unchanged.

- [ ] **Step 2: Clarify the from-source heading**

Change the heading on the next section from:

```markdown
## Run with Docker
```

to:

```markdown
## Run with Docker (from source)
```

- [ ] **Step 3: Build the book and confirm it passes clean**

Run: `cd book && mdbook build`
Expected: ends with `INFO HTML book written to ...`, no error about broken links.

- [ ] **Step 4: Confirm no leftover TODO in the page**

Run: `grep -n "TODO" book/src/getting-started/installation.md || echo "no TODO"`
Expected: `no TODO`.

- [ ] **Step 5: Commit**

```bash
git add book/src/getting-started/installation.md
git commit -m "docs: add published-image install path"
```

---

### Task 6: Note the published image in the README

**Files:**
- Modify: `README.md` (the `## Docker (quickest demo)` section starts at line 15)

- [ ] **Step 1: Add a published-image note under the Docker heading**

In `README.md`, immediately after the `## Docker (quickest demo)` heading line (line 15), insert this block before the existing `docker compose up --build` instructions:

```markdown
The quickest way to try Rustapi without cloning is the published image. Point it
at a Postgres database and give it a JWT secret (required, 32+ chars):

```sh
docker run -p 8080:8080 \
  -e DATABASE_URL=postgres://USER:PASS@HOST:5432/rustapi \
  -e RUSTAPI_JWT_SECRET=$(openssl rand -hex 32) \
  ghcr.io/<owner>/rustapi:latest
```

Or run image + database together with the standalone compose file:

```sh
export RUSTAPI_JWT_SECRET=$(openssl rand -hex 32)
docker compose -f docker-compose.prod.yml up
```

Replace `<owner>` with the GitHub owner the image is published under.

To build from source instead:
```

> This leaves the original `docker compose up --build` block intact directly
> below as the from-source option.

- [ ] **Step 2: Confirm the README still renders sane**

Run: `grep -n "ghcr.io/<owner>/rustapi" README.md`
Expected: at least one matching line.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: note published docker image in README"
```

---

## Notes for the implementer

- **Owner placeholder:** `<owner>` stays literal in docs and `docker-compose.prod.yml` until the GitHub owner is known. CI does NOT need it — it uses `${{ github.repository_owner }}`. Do a single find-replace sweep across `docker-compose.prod.yml`, `book/src/getting-started/installation.md`, and `README.md` once the owner is decided.
- **The workflow can't be run locally.** Its real test is the first `workflow_dispatch` run on GitHub. Local verification stops at YAML lint (Task 2) + proving the image itself runs (Tasks 1, 4).
- **Don't touch the user's dev Postgres.** Task 4 runs its own container on port 5433.
- **No binary changes.** If a task tempts you to add a JWT-secret default or change config loading, stop — that's explicitly out of scope (security).
```
