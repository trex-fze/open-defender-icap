ALTER TABLE iam_users
    ADD COLUMN IF NOT EXISTS token_version BIGINT NOT NULL DEFAULT 1;

UPDATE iam_users
SET token_version = 1
WHERE token_version IS NULL OR token_version < 1;
