# Fields & field kinds

A field is one named, typed slot on a [content type](content-types.md). Every
field has a `name`, a `kind`, and a handful of optional modifiers. The `kind`
decides what the field stores, how Rustapi validates incoming values, and what
Postgres column (or table) backs it.

This is a field on the blog `article` type:

```json
{
  "name": "title",
  "kind": "string",
  "required": true,
  "unique": false,
  "default": null,
  "max_length": 200
}
```

## Field properties

| Property | Meaning |
|---|---|
| `name` | Machine name. Lowercase identifier, immutable. |
| `kind` | The field kind — see the table below. |
| `required` | Reject a write that omits or nulls the field. Default `false`. |
| `unique` | Enforce a unique constraint on the column. Default `false`. |
| `default` | Value applied when the field is omitted. Must match the kind. |
| `max_length` | For text-like kinds, the max character length (`1..=10000`). |
| `kind_meta` | Per-kind configuration (relation target, enum values, …). |

A field `name` must match `^[a-z][a-z0-9_]{0,62}$`: starts with a lowercase
letter, then lowercase letters, digits, and underscores. `Title`, `1st`, and
`with-dash` are all rejected.

Some names are **reserved** because they collide with system columns or SQL
keywords. You cannot name a field any of: `id`, `created_at`, `updated_at`,
`published_at`, `user`, `select`, `from`, `where`, `table`, `order`, `group`,
`having`, `null`, `true`, `false`, `default`, `primary`, `foreign`, `index`.

A field cannot be renamed or have its kind changed in place. To change a field's
type, drop it and add a new one — which discards its data. See
[Changing a content type](content-types.md#changing-a-content-type).

## Field kinds

| Kind | Stores | Notes |
|---|---|---|
| `string` | Short text | One line. `max_length` defaults to 255. |
| `text` | Long text | Multi-line plain text. |
| `integer` | Whole number | 64-bit signed. Rejects fractional values. |
| `float` | Decimal number | 64-bit. Accepts integers (coerced). |
| `boolean` | `true` / `false` | |
| `datetime` | Timestamp | RFC 3339 string, stored UTC. |
| `enum` | One of a fixed set | Allowed values in `kind_meta`. |
| `email` | Email address | Validated at write time. |
| `url` | http/https URL | Validated at write time. |
| `slug` | URL slug | Lowercase, dash-separated. |
| `json` | Arbitrary JSON | Stored as `jsonb`. No schema validation. |
| `rich_text` | Rich document | ProseMirror JSON, stored as `jsonb`. |
| `relation` | Link to another type | See [Relations](relations.md). |
| `media` | Media Library asset(s) | See [Media storage](../guides/media-storage.md). |
| `component` | Structured sub-object | See [Components](components.md). |

A few kinds carry validation beyond shape:

- **`email`** must match `something@something.tld` — no spaces, an `@`, and a dot
  in the domain.
- **`url`** must parse as an absolute URL with an `http` or `https` scheme.
  `ftp://…`, `mailto:…`, and `javascript:…` are rejected.
- **`slug`** must be lowercase letters/digits joined by single dashes
  (`hello-world`), at most 200 characters. No leading/trailing/double dashes, no
  underscores, no capitals.

Validation runs on every write. A value of the wrong type or failing its
format check returns `422 Unprocessable Entity`.

## Modifiers and their limits

Not every modifier applies to every kind. The rules are enforced at create time:

- **`required`** works on most kinds. A `media` field cannot be required, and a
  `many_to_many` relation cannot be required.
- **`unique`** works on scalar kinds. It is **not** allowed on `relation`,
  `media`, `json`, `rich_text`, or `component` fields.
- **`default`** must coerce to the field's kind (a `string` default for an
  `integer` field is rejected). It is **not** allowed on `relation`, `media`, or
  `component` fields. An `enum` default must be one of the declared values.
- **`max_length`**, when set, must be in `1..=10000`. String fields without it
  default to 255 characters.

## Per-kind metadata (`kind_meta`)

Scalar kinds (`string`, `text`, `integer`, `float`, `boolean`, `datetime`,
`email`, `url`, `slug`, `json`, `rich_text`) take **no** `kind_meta` — a
non-empty object is rejected. Four kinds configure themselves through it:

**`enum`** — the allowed values, each a valid identifier, non-empty and unique:

```json
{
  "name": "status",
  "kind": "enum",
  "kind_meta": { "values": ["draft", "review", "published"] }
}
```

**`relation`** — the target type and cardinality (`many_to_one`, `one_to_one`,
or `many_to_many`), plus an optional `inverse` field name on the target:

```json
{
  "name": "author",
  "kind": "relation",
  "kind_meta": { "target": "author", "cardinality": "many_to_one", "inverse": "posts" }
}
```

**`media`** — whether the field holds one asset or many:

```json
{
  "name": "cover_image",
  "kind": "media",
  "kind_meta": { "multiple": false }
}
```

**`component`** — the component `uid` and whether the field repeats:

```json
{
  "name": "seo",
  "kind": "component",
  "kind_meta": { "component": "shared.seo", "multiple": false }
}
```

For each, `kind_meta` rejects unknown keys, so a typo surfaces as a validation
error rather than being silently ignored.

## How fields map to storage

Most fields are a single column on the type's own table, named after the field.
Two kinds differ:

- **Relation** and single **media** fields store a foreign key in a column named
  `<field>_id` (so a `cover_image` media field produces a `cover_image_id`
  column). Because of this, you cannot also declare a sibling field literally
  named `cover_image_id` — the physical columns would collide.
- **`many_to_many`** relations and **multiple** media fields have no column on
  the row; they live in a separate ordered join table.

You don't manage these tables yourself — Rustapi creates and migrates them when
you create or patch the content type.

## Where to go next

- [Content types](content-types.md) — how fields combine into a schema.
- [Relations](relations.md) — linking content types together.
- [Components](components.md) — reusable groups of fields.
- [Media storage](../guides/media-storage.md) — how media assets are stored and
  served.
- [Schema as code](../guides/schema-as-code.md) — declaring fields in TOML.
