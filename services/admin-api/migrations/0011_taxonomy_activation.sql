CREATE TABLE IF NOT EXISTS taxonomy_activation_profiles (
    id UUID PRIMARY KEY,
    version TEXT NOT NULL,
    updated_by TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS taxonomy_activation_entries (
    profile_id UUID NOT NULL REFERENCES taxonomy_activation_profiles(id) ON DELETE CASCADE,
    category_id TEXT NOT NULL,
    subcategory_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL,
    PRIMARY KEY (profile_id, category_id, subcategory_id)
);

CREATE INDEX IF NOT EXISTS taxonomy_activation_entries_category_idx
    ON taxonomy_activation_entries (category_id);
