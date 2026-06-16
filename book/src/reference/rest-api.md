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
