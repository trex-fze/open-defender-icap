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
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS page_contents_norm_key_version_idx
    ON page_contents (normalized_key, fetch_version DESC);

CREATE INDEX IF NOT EXISTS page_contents_expires_idx
    ON page_contents (expires_at);

CREATE OR REPLACE FUNCTION page_contents_set_expiry()
RETURNS TRIGGER AS $$
BEGIN
    NEW.expires_at := COALESCE(NEW.fetched_at, NOW()) + make_interval(secs => NEW.ttl_seconds);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_page_contents_set_expiry ON page_contents;

CREATE TRIGGER trg_page_contents_set_expiry
BEFORE INSERT OR UPDATE ON page_contents
FOR EACH ROW EXECUTE FUNCTION page_contents_set_expiry();
