CREATE TABLE IF NOT EXISTS "_locales" (
    "code"       text PRIMARY KEY,
    "name"       text NOT NULL,
    "is_default" boolean NOT NULL DEFAULT false,
    "position"   int NOT NULL DEFAULT 0
);

-- Seed the default locale. Exactly one row must have is_default = true; the
-- application layer enforces that invariant on mutations.
INSERT INTO "_locales" ("code", "name", "is_default", "position")
VALUES ('en', 'English', true, 0)
ON CONFLICT ("code") DO NOTHING;

-- At most one default (partial unique index).
CREATE UNIQUE INDEX IF NOT EXISTS "_locales_one_default"
    ON "_locales" (("is_default")) WHERE "is_default";
