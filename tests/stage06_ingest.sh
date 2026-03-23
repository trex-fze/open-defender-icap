#!/usr/bin/env bash
set -euo pipefail

# Stage 6 smoke test: pushes a synthetic event through the Filebeat → event-ingester → Elasticsearch path
# and verifies that the Admin API traffic report endpoint returns data.

: "${INGEST_URL:=http://localhost:19100}"
: "${INGEST_SECRET:=changeme-ingest}"
: "${ELASTIC_URL:=http://localhost:9200}"
: "${ELASTIC_USER:=elastic}"
: "${ELASTIC_PASS:=changeme-elastic}"
: "${ADMIN_URL:=http://localhost:19000}"
: "${INDEX_PATTERN:=traffic-events-*}"

echo "[Stage06] Sending synthetic event to ${INGEST_URL}/ingest/filebeat"
payload=$(cat <<'JSON'
{
  "events": [
    {
      "@timestamp": "2026-03-23T12:00:00Z",
      "message": "stage06 smoke",
      "url": {"full": "http://integration.test/smoke"},
      "source": {"ip": "10.20.30.40"},
      "category": "Testing",
      "recommended_action": "block",
      "od": {
        "environment": "dev",
        "service": "squid",
        "integration_test": "stage06-smoke"
      }
    }
  ]
}
JSON
)

curl -sS -o /dev/null -w "%{http_code}\n" \
  -H 'Content-Type: application/json' \
  -H "X-Filebeat-Secret: ${INGEST_SECRET}" \
  -d "${payload}" \
  "${INGEST_URL}/ingest/filebeat" | grep -E '202|200' >/dev/null

sleep "${INGEST_WAIT_SECONDS:-5}"

echo "[Stage06] Verifying document landed in Elasticsearch (${ELASTIC_URL})"
count=$(curl -sS -u "${ELASTIC_USER}:${ELASTIC_PASS}" \
  -H 'Content-Type: application/json' \
  -d '{"query":{"match":{"od.integration_test":"stage06-smoke"}}}' \
  "${ELASTIC_URL}/${INDEX_PATTERN}/_count" | jq -r '.count')

if [[ "${count}" -lt 1 ]]; then
  echo "Elasticsearch document count was ${count}, expected >=1" >&2
  exit 1
fi

echo "[Stage06] Querying Admin API traffic report"
curl -sS "${ADMIN_URL}/api/v1/reporting/traffic?range=1h&top_n=3" | jq '.' >/dev/null

echo "Stage 6 ingestion smoke succeeded (count=${count})"
