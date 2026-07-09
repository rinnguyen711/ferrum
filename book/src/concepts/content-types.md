# Content types

A content type is the schema for a kind of content. It names the content, lists
its [fields](fields.md), and decides whether you get a *collection* of entries
or a *single* one-off entry. When you define a content type, Ferrum creates a
Postgres table for it and exposes a REST and GraphQL surface for its entries.

If you model a blog, you create an `article` content type with `title` and
`body` fields; Ferrum then serves your articles at `/api/article`.

## What a content type holds

Every content type has these properties:

| Property | Meaning |
|---|---|
| `name` | Machine name, used in URLs and tables. Lowercase, immutable. |
| `display_name` | Human label shown in the admin UI. |
| `fields` | The list of [fields](fields.md) that make up an entry. |
| `kind` | `collection` (many entries) or `single` (one entry). |
| `options` | Per-type switches, such as `draft_publish`. |
| `id`, `created_at`, `updated_at` | Assigned by the server. |

The `name` must be a lowercase identifier: it starts with a letter and contains
only lowercase letters, digits, and underscores (`^[a-z][a-z0-9_]{0,62}$`).
`Article`, `blog post`, and `1st` are all rejected. Pick the name carefully —
you cannot rename a content type after you create it (see
[Changing a content type](#changing-a-content-type)).

## Collections vs single types

The `kind` decides how many entries a content type has and where they live.

- **Collection** (the default) holds many entries. You create, list, update,
  and delete entries under `/api/<name>` — for example `GET /api/article` lists
  articles and `POST /api/article` creates one.
- **Single type** holds exactly one entry. There is no list; you read and
  replace the single entry at `/api/single-types/<name>`. Use it for a homepage,
  an about page, or global settings.

Single types have their own page — see [Single types](single-types.md).

## System columns

Every entry table carries three columns Ferrum manages for you:

- `id` — a UUID assigned on create.
- `created_at` — set on create.
- `updated_at` — bumped on every write.

You cannot define a field with one of these names, and you cannot drop them.
A handful of other names are reserved because they collide with SQL or internal
columns — including `published_at`, `user`, `select`, `from`, `where`, and
`default`. Naming a field one of these is rejected at create time. See
[Fields & field kinds](fields.md) for the full field rules.

## The registry

Ferrum keeps every content type in an in-memory registry that the HTTP layer
reads on each request, so dispatching a request to the right table costs no
database round trip. The registry is loaded from the database at boot and stays
in sync as you create, patch, and delete types. You never edit it directly — it
is a cache in front of the `_content_types` table, kept consistent by the
schema service.

## Managing content types

You manage content types under the `/admin/content-types` API (or the admin UI,
which calls it):

| Method | Path | Does |
|---|---|---|
| `GET` | `/admin/content-types` | List all content types. |
| `POST` | `/admin/content-types` | Create a content type. |
| `GET` | `/admin/content-types/{name}` | Read one content type. |
| `PATCH` | `/admin/content-types/{name}` | Add/drop fields, edit options. |
| `DELETE` | `/admin/content-types/{name}?confirm=true` | Delete it and its entries. |

To create a content type, post its definition. This creates an `article`
collection with a required `title` and a `body`:

```sh
curl -X POST http://localhost:8080/admin/content-types \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "article",
    "display_name": "Article",
    "kind": "collection",
    "fields": [
      { "name": "title", "kind": "string", "required": true },
      { "name": "body", "kind": "text" }
    ]
  }'
```

```json
{
  "id": "0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90",
  "name": "article",
  "display_name": "Article",
  "kind": "collection",
  "fields": [
    {
      "name": "title",
      "kind": "string",
      "required": true,
      "unique": false,
      "default": null,
      "max_length": null
    },
    {
      "name": "body",
      "kind": "text",
      "required": false,
      "unique": false,
      "default": null,
      "max_length": null
    }
  ],
  "options": { "draft_publish": false },
  "created_at": "2026-06-16T12:00:00Z",
  "updated_at": "2026-06-16T12:00:00Z"
}
```

A content type must have at least one field, no duplicate field names, and no
field whose physical column collides with another's (a relation field `author`
produces an `author_id` column, so a sibling field named `author_id` is
rejected).

For an end-to-end walkthrough that also creates an entry, see
[Your first content type](../getting-started/first-content-type.md).

## Changing a content type

You evolve a content type with `PATCH`. Patches are **additive and
non-destructive**: you can change the `display_name`, add fields, drop fields,
extend an [enum](fields.md) field's allowed values, or set options. You cannot
**rename** a field or **change its kind** in place — to change a field's type,
drop it and add a new one (which discards its data).

This adds a `summary` field and renames the display label:

```sh
curl -X PATCH http://localhost:8080/admin/content-types/article \
  -H 'Content-Type: application/json' \
  -d '{
    "display_name": "Blog article",
    "add_fields": [
      { "name": "summary", "kind": "string" }
    ]
  }'
```

A patch that would change nothing is rejected as a no-op.

To turn on the draft/publish lifecycle for a type, patch its options:

```sh
curl -X PATCH http://localhost:8080/admin/content-types/article \
  -H 'Content-Type: application/json' \
  -d '{ "options": { "draft_publish": true } }'
```

See [Draft & publish](draft-publish.md) for what that changes.

## Deleting a content type

`DELETE` drops the content type **and all its entries**, so it requires explicit
confirmation:

```sh
curl -X DELETE 'http://localhost:8080/admin/content-types/article?confirm=true'
```

Without `?confirm=true` the request is rejected.

## Schema as code vs the API

You can define content types two ways:

- **Through the API / admin UI**, as above. These types are stored in the
  database and editable at any time.
- **Declaratively in TOML**, synced from disk on startup. Types defined this way
  are *managed*: they are read-only over the API, and the `PATCH` and `DELETE`
  calls above are rejected with a message telling you to edit the TOML instead.
  Edit the file and restart to change them.

Managing schema in TOML keeps it in version control and consistent across
environments. See [Schema as code](../guides/schema-as-code.md) for how sync
works and its constraints.
