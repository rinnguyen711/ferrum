# GraphQL

Every collection [content type](../concepts/content-types.md) is queryable over
GraphQL as well as REST. The schema is generated from your live content-type
registry and rebuilt whenever you create, change, or delete a type — so it
always matches your content model. This guide shows how to query and mutate;
for the exact naming rules see the [GraphQL surface reference](../reference/graphql.md).

## The endpoint

GraphQL is served at a single endpoint:

```
POST /api/graphql
```

When docs are enabled (`FERRUM_DOCS_ENABLED`, on by default), opening
`/api/graphql` in a browser serves an interactive GraphiQL playground. Auth
works the same as REST — send a `Bearer` token, whether a user JWT or an
[API token](api-tokens.md).

## Query a list

Each collection gets a pluralized list query returning a `data` array and a
`meta` envelope (page, pageSize, total). For an `article` type:

```graphql
query {
  articles(page: 1, pageSize: 10, sort: "createdAt:desc") {
    data {
      id
      title
      body
    }
    meta { page pageSize total }
  }
}
```

The list query takes `page`, `pageSize`, `sort`, and a `filters` JSON argument.

## Query one entry

Each collection also gets a single-item query named after the type, taking an
`id`:

```graphql
query {
  article(id: "0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90") {
    id
    title
  }
}
```

## Populate relations

[Relation](../concepts/relations.md) and [media](media-storage.md) fields are
typed as their target object, so you populate them just by selecting subfields —
no separate `populate` argument as in REST:

```graphql
query {
  article(id: "0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90") {
    title
    author { name }
    cover_image { id file_name width height }
  }
}
```

Media fields resolve to a shared `Media` object with the asset's metadata
(`id`, `file_name`, `mime_type`, `size_bytes`, `width`, `height`, and so on).

## Mutate

Each collection gets `create`, `update`, and `delete` mutations named after the
type. Create and update take a typed `data` input; delete takes an `id` and
returns a boolean:

```graphql
mutation {
  createArticle(data: { title: "Hello world", body: "..." }) {
    id
    title
  }
}
```

```graphql
mutation {
  updateArticle(
    id: "0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90"
    data: { title: "Edited" }
  ) {
    id
    title
  }

}
```

```graphql
mutation {
  deleteArticle(id: "0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90")
}
```

## What's not in GraphQL

[Single types](../concepts/single-types.md) are **not** exposed as collection
queries — read and write them over their REST route. Their object type is still
registered so a relation can point at a single type without breaking the schema.

## Where to go next

- [GraphQL surface](../reference/graphql.md) — exact type, query, and mutation
  naming.
- [Relations](../concepts/relations.md) — how related objects nest.
- [REST API](../reference/rest-api.md) — the equivalent REST surface.
