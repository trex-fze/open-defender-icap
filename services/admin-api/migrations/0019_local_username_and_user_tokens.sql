ALTER TABLE iam_users
    ALTER COLUMN email DROP NOT NULL;

UPDATE iam_users
SET email = NULL
WHERE email IS NOT NULL AND BTRIM(email) = '';

DROP INDEX IF EXISTS iam_users_email_lower_idx;
CREATE UNIQUE INDEX IF NOT EXISTS iam_users_email_lower_uidx
    ON iam_users (LOWER(email))
    WHERE email IS NOT NULL;

ALTER TABLE iam_users
    DROP CONSTRAINT IF EXISTS iam_users_identity_not_empty;

ALTER TABLE iam_users
    ADD CONSTRAINT iam_users_identity_not_empty
    CHECK (NULLIF(BTRIM(COALESCE(username, '')), '') IS NOT NULL OR NULLIF(BTRIM(COALESCE(email, '')), '') IS NOT NULL);

CREATE TABLE IF NOT EXISTS iam_user_tokens (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES iam_users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    token_hint TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    CONSTRAINT iam_user_tokens_status_check CHECK (status IN ('active', 'disabled'))
);

CREATE UNIQUE INDEX IF NOT EXISTS iam_user_tokens_user_name_uidx
    ON iam_user_tokens (user_id, LOWER(name));

CREATE INDEX IF NOT EXISTS iam_user_tokens_hint_idx
    ON iam_user_tokens (token_hint);
