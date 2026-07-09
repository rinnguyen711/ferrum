# Ferrum

Headless CMS framework in Rust. Axum + sqlx backend, React + TS admin UI.

## Layout

- `crates/core` — domain types, validation, field kinds
- `crates/sql` — Postgres storage layer (sqlx)
- `crates/schema` — content-type registry
- `crates/http` — axum router, HTTP surface
- `crates/media` — pluggable media storage (local + S3 providers)
- `crates/bin` — server binary
- `ui/` — React + TS admin UI (Vite)

## Docker (quickest demo)

The quickest way to try Ferrum without cloning is the published image. Point it
at a Postgres database and give it a JWT secret (required, 32+ chars):

```sh
docker run -p 8080:8080 \
  -e DATABASE_URL=postgres://USER:PASS@HOST:5432/ferrum \
  -e FERRUM_JWT_SECRET=$(openssl rand -hex 32) \
  ghcr.io/<owner>/ferrum:latest
```

Or run image + database together with the standalone compose file:

```sh
export FERRUM_JWT_SECRET=$(openssl rand -hex 32)
docker compose -f docker-compose.prod.yml up
```

Replace `<owner>` with the GitHub owner the image is published under.

To build from source instead:

```sh
docker compose up --build
# → http://localhost:8080/studio  (UI)
# → http://localhost:8080/healthz (API)
```

Override the default demo JWT secret for anything beyond a local demo:

```sh
export FERRUM_JWT_SECRET=$(openssl rand -hex 32)
docker compose up --build
```

