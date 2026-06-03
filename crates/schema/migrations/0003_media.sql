CREATE TABLE IF NOT EXISTS _media_folders (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_id   UUID REFERENCES _media_folders(id),
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (parent_id, name)
);

CREATE TABLE IF NOT EXISTS _media_assets (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    folder_id          UUID REFERENCES _media_folders(id),
    provider           TEXT NOT NULL,
    storage_key        TEXT NOT NULL,
    file_name          TEXT NOT NULL,
    alt_text           TEXT,
    caption            TEXT,
    mime_type          TEXT NOT NULL,
    size_bytes         BIGINT NOT NULL,
    width              INTEGER,
    height             INTEGER,
    original_filename  TEXT NOT NULL,
    checksum           TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS _media_assets_folder_idx ON _media_assets (folder_id);

CREATE TABLE IF NOT EXISTS _media_settings (
    id          BOOLEAN PRIMARY KEY DEFAULT TRUE,
    provider    TEXT NOT NULL,
    config      JSONB NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT _media_settings_singleton CHECK (id)
);
