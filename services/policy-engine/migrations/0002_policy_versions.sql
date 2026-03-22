CREATE TABLE IF NOT EXISTS policy_versions (
    id UUID PRIMARY KEY,
    policy_id UUID NOT NULL REFERENCES policies(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    created_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notes TEXT,
    rules JSONB NOT NULL,
    deployed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS policy_versions_policy_idx ON policy_versions (policy_id, created_at DESC);
