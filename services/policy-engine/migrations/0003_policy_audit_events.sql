CREATE TABLE IF NOT EXISTS policy_audit_events (
    id UUID PRIMARY KEY,
    policy_id UUID REFERENCES policies(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    actor TEXT,
    version TEXT,
    status TEXT,
    notes TEXT,
    diff JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS policy_audit_events_policy_idx
    ON policy_audit_events (policy_id, created_at DESC);
