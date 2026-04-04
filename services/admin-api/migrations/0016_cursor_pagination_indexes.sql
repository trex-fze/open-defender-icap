CREATE INDEX IF NOT EXISTS classifications_updated_key_idx
    ON classifications (updated_at DESC, normalized_key DESC)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS classification_requests_status_updated_key_idx
    ON classification_requests (status, updated_at DESC, normalized_key DESC);

CREATE INDEX IF NOT EXISTS classification_requests_updated_key_idx
    ON classification_requests (updated_at DESC, normalized_key DESC);

CREATE INDEX IF NOT EXISTS overrides_created_id_idx
    ON overrides (created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS cli_created_id_idx
    ON cli_operation_logs (created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS cli_operator_created_id_idx
    ON cli_operation_logs (operator_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS iam_users_created_id_idx
    ON iam_users (created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS iam_groups_name_id_idx
    ON iam_groups (name ASC, id ASC);

CREATE INDEX IF NOT EXISTS iam_service_accounts_name_id_idx
    ON iam_service_accounts (name ASC, id ASC);

CREATE INDEX IF NOT EXISTS iam_audit_events_created_id_idx
    ON iam_audit_events (created_at DESC, id DESC);
