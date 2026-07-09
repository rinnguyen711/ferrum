# OpenAPI / Swagger

A running server generates an OpenAPI 3.1 document from the live content-type
registry and serves a browsable Swagger UI alongside it. Because the spec is
built from the registry at request time, it always reflects your current content
types — create a type and its REST paths appear in the spec immediately.

## Endpoints

| Path | Serves |
|---|---|
| `/openapi.json` | The OpenAPI 3.1 document as JSON. |
| `/docs` | Swagger UI, a browsable view of the spec. |

Both are **public** (no authentication) and need no special setup — start the
server and open `/docs`:

```sh
curl http://localhost:8080/openapi.json
```

The document's `info.version` and `servers[0].url` come from
`FERRUM_API_VERSION` and `FERRUM_PUBLIC_URL`. See
[Environment variables](env-vars.md).

## Disabling docs

The spec and Swagger UI are controlled by `FERRUM_DOCS_ENABLED` (on by
default). Set it to `false` (or `0`/`no`) to run an API-only server with neither
`/openapi.json` nor `/docs` mounted — a common choice in production:

```sh
FERRUM_DOCS_ENABLED=false
```

The same flag also gates the GraphiQL playground on
[`/api/graphql`](graphql.md).

## Where to go next

- [REST API](rest-api.md) — the REST surface this spec describes.
- [Environment variables](env-vars.md) — `FERRUM_DOCS_ENABLED`,
  `FERRUM_API_VERSION`, `FERRUM_PUBLIC_URL`.
