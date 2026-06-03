# rustapi

Headless CMS framework in Rust. Axum + sqlx backend, React + TS admin UI.

## Layout

- `crates/core` — domain types, validation, field kinds
- `crates/sql` — Postgres storage layer (sqlx)
- `crates/schema` — content-type registry
- `crates/http` — axum router, HTTP surface
- `crates/bin` — server binary
- `ui/` — React + TS admin UI (Vite)

## Docker (quickest demo)

```sh
docker compose up --build
# → http://localhost:8080/studio  (UI)
# → http://localhost:8080/healthz (API)
```

Override the default demo JWT secret for anything beyond a local demo:

```sh
export RUSTAPI_JWT_SECRET=$(openssl rand -hex 32)
docker compose up --build
```

The demo seeds default **Article**, **Author**, and **Category** types with
sample data on first boot (empty DB only). Disable with `RUSTAPI_SEED=false`.

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
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/rustapi
export RUSTAPI_JWT_SECRET=$(openssl rand -hex 32)
export RUSTAPI_STUDIO_DIR=$PWD/ui/dist   # optional: serve admin UI at /studio
cargo run -p rustapi
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
