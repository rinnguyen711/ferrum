CREATE TABLE IF NOT EXISTS _components (
    uid          TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    fields       JSONB NOT NULL DEFAULT '[]'
);
