# Schema as code

Instead of creating [content types](../concepts/content-types.md) and
[components](../concepts/components.md) through the API, you can declare them in
TOML files and let the server reconcile the database to match on startup. Schema
lives in version control, ships with your code, and stays consistent across
environments.

A type defined this way is **managed**: read-only over the API and in the admin
UI. To change it, you edit the TOML and restart.

## Turn it on

Sync is off by default. Point the server at your schema with one of two
environment variables:

- `FERRUM_SCHEMA_DIR` — a directory; every `*.toml` file in it (non-recursive)
  is loaded and merged.
- `FERRUM_SCHEMA_FILE` — a single `.toml` file.

If both are set, `FERRUM_SCHEMA_DIR` wins. See
[Environment variables](../reference/env-vars.md) for the full list.

The repo ships a ready-to-run blog preset. Run the server against it:

```sh
FERRUM_SCHEMA_DIR=examples/schema/blog cargo run -p ferrum-bin
```

On boot the server loads the TOML, diffs it against the live registry, applies
the plan, and logs a summary:

```
INFO schema sync complete created=4 patched=0 dropped=0 unmanaged=0 mode=Additive
```

Sync runs **fail-fast**: the first error (a parse error, an invalid type, a
field whose kind changed) aborts sync and the server refuses to boot. A broken
schema file never half-applies.

## Write a schema file

A file holds any number of content types and components. Each content type is a
`[[content_type]]` block; its fields are nested `[[content_type.field]]` blocks.
This is `examples/schema/blog/author.toml`:

```toml
[[content_type]]
name = "author"
display_name = "Author"
kind = "collection"

  [[content_type.field]]
  name = "name"
  kind = "string"
  required = true

  [[content_type.field]]
  name = "slug"
  kind = "slug"
  required = true
  unique = true

  [[content_type.field]]
  name = "email"
  kind = "email"

  [[content_type.field]]
  name = "bio"
  kind = "text"

  [[content_type.field]]
  name = "avatar"
  kind = "media"
  kind_meta = { multiple = false }
```

The keys map onto a content type's properties one-to-one — `name`,
`display_name`, `kind`, `options`, and the field list. Per-type switches go in an
inline `options` table:

```toml
[[content_type]]
name = "post"
display_name = "Post"
kind = "collection"
options = { draft_publish = true }
```

Field rules are identical to the API. Field-specific settings live in a
`kind_meta` inline table — for a relation, its target and cardinality:

```toml
  [[content_type.field]]
  name = "author"
  kind = "relation"
  kind_meta = { target = "author", cardinality = "many_to_one", inverse = "posts" }
```

See [Fields & field kinds](../concepts/fields.md) for every kind and its
`kind_meta`.

## Components in TOML

Declare a reusable component with a `[[component]]` block and nested
`[[component.field]]` blocks. A `uid` (such as `shared.seo`) names it; a
component field on a content type points at that uid. This is
`examples/schema/blog/seo.toml`:

```toml
[[component]]
uid = "shared.seo"
display_name = "SEO"

  [[component.field]]
  name = "meta_title"
  kind = "string"
  max_length = 60

  [[component.field]]
  name = "meta_description"
  kind = "text"
  max_length = 160
```

A content type uses it through a `component` field:

```toml
  [[content_type.field]]
  name = "seo"
  kind = "component"
  kind_meta = { component = "shared.seo", multiple = false }
```

Components sync **before** content types, so a component field always finds its
target. You can split components and types across files however you like — the
blog preset keeps each type in its own file and the SEO component in
`seo.toml`. Relation order across files doesn't matter: sync orders type
creation by relation dependency, so a relation's target is always created first.

A duplicate type `name` or component `uid` across files aborts sync.

## Sync modes

`FERRUM_SCHEMA_SYNC` controls how aggressively sync reconciles. It defaults to
`additive`.

| Mode | Creates | Adds fields | Drops types/fields missing from TOML |
|---|---|---|---|
| `additive` (default) | yes | yes | no |
| `full` | yes | yes | yes |

```sh
FERRUM_SCHEMA_SYNC=full FERRUM_SCHEMA_DIR=examples/schema/blog cargo run -p ferrum-bin
```

Both modes are **safe for existing fields**: changing a field's `kind` or
`kind_meta` in place is not supported and aborts sync with an error. To change a
field's type, drop it and add a new one (which discards its data) — and that drop
only happens in `full` mode.

The difference is what happens to things **absent from the TOML**:

- **`additive`** never removes anything. A managed type or field you delete from
  the TOML is left in the database. The type is *unmanaged* instead — its
  `managed` flag is cleared, so it becomes editable again over the API. This is
  the safe default: TOML can't destroy data.
- **`full`** makes the database match the TOML exactly. A managed type or field
  no longer in the TOML is **dropped**, along with its data. Type drops are
  ordered so a type holding a relation is removed before its target, avoiding
  foreign-key violations.

Unmanaged types (ones you created through the API, never in TOML) are never
touched by either mode.

## Managed types are read-only

Once a type is managed, the API and admin UI refuse to change it. `PATCH` and
`DELETE` on a managed type return `409 Conflict`:

```sh
curl -X DELETE 'http://localhost:8080/admin/content-types/author?confirm=true'
```

```json
{
  "error": "content type `author` is managed by a schema file; edit the TOML instead"
}
```

This is the point: the TOML is the single source of truth for a managed type.
Edit the file and restart to change it.

To hand a managed type back to API/UI editing, remove it from the TOML and boot
once in `additive` mode. Sync unmanages it (keeping its data), after which it's
editable again.

## Where to go next

- [Content types](../concepts/content-types.md) — what a content type is and the
  full property list.
- [Fields & field kinds](../concepts/fields.md) — every field kind and its
  `kind_meta`.
- [Components](../concepts/components.md) — reusable field groups.
- [Environment variables](../reference/env-vars.md) — the schema sync variables.
