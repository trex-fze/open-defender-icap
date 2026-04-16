WITH ranked_active_overrides AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY scope_type, scope_value
            ORDER BY updated_at DESC, created_at DESC, id DESC
        ) AS row_rank
    FROM overrides
    WHERE status = 'active'
)
UPDATE overrides AS o
SET
    status = 'revoked',
    updated_at = NOW()
FROM ranked_active_overrides AS r
WHERE o.id = r.id
  AND r.row_rank > 1;

CREATE UNIQUE INDEX IF NOT EXISTS overrides_active_scope_uidx
    ON overrides (scope_type, scope_value)
    WHERE status = 'active';
