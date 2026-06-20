# GraphQL surface

The GraphQL schema is generated from the live content-type registry and rebuilt
on every content-type create, patch, or delete. This page is the lookup
reference for how content types map to GraphQL types, queries, and mutations.
For task-oriented usage, see the [GraphQL guide](../guides/graphql.md).

## Endpoint

| Path | Method | Notes |
|---|---|---|
| `/api/graphql` | `POST` | Execute a GraphQL query or mutation. |
| `/api/graphql` | `GET` | GraphiQL playground, only when `docs_enabled`. |

Authentication is the same as REST — a `Bearer` JWT or API token.

## Naming

A content type's name maps to GraphQL names by these rules:

| GraphQL element | Rule | Example (`article`) |
|---|---|---|
| Object type | PascalCase of the name | `Article` |
| Input type | `<Pascal>Input` | `ArticleInput` |
| List envelope | `<Pascal>List` | `ArticleList` |
| List query | pluralized camelCase | `articles` |
| Single query | camelCase of the name | `article` |
| Create mutation | `create<Pascal>` | `createArticle` |
| Update mutation | `update<Pascal>` | `updateArticle` |
| Delete mutation | `delete<Pascal>` | `deleteArticle` |

Pluralization is naive: a trailing `y` becomes `ies`, otherwise an `s` is
appended (`category` → `categories`, `article` → `articles`).

## Queries

Each **collection** type gets two queries:

```graphql
# List, with paging/sort/filter
articles(page: Int, pageSize: Int, sort: String, filters: JSON): ArticleList!

# Single entry by id
article(id: UUID!): Article
```

The `<Type>List` envelope has:

```graphql
type ArticleList {
  data: [Article!]!
  meta: Meta!
}

type Meta { page: Int! pageSize: Int! total: Int! }
```

## Mutations

Each collection type gets three mutations:

```graphql
createArticle(data: ArticleInput!): Article!
updateArticle(id: UUID!, data: ArticleInput!): Article!
deleteArticle(id: UUID!): Boolean!
```

## Field-kind → GraphQL type

[Field kinds](../concepts/fields.md) map to GraphQL types as follows. A
`required` field is non-null (`!`); a multiple relation/media is a list.

| Field kind | Output type | Input type |
|---|---|---|
| `string`, `text`, `slug`, `email`, `url` | `String` | `String` |
| `integer` | `Int` | `Int` |
| `float` | `Float` | `Float` |
| `boolean` | `Boolean` | `Boolean` |
| `datetime` | `DateTime` | `DateTime` |
| `enum` | generated enum type | generated enum type |
| `json`, `rich_text`, `component` | `JSON` | `JSON` |
| `relation` | the target object type | `UUID` (id) |
| `media` | `Media` | `UUID` (id) |

Three custom scalars back these: `UUID`, `DateTime`, and `JSON`.

On **output**, relation and media fields are typed as the related object
(`Article.author: Author`, `Article.cover_image: Media`) so you populate them by
selecting subfields. On **input**, they are the target's `UUID` id (or a list of
ids for multiple).

## The `Media` object

Media fields resolve to a shared `Media` object:

```graphql
type Media {
  id: UUID!
  file_name: String!
  original_filename: String!
  mime_type: String!
  size_bytes: JSON!
  width: Int
  height: Int
  alt_text: String
  caption: String
}
```

`size_bytes` is exposed via the `JSON` scalar because it is a 64-bit integer and
GraphQL's `Int` is 32-bit.

## Localization

When a content type is [localized](rest-api.md#localization), both collection
queries take an optional `locale` argument, and the object type exposes
`document_id` and `locale` as selectable `String` fields:

```graphql
posts(locale: "fr"): PostList!
post(id: UUID!, locale: String): Post
```

```graphql
{
  posts(locale: "fr") {
    data { id document_id locale title }
    meta { total }
  }
}
```

Behavior matches REST: the requested locale is resolved (an unknown code is an
error), the list returns one row per document with default-locale fallback, and
each row's `locale` field reports the code actually served. An omitted `locale`
uses the default. The argument is accepted on non-localized types too, where it
is a no-op.

## Single types

[Single types](../concepts/single-types.md) are **not** exposed as collection
queries or mutations. Their object type is still registered (so a relation may
target one), but you read and write the entry over its REST route,
`/api/single-types/{name}`.

## Where to go next

- [GraphQL guide](../guides/graphql.md) — querying and mutating in practice.
- [Fields & field kinds](../concepts/fields.md) — the field kinds being mapped.
