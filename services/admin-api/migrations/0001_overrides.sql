CREATE TABLE IF NOT EXISTS overrides (
    id UUID PRIMARY KEY,
    scope_type TEXT NOT NULL,
    scope_value TEXT NOT NULL,
    action TEXT NOT NULL,
    reason TEXT,
    created_by TEXT,
    expires_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS overrides_scope_idx ON overrides (scope_type, scope_value);
CREATE INDEX IF NOT EXISTS overrides_status_idx ON overrides (status, expires_at);

CREATE TABLE IF NOT EXISTS review_queue (
    id UUID PRIMARY KEY,
    normalized_key TEXT NOT NULL,
    request_metadata JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    submitter TEXT,
    assigned_to TEXT,
    decided_by TEXT,
    decision_notes TEXT,
    decision_action TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS review_queue_status_idx ON review_queue (status, created_at);
