CREATE TABLE IF NOT EXISTS policies (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS policy_rules (
    id UUID PRIMARY KEY,
    policy_id UUID NOT NULL REFERENCES policies(id) ON DELETE CASCADE,
    priority INTEGER NOT NULL,
    action TEXT NOT NULL,
    description TEXT,
    conditions JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS policy_rules_policy_priority_idx
    ON policy_rules (policy_id, priority);
