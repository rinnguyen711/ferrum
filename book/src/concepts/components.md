# Components

A component is a reusable group of [fields](fields.md) you embed inside a
[content type](content-types.md) — without repeating those fields on every type.
Define an `seo` component once (meta title, description, canonical URL), then
drop it onto `article`, `page`, and `product`.

Unlike a content type, a component has no table and no entries of its own. It is
a *shape*. Its values are stored inline on the entry that embeds it, as `jsonb`.

## What a component holds

| Property | Meaning |
|---|---|
| `uid` | Machine id. Two dot-separated segments, e.g. `shared.seo`. |
| `display_name` | Human label shown in the admin UI. |
| `fields` | The list of [fields](fields.md) in the component. |

The `uid` must be two dot-separated lowercase identifiers — a namespace and a
name, like `shared.seo` or `blocks.hero_block`. The namespace is a convention
for grouping; `shared.*` is the usual home for cross-cutting components.

Component fields follow the same rules as content-type fields, with the same
[field kinds](fields.md).

## Embedding a component

A content type uses a component through a `component` field. Its `kind_meta`
names the `uid` and whether the field holds one object or an array:

```json
{
  "name": "seo",
  "kind": "component",
  "kind_meta": { "component": "shared.seo", "multiple": false }
}
```

With `multiple: false` the entry carries one SEO object; with `multiple: true`
it carries an ordered array of them (a list of repeatable blocks). A component
field can't be `unique`, `required`, or carry a `default`.

On the entry, the component value is a nested object matching the component's
fields:

```json
{
  "title": "Hello world",
  "seo": {
    "meta_title": "Hello world",
    "meta_description": "An introduction.",
    "no_index": false
  }
}
```

## Managing components

Components live under the `/admin/components` API (or the admin UI, which calls
it):

| Method | Path | Does |
|---|---|---|
| `GET` | `/admin/components` | List all components. |
| `POST` | `/admin/components` | Create a component. |
| `GET` | `/admin/components/{uid}` | Read one component. |
| `PUT` | `/admin/components/{uid}` | Replace its display name and fields. |
| `DELETE` | `/admin/components/{uid}` | Delete it (if unreferenced). |

Create a component by posting its shape:

```sh
curl -X POST http://localhost:8080/admin/components \
  -H 'Content-Type: application/json' \
  -d '{
    "uid": "shared.seo",
    "display_name": "SEO",
    "fields": [
      { "name": "meta_title", "kind": "string", "max_length": 60 },
      { "name": "meta_description", "kind": "text", "max_length": 160 },
      { "name": "no_index", "kind": "boolean", "default": false }
    ]
  }'
```

A `uid` that already exists is rejected as a conflict.

## Deleting a referenced component

You cannot delete a component while a content type still has a field pointing at
it. The delete is rejected with the list of referencing types:

```json
{
  "error": "component `shared.seo` is referenced by: article, page"
}
```

Remove the `component` fields from those types first, then delete the component.

## Managed components

Like content types, a component can be **managed** — declared in TOML and synced
on startup instead of created through the API. A managed component is read-only
over the API; you edit the TOML and restart to change it. See
[Schema as code](../guides/schema-as-code.md) for how component sync works.

## Where to go next

- [Fields & field kinds](fields.md) — the fields a component can hold.
- [Content types](content-types.md) — what embeds a component.
- [Schema as code](../guides/schema-as-code.md) — declaring components in TOML.
