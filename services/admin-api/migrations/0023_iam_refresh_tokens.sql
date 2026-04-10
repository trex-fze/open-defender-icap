CREATE TABLE IF NOT EXISTS iam_refresh_tokens (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES iam_users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    token_hint TEXT,
    token_version BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,
    replaced_by_id UUID REFERENCES iam_refresh_tokens(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS iam_refresh_tokens_user_idx
    ON iam_refresh_tokens (user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS iam_refresh_tokens_active_idx
    ON iam_refresh_tokens (user_id, expires_at)
    WHERE revoked_at IS NULL;
