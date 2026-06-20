# REST API

All REST endpoints are documented live — run the server and open `/docs` for the full Swagger UI. This page covers cross-cutting list behaviour: pagination modes, sorting, and count control.

## Pagination

Every `GET /api/<type>` list endpoint supports two pagination modes: offset (default) and keyset/cursor. Choose based on your use case.

### Offset pagination

Add `?page=<n>&pageSize=<n>` to jump to any page. The response `meta` includes `total` — the full row count across all pages.

Good for admin UIs and anywhere you need a page-number control or need to display the total. Performance degrades on deep pages: both the `COUNT(*)` and the `OFFSET` scan grow with table size.

```sh
curl http://localhost:8080/api/article?page=2&pageSize=25
```

```json
{
  "data": [
    { "id": "01hw...", "title": "Rust async tips", "body": "...", "views": 412 },
    { "id": "01hv...", "title": "Intro to axum", "body": "...", "views": 308 }
  ],
  "meta": {
    "page": 2,
    "pageSize": 25,
    "total": 142
  }
}
```

### Keyset pagination

Start with `?cursor=first` to begin a keyset traversal. Each response `meta` contains a `nextCursor` token; pass it as `?cursor=<token>` on the next request. When `nextCursor` is `null` you have reached the last page.

Keyset cost is constant at any depth — ideal for feeds, infinite scroll, and full-export jobs. Trade-offs: `total` is never returned, and you cannot jump to an arbitrary page number.

Sorting uses a `(sort_field, id)` composite key internally, so paging is stable even when the sort field contains duplicate values.

First page:

```sh
curl "http://localhost:8080/api/article?cursor=first&pageSize=25&sort=views:desc"
```

```json
{
  "data": [
    { "id": "01hw...", "title": "Rust async tips", "body": "...", "views": 412 },
    { "id": "01hv...", "title": "Intro to axum", "body": "...", "views": 308 }
  ],
  "meta": {
    "page": 1,
    "pageSize": 25,
    "nextCursor": "eyJ2aWV3cyI6MzA4LCJpZCI6IjAxaHYuLi4ifQ"
  }
}
```

Follow-up request using the token from `meta.nextCursor`:

```sh
curl "http://localhost:8080/api/article?cursor=eyJ2aWV3cyI6MzA4LCJpZCI6IjAxaHYuLi4ifQ&pageSize=25&sort=views:desc"
```

Last page: `meta.nextCursor` is `null`.

### Skipping the count

In offset mode, `total` requires a full-table `COUNT(*)`. If you don't need it, pass `?withCount=false` to omit it and save the extra query.

```sh
curl "http://localhost:8080/api/article?withCount=false"
```

`meta` will contain `page` and `pageSize` but no `total` key.

### Sorting

Use `?sort=<field>:<direction>` to control order. Direction is `asc` or `desc`. Default is `created_at:desc`.

```sh
curl "http://localhost:8080/api/article?sort=views:desc"
```

Any stored field can be used as the sort key. In keyset mode the sort key is always paired with `id` as a tiebreaker to guarantee stable, gap-free paging.

## Localization

A content type can opt into localization. A localized entry exists as one row per locale, and all of a translation set's rows share a stable `document_id`. Reads accept a `?locale=<code>` selector and fall back to the default locale when a translation is missing.

### Registering locales

Locales live in a global registry, managed under `/admin/locales` (admin token required). The database seeds one default locale, `en` (English).

List the registered locales:

```http
GET /admin/locales
```

```json
{
  "data": [
    { "code": "en", "name": "English", "is_default": true, "position": 0 }
  ]
}
```

Add or update a locale with `POST /admin/locales`. The body takes `code`, `name`, and optional `position` and `is_default`. A `code` must be a lowercase locale tag (`fr`, `pt-br`); anything else returns `422`.

```http
POST /admin/locales
Content-Type: application/json

{ "code": "fr", "name": "French", "position": 1 }
```

Setting `is_default: true` flips the default to this locale and clears the previous one in the same transaction — exactly one locale is the default at any time.

Delete a locale by code:

```http
DELETE /admin/locales/fr
```

You cannot delete the default locale — `DELETE /admin/locales/en` returns `422` while `en` is the default. Reassign the default first.

