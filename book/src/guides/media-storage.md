# Media storage

Rustapi stores uploaded files — images, documents, anything — in the Media
Library, and serves them back through a storage **provider**. Two providers ship
in the box: the local filesystem and Amazon S3 (or any S3-compatible service).
This guide shows how to pick a provider, upload assets, and attach them to your
content.

## Choose a provider

The active provider is resolved at startup in this order: environment variables
win, then the provider saved through the settings API, then a `local` default
writing to `./media-data`.

For the local provider, set the base directory:

```sh
RUSTAPI_MEDIA_PROVIDER=local
RUSTAPI_MEDIA_BASE_DIR=./media-data
```

For S3, set the bucket and credentials:

```sh
RUSTAPI_MEDIA_PROVIDER=s3
RUSTAPI_S3_BUCKET=my-bucket
RUSTAPI_S3_REGION=us-east-1
RUSTAPI_S3_ACCESS_KEY=...
RUSTAPI_S3_SECRET_KEY=...
```

See [Environment variables](../reference/env-vars.md) for the full set,
including the optional S3 `endpoint` for S3-compatible services.

## Configure a provider at runtime

You can also set the provider through the API instead of env vars — useful when
admins manage storage from the UI. List what's available, then save a config:

```sh
# What providers exist and what fields each needs
curl http://localhost:8080/admin/media/providers
```

```sh
# Save the active provider
curl -X PUT http://localhost:8080/admin/media/settings \
  -H 'Content-Type: application/json' \
  -d '{ "provider": "s3", "config": {
        "bucket": "my-bucket", "region": "us-east-1",
        "access_key": "AKIA...", "secret_key": "..." } }'
```

Test a config before committing to it — this opens a real connection to the
backend:

```sh
curl -X POST http://localhost:8080/admin/media/settings/test \
  -H 'Content-Type: application/json' \
  -d '{ "provider": "s3", "config": { "bucket": "my-bucket", "region": "us-east-1",
        "access_key": "AKIA...", "secret_key": "..." } }'
```

Secret fields (such as the S3 `secret_key`) are encrypted at rest and masked as
`••••` when you read the settings back. Storing secrets in the database
**requires `RUSTAPI_SECRET_KEY`** (a 64-character hex key); without it, saving a
provider that has secrets is rejected. On read and re-save, a value still equal
to the mask reuses the stored secret instead of overwriting it.

## Upload an asset

Assets are uploaded as multipart form data. The `file` part is required;
`folder_id` is optional:

```sh
curl -X POST http://localhost:8080/admin/media/assets \
  -F 'file=@cover.jpg' \
  -F 'folder_id=0c3e1a5e-2b1f-4d8a-9b6e-7c2f1d4a8e90'
```

On upload Rustapi detects the MIME type from the bytes, computes a SHA-256
checksum, reads image dimensions when it can, writes the bytes through the
provider, and records the asset:

```json
{
  "id": "9b6e7c2f-1d4a-8e90-0c3e-1a5e2b1f4d8a",
  "folder_id": null,
  "file_name": "cover.jpg",
  "mime_type": "image/jpeg",
  "size_bytes": 48211,
  "width": 1200,
  "height": 630,
  "original_filename": "cover.jpg",
  "created_at": "2026-06-17T12:00:00Z",
  "updated_at": "2026-06-17T12:00:00Z"
}
```

## Organize and serve assets

The Media Library is a set of assets grouped into nested folders:

| Method | Path | Does |
|---|---|---|
| `GET` | `/admin/media/assets` | List assets (filter by `?folder_id=`). |
| `GET` | `/admin/media/assets/{id}` | Read one asset's metadata. |
| `PATCH` | `/admin/media/assets/{id}` | Edit `alt_text`, `caption`, `file_name`, move folders. |
| `DELETE` | `/admin/media/assets/{id}` | Delete the asset and its stored bytes. |
| `GET` | `/admin/media/assets/{id}/raw` | Stream the raw file. |
| `GET` / `POST` | `/admin/media/folders` | List or create folders. |

A folder must be empty before it can be deleted.

## Attach media to content

To put media on your content, give a content type a
[`media` field](../concepts/fields.md):

```json
{ "name": "cover_image", "kind": "media", "kind_meta": { "multiple": false } }
```

Then set it on an entry by sending the asset's `id`. A single media field takes
one uuid string (or `null`); a `multiple` media field takes an array of uuid
strings:

```json
{
  "title": "Hello world",
  "cover_image": "9b6e7c2f-1d4a-8e90-0c3e-1a5e2b1f4d8a"
}
```

Rustapi checks the asset exists before saving. A media field can't be
`required`, `unique`, or carry a `default`.

## Where to go next

- [Fields & field kinds](../concepts/fields.md) — the `media` field kind.
- [Environment variables](../reference/env-vars.md) — provider configuration.
- [Roles & permissions](roles.md) — media access reuses content read/write
  permissions.
