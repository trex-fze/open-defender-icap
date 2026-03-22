CREATE TABLE IF NOT EXISTS taxonomy_categories (
    id UUID PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    default_action TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS taxonomy_subcategories (
    id UUID PRIMARY KEY,
    category_id UUID NOT NULL REFERENCES taxonomy_categories(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    default_action TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(category_id, name)
);

CREATE TABLE IF NOT EXISTS reclassification_jobs (
    id UUID PRIMARY KEY,
    normalized_key TEXT NOT NULL,
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS reclass_status_idx ON reclassification_jobs (status, created_at DESC);

CREATE TABLE IF NOT EXISTS cache_entries (
    cache_key TEXT PRIMARY KEY,
    value_json JSONB NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    source TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS cli_operation_logs (
    id UUID PRIMARY KEY,
    operator_id TEXT,
    command TEXT NOT NULL,
    args_hash TEXT,
    result TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS cli_operator_idx ON cli_operation_logs (operator_id, created_at DESC);

CREATE TABLE IF NOT EXISTS ui_action_audit (
    id UUID PRIMARY KEY,
    user_id TEXT,
    route TEXT NOT NULL,
    action TEXT NOT NULL,
    payload JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS ui_action_user_idx ON ui_action_audit (user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS reporting_aggregates (
    id UUID PRIMARY KEY,
    dimension TEXT NOT NULL,
    period_start TIMESTAMPTZ NOT NULL,
    metrics JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE UNIQUE INDEX IF NOT EXISTS reporting_dimension_period_idx ON reporting_aggregates (dimension, period_start);