To ship pre-defined content types, set `FERRUM_SCHEMA_DIR` (see [Schema as code](#schema-as-code) below).

### Media storage

The Media Library defaults to **local filesystem** storage under `./media-data`
— no configuration needed. To use S3 (or an S3-compatible service) set:

```sh
export FERRUM_MEDIA_PROVIDER=s3
export FERRUM_S3_BUCKET=my-bucket
export FERRUM_S3_REGION=us-east-1
export FERRUM_S3_ENDPOINT=https://...   # optional, for MinIO/R2/Spaces
export FERRUM_S3_ACCESS_KEY=...
export FERRUM_S3_SECRET_KEY=...
```

Alternatively configure the provider at runtime via `PUT /admin/media/settings`.
Storing provider secrets in the database requires a 32-byte hex encryption key:

```sh
export FERRUM_SECRET_KEY=$(openssl rand -hex 32)
```

Env configuration always overrides database settings.

### First-run setup

The users table starts empty. Create the initial admin (only works while no
user exists — the endpoint returns 409 afterwards):

```sh
curl -X POST http://localhost:8080/auth/setup \
  -H "content-type: application/json" \
  -d '{"email":"admin@example.com","password":"change-me-please"}'
```

Log in to get a JWT, then send `Authorization: Bearer <token>` on API requests:

```sh
curl -X POST http://localhost:8080/auth/login \
  -H "content-type: application/json" \
  -d '{"email":"admin@example.com","password":"change-me-please"}'
# → {"token":"<jwt>","expires_at":<unix>}
```

## Backend

Requires: Rust 1.88, Docker (integration tests).

```sh
# Build
cargo build --workspace

# Run unit + integration tests (spawns ephemeral Postgres via testcontainers)
cargo test --workspace

# Run the server against an external Postgres
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/ferrum
export FERRUM_JWT_SECRET=$(openssl rand -hex 32)
export FERRUM_DB_MAX_CONNECTIONS=20          # optional: Postgres pool size, default 10
export FERRUM_STUDIO_DIR=$PWD/ui/dist        # optional: serve admin UI at /studio
export FERRUM_SCHEMA_DIR=examples/schema/blog # optional: load TOML schema files on startup
cargo run -p ferrum
```

## Admin UI

Vite + React 18 + TypeScript. Currently renders from mock data; API wiring TBD.

```sh
cd ui
pnpm install
pnpm dev      # http://localhost:5173 (proxies /api + /admin to :8080)
pnpm build    # static bundle in ui/dist
pnpm typecheck
```

Screens: Dashboard, Content Manager, Entry Editor, Content-Type Builder, Media Library, API tokens.

## API quick start

```sh
# Obtain a token first (see First-run setup above), then:
TOKEN=<jwt from /auth/login>

# Create a content type
curl -X POST http://localhost:8080/admin/content-types \
  -H "authorization: Bearer $TOKEN" \
  -H "content-type: application/json" \
  -d '{"name":"post","display_name":"Post","fields":[{"name":"title","kind":"string","required":true}]}'

# Create an entry
curl -X POST http://localhost:8080/api/post \
  -H "authorization: Bearer $TOKEN" \
  -H "content-type: application/json" \
  -d '{"title":"Hello"}'
```

## API documentation

The server generates an OpenAPI 3.1 spec from the live content-type registry,
so the dynamic `/api/{type}` endpoints always reflect your current schema.

- `GET /openapi.json` — the generated spec.
- `GET /docs` — Swagger UI, browsable in any browser.

Both are **public by default**. The spec exposes every content type's name and
field structure, so disable them in production if your schema is sensitive.

```sh
export FERRUM_DOCS_ENABLED=false        # default true; false → /openapi.json and /docs return 404
export FERRUM_API_VERSION=1.2.3         # default 0.1.0; reported as OpenAPI info.version
export FERRUM_PUBLIC_URL=https://api.example.com   # default "/"; reported as OpenAPI servers[0].url
```

## Schema as code

Define content types declaratively in TOML files and let the server sync the database to match on startup — no manual API calls or UI clicks required.

Point `FERRUM_SCHEMA_DIR` at a directory of `*.toml` files; all are loaded and merged:

```sh
export FERRUM_SCHEMA_DIR=examples/schema/blog
```

A working blog preset ships in `examples/schema/blog/` (`author.toml`, `post.toml`). Trimmed example:

```toml
# post.toml
[[content_type]]
name = "post"
display_name = "Post"
kind = "collection"
options = { draft_publish = true }

  [[content_type.field]]
  name = "title"
  kind = "string"
  required = true

  [[content_type.field]]
  name = "author"
  kind = "relation"
  kind_meta = { target = "author", cardinality = "many_to_one", inverse = "posts" }
```

Alternatively, point `FERRUM_SCHEMA_FILE` at a single `.toml` file (used when `FERRUM_SCHEMA_DIR` is not set).

### Components

Reusable field groups (components) can be declared with `[[component]]` blocks and referenced from a `component` field:

```toml
[[component]]
uid = "shared.seo"
display_name = "SEO"

  [[component.field]]
  name = "meta_title"
  kind = "string"

[[content_type]]
name = "post"
display_name = "Post"

  [[content_type.field]]
  name = "seo"
  kind = "component"
  kind_meta = { component = "shared.seo", multiple = false }
```

Components are synced **before** content types, so a `component` field can reference a component the same files define. Managed components are read-only in the UI/API (edits return **409**). In `full` mode, sync will **not** drop a component still referenced by a content type — it aborts startup so you remove the reference first. This also means you cannot drop a component and its last referencing content type in a single `full` sync — remove the referencing field or type first, then drop the component on a subsequent sync.

### Sync modes

```sh
export FERRUM_SCHEMA_SYNC=additive   # default — creates types, adds new fields, never drops
export FERRUM_SCHEMA_SYNC=full       # also drops types and fields absent from the TOML
```

### Managed types are read-only

Types defined in TOML are locked in the admin UI and API. Attempts to edit or delete them via the API return **409 Conflict**. To change a managed type, edit the TOML and restart the server.

### Rename limitation

Field and type **rename is not supported**. A name change is treated as add-new + (in `full` mode) drop-old, which loses the old column's data. To rename safely, use the admin UI for unmanaged types, or accept the data loss in `full` mode.

### Fail-fast on errors

If any TOML file is invalid or a sync step fails, the server aborts startup. Treat schema files like DB migrations — fix the error before the server will start.

Each type is created/updated in its own transaction and they are applied in sequence, so a mid-sync failure can leave earlier types already applied. Fix the offending file and restart; sync is idempotent, so the already-applied types are skipped on the next boot.
