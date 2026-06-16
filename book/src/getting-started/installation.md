# Installation

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

## Run with Docker (from source)

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
