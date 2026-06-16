# Environment variables

A reference of the environment variables that configure a Rustapi server. The
server reads these at startup (see `Config::from_env`). Unset optional variables
fall back to the defaults below.

## Core

| Variable | Required | Default | Description |
|---|---|---|---|
| `DATABASE_URL` | yes | — | Postgres connection string, e.g. `postgres://user:pass@host:5432/db`. |
| `RUSTAPI_JWT_SECRET` | yes | — | Secret for signing auth JWTs. Must be at least 32 characters; the server refuses to start otherwise. |
| `RUSTAPI_DB_MAX_CONNECTIONS` | no | `10` | Postgres connection pool size. Raise it for higher concurrency; a value of `0` or an unparseable value falls back to the default. |
| `RUSTAPI_BIND` | no | `0.0.0.0:8080` | Address and port the HTTP server binds to. |
| `RUSTAPI_LOG` | no | `info` | Log filter (`tracing` env-filter syntax, e.g. `debug`, `rustapi=debug`). |
| `RUSTAPI_JWT_TTL_SECS` | no | `86400` | Lifetime of an issued JWT, in seconds (default 24h). |
| `RUSTAPI_PAGE_SIZE_MAX` | no | `100` | Upper bound on the `pageSize` query parameter for list endpoints. Requests above it are clamped. |

## Schema as code

Point the server at TOML schema files to manage content types declaratively.
See the "Schema as code" section of the project `README.md` for the file format.

| Variable | Required | Default | Description |
|---|---|---|---|
| `RUSTAPI_SCHEMA_DIR` | no | unset | Directory of `*.toml` schema files, loaded and merged at startup. Wins over `RUSTAPI_SCHEMA_FILE` when both are set. |
| `RUSTAPI_SCHEMA_FILE` | no | unset | Path to a single `.toml` schema file (used when `RUSTAPI_SCHEMA_DIR` is unset). |
| `RUSTAPI_SCHEMA_SYNC` | no | `additive` | Reconcile mode. `additive` creates types and adds new fields but never drops; `full` also drops types and fields absent from the TOML. |

## Media storage

The Media Library defaults to local filesystem storage; no configuration is
needed for that. To use S3 (or an S3-compatible service), set the provider and
its credentials. The S3 settings may also be configured at runtime via
`PUT /admin/media/settings`; environment values override database settings.

| Variable | Required | Default | Description |
|---|---|---|---|
| `RUSTAPI_MEDIA_PROVIDER` | no | `local` | Storage backend: `local` or `s3`. |
| `RUSTAPI_MEDIA_BASE_DIR` | no | `./media-data` | Local storage directory (when provider is `local`). |
| `RUSTAPI_S3_BUCKET` | for s3 | — | S3 bucket name. |
| `RUSTAPI_S3_REGION` | for s3 | — | S3 region. |
| `RUSTAPI_S3_ENDPOINT` | no | unset | Custom endpoint for S3-compatible services (MinIO, R2, Spaces). |
| `RUSTAPI_S3_ACCESS_KEY` | for s3 | — | S3 access key. |
| `RUSTAPI_S3_SECRET_KEY` | for s3 | — | S3 secret key. |
| `RUSTAPI_SECRET_KEY` | when storing provider secrets in the DB | — | 32-byte hex key used to encrypt provider secrets persisted via the media settings API. |

## Docs and metadata

| Variable | Required | Default | Description |
|---|---|---|---|
| `RUSTAPI_DOCS_ENABLED` | no | `true` | When false (`0`/`false`/`no`), `/openapi.json` and `/docs` return 404. Disable in production if the schema is sensitive. |
| `RUSTAPI_API_VERSION` | no | `0.1.0` | Reported as OpenAPI `info.version`. |
| `RUSTAPI_PUBLIC_URL` | no | `/` | Reported as OpenAPI `servers[0].url`. |
| `RUSTAPI_STUDIO_DIR` | no | unset | Directory of the built admin UI to serve at `/studio`. Unset → no UI route is mounted (API-only). |

## Operational

| Variable | Required | Default | Description |
|---|---|---|---|
| `AUDIT_RETENTION_DAYS` | no | `90` | Audit-log entries older than this are pruned by the background worker. |
