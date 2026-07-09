# Relations

A relation links one [content type](content-types.md) to another. A `post`
belongs to an `author`; an `author` has many `post`s. You model that link once,
as a `relation` [field](fields.md), and Ferrum maintains the foreign keys, the
join tables, and the reverse lookups.

## Declaring a relation

A relation is a field of kind `relation`. Its `kind_meta` names the **target**
type, the **cardinality**, and an optional **inverse** field name:

```json
{
  "name": "author",
  "kind": "relation",
  "kind_meta": { "target": "author", "cardinality": "many_to_one", "inverse": "posts" }
}
```

This puts an `author` relation on `post`. `inverse` is the name the reverse link
takes when read from the other side — here, an `author` exposes its `posts`.

A relation field cannot be `unique` and cannot carry a `default`. A
`many_to_many` relation cannot be `required`.

## Cardinalities

The `cardinality` decides how many records link on each side and where the link
is stored.

| Cardinality | Means | Stored as |
|---|---|---|
| `many_to_one` | Many of this type point at one target | FK column `<field>_id` on this type's row |
| `one_to_one` | One-to-one link | FK column `<field>_id`, UNIQUE |
| `many_to_many` | Many on both sides | A separate join table |

`many_to_one` is the common case: many posts to one author. `one_to_one` is the
same FK with a uniqueness guarantee — one profile per user. `many_to_many` has
no column on the row; the links live in their own table, so a post can carry any
number of tags and a tag any number of posts.

## Writing a relation

You set a relation by sending the target's `id`. A `many_to_one` or
`one_to_one` field takes a single uuid string (or `null` to clear it):

```json
{
  "title": "Hello world",
  "author": "0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90"
}
```

A `many_to_many` field takes an **array** of uuid strings. An empty array clears
all links; duplicates are de-duplicated:

```json
{
  "title": "Hello world",
  "tags": [
    "11111111-1111-1111-1111-111111111111",
    "22222222-2222-2222-2222-222222222222"
  ]
}
```

Ferrum checks every target id exists before writing. Pointing a relation at a
missing id returns `422 Unprocessable Entity` naming the offending field.

## Reading related records

By default an entry returns the relation as a raw id (or array of ids). To get
the linked objects inline, ask the API to **populate** the relation — see the
[REST API](../reference/rest-api.md) for the `populate` parameter. Population
works in both directions:

- **Forward** — follow the FK to the single target object (`many_to_one`,
  `one_to_one`).
- **Inverse** — read the reverse side. A `many_to_one` becomes an array of
  children on the target (the `inverse` field), capped per parent. A
  `one_to_one` inverse resolves to a single object or `null`.
- **Many-to-many** — resolve through the join table to a capped array, from
  either side.

The inverse only appears when you declared an `inverse` name in the relation's
`kind_meta`.

## Deleting

A relation enforces referential integrity at the database. Deleting a record
that another record still points at is rejected rather than silently orphaning
the link — clear or repoint the referencing relations first.

## Where to go next

- [Fields & field kinds](fields.md) — the `relation` kind and its `kind_meta`.
- [Content types](content-types.md) — the types a relation links.
- [REST API](../reference/rest-api.md) — the `populate` parameter for reading
  related records.
