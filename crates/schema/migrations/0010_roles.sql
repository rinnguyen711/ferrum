CREATE TABLE IF NOT EXISTS _roles (
    key         TEXT        PRIMARY KEY,
    name        TEXT        NOT NULL,
    description TEXT        NOT NULL DEFAULT '',
    color       TEXT        NOT NULL DEFAULT '#52525B',
    is_system   BOOLEAN     NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS _role_permissions (
    role_key     TEXT NOT NULL REFERENCES _roles(key) ON DELETE CASCADE,
    content_type TEXT NOT NULL,
    action       TEXT NOT NULL,
    PRIMARY KEY (role_key, content_type, action)
);

-- System roles. Locked in API/UI. `admin` keeps a code-level all-access
-- short-circuit, so it needs no permission rows. `editor`/`viewer` enforcement
-- also stays code-level via role_allows; these rows exist so the roles list/UI
-- shows them and custom roles can be distinguished by `is_system`.
INSERT INTO _roles (key, name, description, color, is_system) VALUES
    ('admin',  'Admin',  'Full access to content, schema, and users.', '#D14D2B', true),
    ('editor', 'Editor', 'Read and write content entries.',            '#2B6CD1', true),
    ('viewer', 'Viewer', 'Read-only access to content.',               '#52525B', true)
ON CONFLICT (key) DO NOTHING;
