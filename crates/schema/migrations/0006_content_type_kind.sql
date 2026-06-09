ALTER TABLE _content_types
    ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'collection'
        CHECK (kind IN ('collection', 'single'));
