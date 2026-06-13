CREATE TABLE IF NOT EXISTS _audit_log (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    action       TEXT        NOT NULL,
    category     TEXT        NOT NULL CHECK (category IN ('content', 'auth', 'settings', 'perm')),
    status       TEXT        NOT NULL DEFAULT 'success' CHECK (status IN ('success', 'failed')),
    actor_type   TEXT        NOT NULL CHECK (actor_type IN ('user', 'api_token', 'system')),
    actor_id     UUID,
    actor_label  TEXT        NOT NULL,
    target_type  TEXT,
    target_id    TEXT,
    target_label TEXT,
    changes      JSONB,
    note         TEXT,
    ip           TEXT,
    user_agent   TEXT,
    request_id   TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS _audit_log_recent      ON _audit_log (created_at DESC);
CREATE INDEX IF NOT EXISTS _audit_log_by_category ON _audit_log (category, created_at DESC);
CREATE INDEX IF NOT EXISTS _audit_log_by_actor    ON _audit_log (actor_id, created_at DESC);
CREATE INDEX IF NOT EXISTS _audit_log_by_target   ON _audit_log (target_type, target_id, created_at DESC);
