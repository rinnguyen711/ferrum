CREATE TABLE IF NOT EXISTS _webhooks (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT        NOT NULL,
    url        TEXT        NOT NULL,
    events     TEXT[]      NOT NULL,
    secret     TEXT,
    enabled    BOOLEAN     NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS _webhook_deliveries (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id  UUID        NOT NULL REFERENCES _webhooks(id) ON DELETE CASCADE,
    event       TEXT        NOT NULL,
    payload     JSONB       NOT NULL,
    status      TEXT        NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'success', 'failed')),
    attempt     INT         NOT NULL DEFAULT 0,
    next_try_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS _webhook_deliveries_poll
    ON _webhook_deliveries (status, next_try_at)
    WHERE status = 'pending';
