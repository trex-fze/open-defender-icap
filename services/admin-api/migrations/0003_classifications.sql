CREATE TABLE IF NOT EXISTS classifications (
    id UUID PRIMARY KEY,
    normalized_key TEXT NOT NULL UNIQUE,
    taxonomy_version TEXT,
    model_version TEXT,
    primary_category TEXT,
    subcategory TEXT,
    risk_level TEXT,
    recommended_action TEXT,
    confidence NUMERIC(5,4),
    sfw BOOLEAN,
    flags JSONB,
    ttl_seconds INTEGER,
    status TEXT NOT NULL DEFAULT 'active',
    next_refresh_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS classification_versions (
    id UUID PRIMARY KEY,
    classification_id UUID NOT NULL REFERENCES classifications(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    changed_by TEXT,
    reason TEXT,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS classifications_status_idx ON classifications (status, updated_at DESC);
CREATE INDEX IF NOT EXISTS classification_versions_cls_idx ON classification_versions (classification_id, version DESC);
