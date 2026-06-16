# Your first content type

This walkthrough takes you from a running, authenticated server to a piece of
content you can read back over the API. You'll define an `article`
[content type](../concepts/content-types.md), create an entry, and fetch it.

Before you start, make sure you have:

- A server running — see [Installation](installation.md).
- A token in the `TOKEN` shell variable — see [First-run setup](first-run.md).

## Define the content type

A content type names your content and lists its fields. Create an `article` type
with a required `title` and a `body`:

```sh
curl -X POST http://localhost:8080/admin/content-types \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "article",
    "display_name": "Article",
    "fields": [
      { "name": "title", "kind": "string", "required": true },
      { "name": "body", "kind": "text" }
    ]
  }'
```

The server creates the type and a backing table, and returns the stored
definition. `name` must be a lowercase identifier and is fixed once set; `kind`
defaults to `collection`. For the field kinds you can use, see
[Fields & field kinds](../concepts/fields.md).

Rustapi now serves your articles under `/api/article`.

## Create an entry

Post a JSON object whose keys are your field names. The server validates it
against the type — here `title` is required, so it must be present:

```sh
curl -X POST http://localhost:8080/api/article \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{ "title": "Hello world", "body": "My first article." }'
```

```json
{
  "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "created_at": "2026-06-16T12:00:00+00:00",
  "updated_at": "2026-06-16T12:00:00+00:00",
  "title": "Hello world",
  "body": "My first article."
}
```

The server assigns `id`, `created_at`, and `updated_at` — you never set those.

## Read it back

List all articles. The list is paginated: entries come back under `data`, with
page info under `meta`.

```sh
curl http://localhost:8080/api/article \
  -H "Authorization: Bearer $TOKEN"
```

```json
{
  "data": [
    {
      "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
      "created_at": "2026-06-16T12:00:00+00:00",
      "updated_at": "2026-06-16T12:00:00+00:00",
      "title": "Hello world",
      "body": "My first article."
    }
  ],
  "meta": { "page": 1, "pageSize": 25, "total": 1 }
}
```

Or fetch the one you just created by its `id`:

```sh
curl http://localhost:8080/api/article/f47ac10b-58cc-4372-a567-0e02b2c3d479 \
  -H "Authorization: Bearer $TOKEN"
```

## What's next

You've created a content type and an entry end to end. From here:

- Add more [fields](../concepts/fields.md) — relations, enums, media, and more.
- Read how [content types](../concepts/content-types.md) work, including how to
  evolve them and the difference between collections and
  [single types](../concepts/single-types.md).
- Turn on the [draft & publish](../concepts/draft-publish.md) lifecycle.
- Define your schema in version control instead of by API call with
  [Schema as code](../guides/schema-as-code.md).
- Browse the full, schema-aware REST surface in the Swagger UI at `/docs` on
  your running server.
