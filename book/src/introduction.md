# Introduction

Rustapi is a headless CMS framework written in Rust (Axum + sqlx) with a React +
TypeScript admin UI. You define content types, and Rustapi gives you a Postgres
schema, a REST API, a GraphQL API, and an admin interface for them — no
boilerplate to hand-write.

## What you get

- **A content model you define.** Create [content types](concepts/content-types.md)
  with typed [fields](concepts/fields.md), [relations](concepts/relations.md),
  reusable [components](concepts/components.md), and
  [single types](concepts/single-types.md) — through the API, the admin UI, or
  [version-controlled TOML](guides/schema-as-code.md).
- **Two APIs, generated.** Every content type is served over both a
  [REST API](reference/rest-api.md) and a [GraphQL API](guides/graphql.md), kept
  in sync with your schema automatically.
- **Editorial workflow.** Optional [draft & publish](concepts/draft-publish.md)
  per type, [roles & permissions](guides/roles.md) for users, and
  [API tokens](guides/api-tokens.md) for machines.
- **Extensibility.** [Media storage](guides/media-storage.md) (local or S3),
  outbound [webhooks](guides/webhooks.md), and in-process
  [write hooks](guides/write-hooks.md).

## How this documentation is organized

- **Getting Started** — install Rustapi, do the first-run setup, and create your
  first content type.
- **Core Concepts** — what the building blocks *are*: content types, fields,
  relations, components, draft/publish, single types.
- **Guides** — task-oriented recipes: schema as code, media, tokens, roles,
  webhooks, import/export, GraphQL.
- **Reference** — exhaustive lookups: the REST and GraphQL surfaces, environment
  variables, and the OpenAPI spec.

New here? Start with [Installation](getting-started/installation.md), then
[Your first content type](getting-started/first-content-type.md).

The REST surface is generated from your live content types — for an
always-current, schema-aware reference, open the Swagger UI at
[`/docs`](reference/openapi.md) on a running server.
