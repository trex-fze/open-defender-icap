ALTER TABLE iam_service_accounts
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS iam_service_accounts_expires_at_idx
    ON iam_service_accounts (expires_at)
    WHERE status = 'active';
