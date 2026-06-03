# Media Library — Backend Design

**Date:** 2026-06-03
**Scope:** Backend only (storage abstraction, providers, DB tables, HTTP API). The
settings UI page and the media browser UI are separate specs built after this.
**Status:** Approved (brainstorm)

## Goal

A standalone Media Library for rustapi. Users store assets in nested folders or at
root, create folders, and upload assets. Each asset carries metadata: file name,
alternative text, caption. Storage is pluggable behind a `StorageProvider` trait;
ship **local filesystem** (default) and **S3** providers. Adding future providers
(R2, GCS, Azure) must require only a new provider file — no migration, no UI churn.

Media is **standalone** in this phase: content types do NOT reference assets. A
future `Media` field kind is explicitly out of scope.

## Key Decisions

- **Backend first.** This spec is the trait, providers, DB, and HTTP API only.
- **Standalone library.** No content-type integration yet.
- **Proxy upload.** Browser → rustapi server → provider. Provider-agnostic, simple.
- **Proxy serve.** `GET .../raw` streams bytes from provider through the server,
  admin-auth gated. No public URLs / redirects in v1.
- **`rust-s3` crate** for the S3 impl, wrapped behind the trait.
- **Local filesystem is the default provider.** Zero config, no secrets, works on
  first boot — matches the README "quickest demo" ethos. S3 is the first pluggable
  provider, proving the trait extends to remote + secret-bearing storage.
- **Self-describing providers.** Each provider declares its config field schema;
  the API exposes it so UI renders forms generically (mirrors the schema-driven
  content-type builder). New provider = zero UI changes.
- **Per-asset provider + storage_key.** Switching the active provider is safe: old
  assets stay readable on the provider that holds them; new uploads use the new one.
- **Block folder delete unless empty.** No surprise byte loss (409).
- **Secrets encrypted at rest.** Only providers that declare `secret` fields need
  this (S3 yes, local no). Key from dedicated `RUSTAPI_SECRET_KEY` env.
- **Env override wins** over DB settings, for ops lock-down.

## Architecture & Crate Placement

New crate **`crates/media`**:

- `provider/mod.rs` — `StorageProvider` trait, `StorageError`.
- `provider/local.rs` — filesystem impl (default).
- `provider/s3.rs` — `rust-s3` impl.
- `registry.rs` — provider id → factory + self-describing `ProviderDescriptor`.
- `config.rs` — config (de)serialization, secret encryption/decryption.

Changes elsewhere:

- `crates/http/src/routes/media.rs` — endpoints, merged into the protected router
  in `routes/mod.rs` (matches existing `/admin/*` pattern).
- `crates/schema/migrations/0003_media.sql` — tables.
- `crates/sql` — DML for folders/assets following the existing storage-layer style.

Rationale: storage logic isolated from HTTP and content domain. Trait + impls are
testable standalone. Adding a provider = one file in `crates/media`.

## StorageProvider Trait

```rust
#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Store bytes at key. Key chosen by caller (the asset's storage_key).
    async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<(), StorageError>;
    /// Fetch bytes for key.
    async fn get(&self, key: &str) -> Result<Bytes, StorageError>;
    /// Remove object. Idempotent (missing object = Ok).
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    /// Cheap connectivity/credential check for the settings "Test" button.
    async fn test(&self) -> Result<(), StorageError>;
}
```

- **Bytes in/out** (not streams) for v1 — admin uploads are modest. Streaming can
  be added later without changing call sites materially.
- **Caller picks `key`** — stored as `media_assets.storage_key`. Format:
  `{uuid}/{original_filename}` (uuid namespace avoids collisions, keeps a readable
  suffix).
- `StorageError` variants: `NotFound`, `Auth`, `Connection`, `Io`, `Other`.

### Registry & self-describing schema

```rust
pub struct ConfigField {
    pub name: &'static str,
    pub label: &'static str,
    pub r#type: &'static str,   // "string" for now
    pub required: bool,
    pub secret: bool,
}
pub struct ProviderDescriptor {
    pub id: &'static str,        // "local" | "s3"
    pub label: &'static str,
    pub fields: Vec<ConfigField>,
}
pub trait ProviderFactory {
    fn descriptor() -> ProviderDescriptor;
    fn build(config: &serde_json::Value) -> Result<Box<dyn StorageProvider>, StorageError>;
}
```

Provider field schemas:

- **local**: `base_dir` (string, required), `public_url` (string, required) — kept
  for future direct-serve; v1 serves via proxy regardless.
- **s3**: `bucket` (required), `region` (required), `endpoint` (optional, for
  S3-compatible: MinIO/R2/Spaces), `access_key` (required), `secret_key` (required,
  **secret**).

`GET /admin/media/providers` serializes all descriptors. `PUT settings` validates
posted config against the chosen descriptor, then `build`s.

## Data Model — `0003_media.sql`

System tables use the `_` prefix and `CREATE TABLE IF NOT EXISTS`, matching
`_users` / `_content_types`.

