# rustapi

Headless CMS framework in Rust. v1 in progress — see [design spec](docs/superpowers/specs/2026-05-28-rustapi-core-design.md).

## Dev

Requires: Rust 1.88, Docker (for integration tests).

```sh
# Build
cargo build --workspace

# Run unit + integration tests (spawns ephemeral Postgres via testcontainers)
cargo test --workspace

# Run the server against an external Postgres
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/rustapi
export RUSTAPI_ADMIN_KEY=$(openssl rand -hex 32)
cargo run -p rustapi
```

## API

See the [design spec §4](docs/superpowers/specs/2026-05-28-rustapi-core-design.md) for the full HTTP surface.

Quick start:

```sh
# Create a content type
curl -X POST http://localhost:8080/admin/content-types \
  -H "x-api-key: $RUSTAPI_ADMIN_KEY" \
  -H "content-type: application/json" \
  -d '{"name":"post","display_name":"Post","fields":[{"name":"title","kind":"string","required":true}]}'

# Create an entry
curl -X POST http://localhost:8080/api/post \
  -H "x-api-key: $RUSTAPI_ADMIN_KEY" \
  -H "content-type: application/json" \
  -d '{"title":"Hello"}'
```
