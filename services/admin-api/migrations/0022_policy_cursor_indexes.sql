CREATE INDEX IF NOT EXISTS policies_created_id_idx
    ON policies (created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS policy_versions_policy_created_id_idx
    ON policy_versions (policy_id, created_at DESC, id DESC);
