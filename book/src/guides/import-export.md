# Import & export

Move entries in and out of a content type as CSV. Export selected entries to a
file; import a CSV to bulk-create or update entries. This guide covers both
directions and the import limits.

## Export entries to CSV

Export takes a comma-separated list of entry `ids` and streams back a CSV file.
It requires content-read permission on the type:

```sh
curl -G http://localhost:8080/admin/content-types/article/entries/export \
  --data-urlencode 'ids=0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90,9b6e7c2f-1d4a-8e90-0c3e-1a5e2b1f4d8a' \
  -o article.csv
```

The response is `text/csv` with a `Content-Disposition` attachment named after
the type (`article.csv`). The first row is the header — one column per field,
plus the system columns. `ids` is required; an empty list is rejected.

## Import entries from CSV

Import uploads a CSV as multipart form data under the `file` part. It requires
content-write permission:

```sh
curl -X POST http://localhost:8080/admin/content-types/article/entries/import \
  -F 'file=@article.csv'
```

Each row is an **upsert** keyed on the `id` column:

- A row with an `id` that already exists **updates** that entry.
- A row with no `id` (or an unknown one) **creates** a new entry with a
  generated UUID.

The response reports counts and any per-row errors:

```json
{
  "inserted": 8,
  "updated": 2,
  "errors": [
    { "row": 5, "message": "field `title`: required" }
  ]
}
```

Row numbers count from the file: the header is row 1, so the first data row is
row 2.

## Import limits

Import is a straightforward bulk path, not a full migration tool. Know its
boundaries:

- **At most 1000 rows** per import. Larger files are rejected — split them.
- **No many-to-many or multiple-media fields.** A row that sets one is reported
  as a row error rather than partially applied. (Single relations and single
  media work — send the target's uuid in the column.)
- **No write hooks or events.** Imported rows do not fire
  [write hooks](write-hooks.md) or [webhook](webhooks.md) events.
- **Component shapes aren't validated** — component JSON is stored as-is.
- **Rows are independent upserts.** There's no surrounding transaction: a
  failure partway leaves the rows before it committed. The `errors` list tells
  you exactly which rows didn't apply.

A clean round trip is to export, edit the CSV, and import it back — the `id`
column makes each row update its original entry.

## Where to go next

- [Content types](../concepts/content-types.md) — the fields that become CSV
  columns.
- [Write hooks](write-hooks.md) — note that import bypasses them.
- [REST API](../reference/rest-api.md) — the per-entry write endpoints.
