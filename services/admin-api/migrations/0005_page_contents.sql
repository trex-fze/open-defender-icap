CREATE TABLE IF NOT EXISTS page_contents (
    id UUID PRIMARY KEY,
    normalized_key TEXT NOT NULL,
    fetch_version INTEGER NOT NULL DEFAULT 1,
    content_type TEXT,
    content_hash TEXT,
    raw_bytes BYTEA,
    text_excerpt TEXT,
    char_count INTEGER,
    byte_count INTEGER,
    fetch_status TEXT NOT NULL,
    fetch_reason TEXT,
    ttl_seconds INTEGER NOT NULL DEFAULT 21600,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ GENERATED ALWAYS AS (fetched_at + (ttl_seconds || ' seconds')::INTERVAL) STORED
);

CREATE UNIQUE INDEX IF NOT EXISTS page_contents_norm_key_version_idx
    ON page_contents (normalized_key, fetch_version DESC);

CREATE INDEX IF NOT EXISTS page_contents_expires_idx
    ON page_contents (expires_at);
