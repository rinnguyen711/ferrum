# Installation

You can run Rustapi two ways: with Docker Compose for a one-command demo, or
with cargo against your own Postgres. Start with Docker — it's the fastest path
to a running server.

## Run with Docker

You need Docker installed. From the repository root:

```sh
docker compose up --build
```

This starts the API and a Postgres database. When it's up:

- The API is at `http://localhost:8080`.
- The admin UI is at `http://localhost:8080/studio`.
- A health check is at `http://localhost:8080/healthz`.

Confirm the server is live:

```sh
curl http://localhost:8080/healthz
```

That's it — you have a running server. The first thing to do with it is create
an admin account; continue to [First-run setup](first-run.md).

> For anything beyond a local demo, override the default JWT secret with
> `RUSTAPI_JWT_SECRET`. See the [README](../../README.md) for that and the full
> list of environment variables.

## Run with cargo

If you'd rather run the backend directly, you need Rust 1.88, Docker (for the
integration tests), and a Postgres database.

Point Rustapi at your database and give it a JWT secret, then run the server
binary:

```sh
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/rustapi
export RUSTAPI_JWT_SECRET=$(openssl rand -hex 32)
cargo run -p rustapi
```

The server listens on `http://localhost:8080`, the same as the Docker setup.

To serve the bundled admin UI at `/studio`, build it first and point
`RUSTAPI_STUDIO_DIR` at the output:

```sh
cd ui && pnpm install && pnpm build && cd ..
export RUSTAPI_STUDIO_DIR=$PWD/ui/dist
cargo run -p rustapi
```

## The admin UI in development

To work on the admin UI itself, run the Vite dev server. It proxies `/api` and
`/admin` to the backend on `:8080`, so run the server too:

```sh
cd ui
pnpm install
pnpm dev          # http://localhost:5173
```

## Next steps

With a server running, create the first admin account in
[First-run setup](first-run.md), then define a content type in
[Your first content type](first-content-type.md).
