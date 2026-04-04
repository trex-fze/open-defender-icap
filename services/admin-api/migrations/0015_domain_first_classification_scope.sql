-- Promote subdomain-scoped artifacts to domain-scoped keys for domain-first classification mode.

WITH promoted_requests AS (
    SELECT DISTINCT ON (domain_key)
        domain_key,
        status,
        base_url,
        last_error,
        requested_at,
        updated_at
    FROM (
        SELECT
            CONCAT('domain:', regexp_replace(split_part(normalized_key, ':', 2), '^.*?([^.]+\.[^.]+)$', '\1')) AS domain_key,
            status,
            base_url,
            last_error,
            requested_at,
            updated_at
        FROM classification_requests
        WHERE normalized_key LIKE 'subdomain:%'
    ) collapsed
    ORDER BY domain_key, updated_at DESC
)
INSERT INTO classification_requests (normalized_key, status, base_url, last_error, requested_at, updated_at)
SELECT domain_key, status, base_url, last_error, requested_at, updated_at
FROM promoted_requests
ON CONFLICT (normalized_key)
DO UPDATE SET
    status = EXCLUDED.status,
    base_url = COALESCE(EXCLUDED.base_url, classification_requests.base_url),
    last_error = EXCLUDED.last_error,
    updated_at = EXCLUDED.updated_at
WHERE classification_requests.updated_at <= EXCLUDED.updated_at;

DELETE FROM classification_requests WHERE normalized_key LIKE 'subdomain:%';

CREATE TEMP TABLE promoted_classifications AS
SELECT DISTINCT ON (domain_key)
    id,
    domain_key,
    taxonomy_version,
    model_version,
    primary_category,
    subcategory,
    risk_level,
    recommended_action,
    confidence,
    sfw,
    flags,
    ttl_seconds,
    status,
    next_refresh_at,
    created_at,
    updated_at
FROM (
    SELECT
        id,
        CONCAT('domain:', regexp_replace(split_part(normalized_key, ':', 2), '^.*?([^.]+\.[^.]+)$', '\1')) AS domain_key,
        taxonomy_version,
        model_version,
        primary_category,
        subcategory,
        risk_level,
        recommended_action,
        confidence,
        sfw,
        flags,
        ttl_seconds,
        status,
        next_refresh_at,
        created_at,
        updated_at
    FROM classifications
    WHERE normalized_key LIKE 'subdomain:%'
) collapsed
ORDER BY domain_key, updated_at DESC;

DELETE FROM classifications WHERE normalized_key LIKE 'subdomain:%';

INSERT INTO classifications (
    id,
    normalized_key,
    taxonomy_version,
    model_version,
    primary_category,
    subcategory,
    risk_level,
    recommended_action,
    confidence,
    sfw,
    flags,
    ttl_seconds,
    status,
    next_refresh_at,
    created_at,
    updated_at
)
SELECT
    id,
    domain_key,
    taxonomy_version,
    model_version,
    primary_category,
    subcategory,
    risk_level,
    recommended_action,
    confidence,
    sfw,
    flags,
    ttl_seconds,
    status,
    next_refresh_at,
    created_at,
    updated_at
FROM promoted_classifications
ON CONFLICT (normalized_key)
DO UPDATE SET
    taxonomy_version = EXCLUDED.taxonomy_version,
    model_version = EXCLUDED.model_version,
    primary_category = EXCLUDED.primary_category,
    subcategory = EXCLUDED.subcategory,
    risk_level = EXCLUDED.risk_level,
    recommended_action = EXCLUDED.recommended_action,
    confidence = EXCLUDED.confidence,
    sfw = EXCLUDED.sfw,
    flags = EXCLUDED.flags,
    ttl_seconds = EXCLUDED.ttl_seconds,
    status = EXCLUDED.status,
    next_refresh_at = EXCLUDED.next_refresh_at,
    updated_at = EXCLUDED.updated_at
WHERE classifications.updated_at <= EXCLUDED.updated_at;

DROP TABLE promoted_classifications;
