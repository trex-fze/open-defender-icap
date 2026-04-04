ALTER TABLE page_contents
    ADD COLUMN IF NOT EXISTS source_url text,
    ADD COLUMN IF NOT EXISTS resolved_url text,
    ADD COLUMN IF NOT EXISTS attempt_summary text;
