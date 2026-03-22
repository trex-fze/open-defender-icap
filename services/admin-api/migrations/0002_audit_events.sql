CREATE TABLE IF NOT EXISTS audit_events (
    id UUID PRIMARY KEY,
    actor TEXT,
    action TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    payload JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS audit_events_action_idx ON audit_events (action, created_at DESC);
CREATE INDEX IF NOT EXISTS audit_events_target_idx ON audit_events (target_type, target_id);
