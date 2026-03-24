CREATE TABLE IF NOT EXISTS classification_requests (
    normalized_key TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    base_url TEXT,
    last_error TEXT,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS classification_requests_status_idx
    ON classification_requests (status, updated_at DESC);