```sql
CREATE TABLE IF NOT EXISTS _media_folders (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_id   UUID REFERENCES _media_folders(id),   -- NULL = root
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (parent_id, name)                           -- no dup names per folder
);

CREATE TABLE IF NOT EXISTS _media_assets (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    folder_id          UUID REFERENCES _media_folders(id),  -- NULL = root
    provider           TEXT NOT NULL,        -- which provider holds the bytes
    storage_key        TEXT NOT NULL,        -- where in that provider
    file_name          TEXT NOT NULL,        -- display name (editable)
    alt_text           TEXT,
    caption            TEXT,
    mime_type          TEXT NOT NULL,
    size_bytes         BIGINT NOT NULL,
    width              INTEGER,              -- images only
    height             INTEGER,             -- images only
    original_filename  TEXT NOT NULL,
    checksum           TEXT,                 -- sha256 hex, integrity/future dedup
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS _media_assets_folder_idx ON _media_assets(folder_id);

CREATE TABLE IF NOT EXISTS _media_settings (
    id          BOOLEAN PRIMARY KEY DEFAULT TRUE,    -- singleton row
    provider    TEXT NOT NULL,
    config      JSONB NOT NULL,    -- secret fields encrypted in-place
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT _media_settings_singleton CHECK (id)
);
```

Notes:

- **Per-asset `provider` + `storage_key`** → provider switch is non-destructive.
- **Singleton settings** via `BOOLEAN PRIMARY KEY CHECK(id)` → exactly one row.
- **Folder delete** blocked at app layer unless no child folders and no assets (409).
- `gen_random_uuid()` requires the `pgcrypto` extension already enabled in `0001`.

## HTTP Endpoints — `routes/media.rs`

All under `/admin`, auth-gated, merged into the protected router.

### Settings / providers
- `GET  /admin/media/providers` — list descriptors (id, label, fields w/ secret flags).
- `GET  /admin/media/settings` — `{provider, config}`; secret fields masked `"••••"`.
- `PUT  /admin/media/settings` — validate config vs descriptor, encrypt secrets,
  persist, swap active provider in `AppState`.
- `POST /admin/media/settings/test` — build provider from posted config, call
  `.test()`, return 200 or 4xx with the error reason.

### Folders
- `GET    /admin/media/folders` — list; `?parent_id=` filter (omit = root level).
- `POST   /admin/media/folders` — `{parent_id?, name}`.
- `PATCH  /admin/media/folders/:id` — rename and/or move (change `parent_id`).
- `DELETE /admin/media/folders/:id` — 409 if non-empty.

### Assets
- `GET    /admin/media/assets?folder_id=` — list metadata in a folder (omit = root).
- `POST   /admin/media/assets` — multipart: `file` + `folder_id?`. Server reads
  bytes, sniffs mime, computes sha256, extracts image dimensions if image,
  generates `storage_key`, calls `provider.put()`, inserts the row, returns the asset.
- `GET    /admin/media/assets/:id` — metadata.
- `PATCH  /admin/media/assets/:id` — edit `file_name`, `alt_text`, `caption`,
  `folder_id` (move).
- `DELETE /admin/media/assets/:id` — `provider.delete()` then drop the row.
- `GET    /admin/media/assets/:id/raw` — proxy bytes from the asset's provider with
  the correct `Content-Type`.

## Config Resolution, Secrets, Errors

### Active provider resolution (boot, and after `PUT settings`)
1. **Env wins**: if `RUSTAPI_MEDIA_PROVIDER` is set, build from
   `RUSTAPI_MEDIA_*` / `RUSTAPI_S3_*` env vars (ops lock-down).
2. Else the `_media_settings` row, if present.
3. Else **default = local**, `base_dir = ./media-data`, served via proxy.

Held in `AppState` as the active `Arc<dyn StorageProvider>`, swapped on PUT
(via `RwLock` or `arc-swap`).

### Secrets
- Descriptor marks fields `secret: true`.
- On save: encrypt those values before the JSONB write (AES-GCM; add `aes-gcm`
  dep). Key = `RUSTAPI_SECRET_KEY` env (dedicated, separate from the JWT secret).
- On `GET settings`: secret fields masked `"••••"`, never returned decrypted.
- On build/use: decrypt in memory only.
- Missing `RUSTAPI_SECRET_KEY` while saving/using a provider that declares secrets
  → clear, explicit error at save/startup.

### Error mapping
`StorageError` → HTTP, reusing the existing `crates/http/src/error.rs` pattern:
`NotFound`→404, `Auth`/`Connection`→502 (or 400 on the test endpoint), config
validation→400, non-empty folder delete→409.

## Testing

- **`crates/media` unit:** local provider round-trip (put/get/delete) in a tempdir;
  config validation against descriptors; secret encrypt/decrypt round-trip;
  descriptor serialization.
- **S3 impl:** integration test gated behind env (MinIO), skipped by default in CI.
- **HTTP:** folder CRUD; non-empty folder delete → 409; asset upload → `/raw`
  round-trip on the local provider; `PUT settings` masks secrets on read-back.
  Match the existing http test style.

## Out of Scope (future specs)

- Settings UI page and media browser UI.
- `Media` content-type field kind.
- Presigned direct upload/download; public-URL serve.
- Streaming uploads; image thumbnail/variant generation; dedup by checksum.
