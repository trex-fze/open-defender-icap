#!/usr/bin/env bash
set -euo pipefail

# Stage 9 integration smoke: ensure that an ingest event produces a page-fetch job,
# the page-fetcher stores Crawl4AI content, and the Admin API/odctl surface exposes it.

: "${INGEST_URL:=http://event-ingester:19100}"
: "${INGEST_SECRET:=changeme-ingest}"
: "${ADMIN_URL:=http://admin-api:19000}"
: "${PAGE_FETCH_TARGET:=http://admin-api:19000/health/ready}"
: "${PAGE_FETCH_NORMALIZED_KEY:=domain:admin-api}"
: "${PAGE_FETCH_POLL_INTERVAL:=5}"
: "${PAGE_FETCH_MAX_ATTEMPTS:=12}"
: "${ODCTL_BIN:=odctl}"

RUN_ID=$(date +%s)
TRACE_ID="pagefetch-${RUN_ID}"

if [[ "${PAGE_FETCH_TARGET}" == *"?"* ]]; then
  TARGET_URL="${PAGE_FETCH_TARGET}&pf=${RUN_ID}"
else
  TARGET_URL="${PAGE_FETCH_TARGET}?pf=${RUN_ID}"
fi

echo "[PageFetch] Sending synthetic event to ${INGEST_URL}/ingest/filebeat (trace ${TRACE_ID})"
payload=$(cat <<JSON
{
  "events": [
    {
      "@timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
      "message": "page fetch flow",
      "trace_id": "${TRACE_ID}",
      "url": {"full": "${TARGET_URL}"},
      "source": {"ip": "172.30.40.50"},
      "od": {
        "integration_test": "page-fetch-flow",
        "environment": "ci",
        "service": "tests"
      }
    }
  ]
}
JSON
)

http_code=$(curl -sS -o /tmp/page-fetch-ingest.log -w "%{http_code}" \
  -H 'Content-Type: application/json' \
  -H "X-Filebeat-Secret: ${INGEST_SECRET}" \
  -d "${payload}" \
  "${INGEST_URL}/ingest/filebeat")

if [[ "${http_code}" != "202" && "${http_code}" != "200" ]]; then
  echo "[PageFetch] ingest request failed (status=${http_code})" >&2
  cat /tmp/page-fetch-ingest.log >&2
  exit 1
fi

start_epoch=$(date -u +%s)
echo "[PageFetch] Waiting for page contents (key=${PAGE_FETCH_NORMALIZED_KEY})"

success=0
content_json=""
for attempt in $(seq 1 "${PAGE_FETCH_MAX_ATTEMPTS}"); do
  if content_json=$(${ODCTL_BIN} page show --key "${PAGE_FETCH_NORMALIZED_KEY}" --json 2>/dev/null); then
    status=$(jq -r '.fetch_status // empty' <<<"${content_json}")
    excerpt=$(jq -r '.excerpt // empty' <<<"${content_json}")
    fetched_at=$(jq -r '.fetched_at // empty' <<<"${content_json}")
    version=$(jq -r '.fetch_version // 0' <<<"${content_json}")
    fetched_epoch=0
    if [[ -n "${fetched_at}" ]]; then
      fetched_epoch=$(date -u -d "${fetched_at}" +%s 2>/dev/null || echo 0)
    fi
    if [[ "${status}" == "ok" && -n "${excerpt}" && "${version}" != "0" && ${fetched_epoch} -ge ${start_epoch} ]]; then
      success=1
      echo "[PageFetch] received version ${version} at ${fetched_at}"
      break
    fi
  fi
  sleep "${PAGE_FETCH_POLL_INTERVAL}"
done

if [[ "${success}" != "1" ]]; then
  echo "[PageFetch] Timed out waiting for page content" >&2
  exit 1
fi

echo "[PageFetch] Verifying history endpoint"
history_json=$(${ODCTL_BIN} page history --key "${PAGE_FETCH_NORMALIZED_KEY}" --limit 5 --json 2>/dev/null)
history_len=$(jq -r 'length' <<<"${history_json}")
if [[ "${history_len}" == "0" ]]; then
  echo "[PageFetch] History endpoint returned no entries" >&2
  exit 1
fi

latest_hash=$(jq -r '.[0].content_hash // empty' <<<"${history_json}")
if [[ -z "${latest_hash}" ]]; then
  echo "[PageFetch] History response missing latest hash" >&2
  exit 1
fi

echo "[PageFetch] Flow succeeded (key=${PAGE_FETCH_NORMALIZED_KEY}, hash=${latest_hash})"