### Enabling localization on a type

Set the `localized` option to `true` when you create the content type:

```http
POST /admin/content-types
Content-Type: application/json

{
  "name": "post",
  "fields": [
    { "name": "title", "kind": "string", "required": true },
    { "name": "slug", "kind": "slug", "unique": true }
  ],
  "options": { "localized": true }
}
```

You can also localize an existing type by `PATCH`ing its options. Existing rows are backfilled to the default locale — each row's `document_id` is set to its own `id`, and its `locale` to the default code:

```http
PATCH /admin/content-types/post
Content-Type: application/json

{ "options": { "localized": true } }
```

Localizing scopes any unique field constraint (such as a unique `slug`) to `(document_id, locale, <field>)`, so two locales of the same document can share a slug while duplicates within one locale are still rejected.

> De-localizing a type (turning `localized` back to `false`) is unsupported in v1 and is rejected — it would drop locale rows ambiguously.

### Reading localized entries

Add `?locale=<code>` to the list and get endpoints. For a localized type the path id is the **`document_id`** (the stable cross-locale handle), not the per-locale row id.

Get one document in a locale:

```http
GET /api/post/01hw8c.../?locale=fr
```

The served row carries its own `document_id` and `locale` columns, so the response tells you which locale was served:

```json
{
  "id": "01hw9a...",
  "document_id": "01hw8c...",
  "locale": "fr",
  "title": "Bonjour",
  "slug": "bonjour"
}
```

If the requested locale has no row for that document, the read falls back to the default-locale row, and the `locale` field reports the code actually served (so you can detect the fallback):

```http
GET /api/post/01hw8c.../?locale=de
```

```json
{
  "document_id": "01hw8c...",
  "locale": "en",
  "title": "Hello"
}
```

A locale that is not in the registry returns `422`. Passing `?locale=` to a non-localized type is a no-op, not an error.

List endpoints return one row per document — the requested-locale row where it exists, otherwise the default-locale fallback row. The list `meta` echoes the requested locale:

```http
GET /api/post?locale=fr
```

```json
{
  "data": [
    { "document_id": "01hw8c...", "locale": "fr", "title": "Bonjour" },
    { "document_id": "01hw7b...", "locale": "en", "title": "Second post" }
  ],
  "meta": {
    "page": 1,
    "pageSize": 25,
    "total": 2,
    "locale": "fr"
  }
}
```

Localized lists use offset pagination only — keyset/cursor paging over the locale-collapsed result set is deferred. A `?cursor=` on a localized list is ignored and offset paging is used.

### Writing translations

`POST /api/<type>?locale=<code>` creates a row in that locale. To add a translation to an existing document, reuse its `document_id` in the body. Omit `document_id` to start a brand-new document (a fresh `document_id` is assigned).

Create the source (English) entry — no `document_id`, so a new document is started:

```http
POST /api/post?locale=en
Content-Type: application/json

{ "title": "Hello", "slug": "hello" }
```

Add the French translation to the same document by reusing its `document_id`:

```http
POST /api/post?locale=fr
Content-Type: application/json

{ "title": "Bonjour", "slug": "bonjour", "document_id": "01hw8c..." }
```

- An unknown locale returns `422`.
- A duplicate `(document_id, locale)` — the same document already has a row in that locale — returns `409`.

Updates and deletes target the **exact** `(document_id, locale)` row with no fallback. The path id is the `document_id`; the `?locale=` selects the row. If that locale row does not exist, the write returns `404` (it does not silently fall back to the default).

```http
PUT /api/post/01hw8c.../?locale=fr
Content-Type: application/json

{ "title": "Bonjour le monde", "slug": "bonjour" }
```

```http
DELETE /api/post/01hw8c.../?locale=fr
```

### Limitations (v1)

- Publish/unpublish is per row: `POST /api/<type>/:id/publish` is keyed by the content-type **row id** and takes no `?locale=`. Target the specific locale's row id to publish that translation; locales publish independently.
- Localized lists use offset pagination only (no keyset cursor).
- De-localizing a type is unsupported.
- Relations target a specific locale **row**, not a document — cross-locale relation resolution is out of scope for v1.
