WITH ranked_page_contents AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY normalized_key
            ORDER BY fetch_version DESC, fetched_at DESC, id DESC
        ) AS row_rank
    FROM page_contents
)
DELETE FROM page_contents AS pc
USING ranked_page_contents AS ranked
WHERE pc.id = ranked.id
  AND ranked.row_rank > 30;
