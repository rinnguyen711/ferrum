CREATE TABLE IF NOT EXISTS _api_tokens (
  id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
  name         TEXT        NOT NULL,
  token_hash   TEXT        NOT NULL UNIQUE,
  scopes       TEXT[]      NOT NULL,
  expires_at   TIMESTAMPTZ,
  last_used_at TIMESTAMPTZ,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
