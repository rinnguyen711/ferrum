# Single types

A single type is a [content type](content-types.md) that holds exactly one
entry, not a collection of them. Use it for one-off content: a homepage, an
about page, global site settings, a footer.

It's the same schema model as a collection — the same [fields](fields.md), the
same [relations](relations.md) and [components](components.md) — but there is no
list, no per-entry ids in the URL, and no "create another". There is just *the*
entry.

## Declaring a single type

Set `kind` to `single` when you create the type:

```sh
curl -X POST http://localhost:8080/admin/content-types \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "homepage",
    "display_name": "Homepage",
    "kind": "single",
    "fields": [
      { "name": "hero_title", "kind": "string", "required": true },
      { "name": "hero_body", "kind": "rich_text" }
    ]
  }'
```

The default `kind` is `collection`; you opt into a single type explicitly.

## Reading and writing the entry

Single types have their own route — `/api/single-types/{name}` — with no id in
the path. Two methods:

| Method | Path | Does |
|---|---|---|
| `GET` | `/api/single-types/{name}` | Read the entry. |
| `PUT` | `/api/single-types/{name}` | Create or replace the entry. |

`GET` returns the entry, or `null` if it hasn't been written yet:

```sh
curl http://localhost:8080/api/single-types/homepage
```

`PUT` is an **upsert**: it creates the entry the first time and replaces it on
every call after. There is no separate create step.

```sh
curl -X PUT http://localhost:8080/api/single-types/homepage \
  -H 'Content-Type: application/json' \
  -d '{ "hero_title": "Welcome", "hero_body": null }'
```

The collection routes don't serve single types and vice versa — calling
`/api/homepage` on a single type returns a message pointing you at
`/api/single-types/homepage`, and using the single-type route on a collection is
rejected the same way.

## Where to go next

- [Content types](content-types.md) — the shared schema model and the `kind`
  property.
- [Fields & field kinds](fields.md) — the fields a single type can hold.
- [REST API](../reference/rest-api.md) — the full single-type surface.
