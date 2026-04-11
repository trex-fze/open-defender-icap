#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
COMPOSE_FILE=${COMPOSE_FILE:-"$ROOT_DIR/deploy/docker/docker-compose.yml"}
COMPOSE_ENV_FILE=${COMPOSE_ENV_FILE:-"$ROOT_DIR/.env"}
OUT_DIR=${OUT_DIR:-"$ROOT_DIR/tests/artifacts/ops-triage/platform-$(date +%Y%m%d%H%M%S)"}
REDACT=${REDACT:-1}

HOST_TAG=${HOST_TAG:-local}
ADMIN_API_URL=${ADMIN_API_URL:-http://localhost:19000}
POLICY_ENGINE_URL=${POLICY_ENGINE_URL:-http://localhost:19010}
EVENT_INGESTER_URL=${EVENT_INGESTER_URL:-http://localhost:19100}
LLM_METRICS_URL=${LLM_METRICS_URL:-http://localhost:19015/metrics}
PAGE_FETCH_METRICS_URL=${PAGE_FETCH_METRICS_URL:-http://localhost:19025/metrics}
ICAP_METRICS_URL=${ICAP_METRICS_URL:-http://localhost:19005/metrics}
ADMIN_TOKEN=${ADMIN_TOKEN:-${OD_ADMIN_TOKEN:-}}

LLM_STREAM=${LLM_STREAM:-classification-jobs}
LLM_GROUP=${LLM_GROUP:-llm-worker}
LLM_DLQ_STREAM=${LLM_DLQ_STREAM:-classification-jobs-dlq}
PAGE_STREAM=${PAGE_STREAM:-page-fetch-jobs}
PAGE_GROUP=${PAGE_GROUP:-page-fetcher}
PAGE_DLQ_STREAM=${PAGE_DLQ_STREAM:-page-fetch-jobs-dlq}

mkdir -p "$OUT_DIR"

compose() {
  docker compose --env-file "$COMPOSE_ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

capture_cmd() {
  local out_file="$1"
  shift
  {
    printf '$ %s\n' "$*"
    "$@"
  } >"$out_file" 2>&1 || true
}

capture_curl() {
  local out_file="$1"
  local url="$2"
  shift 2
  {
    printf '$ curl %s\n' "$url"
    curl -sS -i --max-time 15 "$@" "$url"
  } >"$out_file" 2>&1 || true
}

capture_psql() {
  local out_file="$1"
  local query="$2"
  capture_cmd "$out_file" compose exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"${query}\""
}

write_meta() {
  local meta="$OUT_DIR/README.txt"
  cat >"$meta" <<EOF
Platform diagnostics bundle

generated_at: $(date -Iseconds)
host_tag: ${HOST_TAG}
redact_mode: ${REDACT}

Captured domains:
- service health and metrics endpoints
- queue state (streams, groups, pending, DLQ heads)
- auth/session and IAM snapshots
- proxy/service log tails
- reporting snapshots
EOF
}

redact_file() {
  local path="$1"
  python3 - "$path" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
if not path.is_file():
    sys.exit(0)
text = path.read_text(errors="ignore")
patterns = [
    (r"(?i)(x-admin-token\s*:\s*)([^\r\n]+)", r"\1<redacted>"),
    (r"(?i)(authorization\s*:\s*bearer\s+)([^\r\n]+)", r"\1<redacted>"),
    (r"(?i)(api[_-]?key\s*[=:]\s*)([^\s,\"']+)", r"\1<redacted>"),
    (r"(?i)(password\s*[=:]\s*)([^\s,\"']+)", r"\1<redacted>"),
    (r"(?i)(postgres://[^:/\s]+:)([^@\s]+)(@)", r"\1<redacted>\3"),
]
for pattern, repl in patterns:
    text = re.sub(pattern, repl, text)
path.write_text(text)
PY
}

redact_all() {
  local f
  while IFS= read -r -d '' f; do
    redact_file "$f"
  done < <(find "$OUT_DIR" -type f -print0)
}

write_meta

capture_curl "$OUT_DIR/health-admin-api.txt" "$ADMIN_API_URL/health/ready"
capture_curl "$OUT_DIR/health-policy-engine.txt" "$POLICY_ENGINE_URL/health/ready"
capture_curl "$OUT_DIR/health-event-ingester.txt" "$EVENT_INGESTER_URL/health/ready"
capture_curl "$OUT_DIR/metrics-llm-worker.txt" "$LLM_METRICS_URL"
capture_curl "$OUT_DIR/metrics-page-fetcher.txt" "$PAGE_FETCH_METRICS_URL"
capture_curl "$OUT_DIR/metrics-icap-adaptor.txt" "$ICAP_METRICS_URL"

capture_cmd "$OUT_DIR/redis-xinfo-stream-classification.txt" compose exec -T redis redis-cli XINFO STREAM "$LLM_STREAM"
capture_cmd "$OUT_DIR/redis-xinfo-groups-classification.txt" compose exec -T redis redis-cli XINFO GROUPS "$LLM_STREAM"
capture_cmd "$OUT_DIR/redis-xpending-classification.txt" compose exec -T redis redis-cli XPENDING "$LLM_STREAM" "$LLM_GROUP"
capture_cmd "$OUT_DIR/redis-dlq-head-classification.txt" compose exec -T redis redis-cli XREVRANGE "$LLM_DLQ_STREAM" + - COUNT 100

capture_cmd "$OUT_DIR/redis-xinfo-stream-page-fetch.txt" compose exec -T redis redis-cli XINFO STREAM "$PAGE_STREAM"
capture_cmd "$OUT_DIR/redis-xinfo-groups-page-fetch.txt" compose exec -T redis redis-cli XINFO GROUPS "$PAGE_STREAM"
capture_cmd "$OUT_DIR/redis-xpending-page-fetch.txt" compose exec -T redis redis-cli XPENDING "$PAGE_STREAM" "$PAGE_GROUP"
capture_cmd "$OUT_DIR/redis-dlq-head-page-fetch.txt" compose exec -T redis redis-cli XREVRANGE "$PAGE_DLQ_STREAM" + - COUNT 100

capture_psql "$OUT_DIR/db-pending-by-status.txt" "SELECT status, count(*) FROM classification_requests GROUP BY status ORDER BY status;"
capture_psql "$OUT_DIR/db-pending-age-buckets.txt" "SELECT CASE WHEN requested_at < NOW() - INTERVAL '15 minutes' THEN 'gt_15m' WHEN requested_at < NOW() - INTERVAL '5 minutes' THEN 'gt_5m' ELSE 'le_5m' END AS bucket, count(*) FROM classification_requests GROUP BY bucket ORDER BY bucket;"
capture_psql "$OUT_DIR/db-recent-failures.txt" "SELECT normalized_key, status, last_error, updated_at FROM classification_requests WHERE status IN ('failed','waiting_content') ORDER BY updated_at DESC LIMIT 100;"

if [[ -n "$ADMIN_TOKEN" ]]; then
  capture_curl "$OUT_DIR/auth-whoami.txt" "$ADMIN_API_URL/api/v1/iam/whoami" -H "X-Admin-Token: ${ADMIN_TOKEN}"
  capture_curl "$OUT_DIR/reporting-dashboard-24h.txt" "$ADMIN_API_URL/api/v1/reporting/dashboard?range=24h" -H "X-Admin-Token: ${ADMIN_TOKEN}"
  capture_curl "$OUT_DIR/reporting-traffic-summary-24h.txt" "$ADMIN_API_URL/api/v1/reporting/traffic?range=24h" -H "X-Admin-Token: ${ADMIN_TOKEN}"
else
  printf 'ADMIN_TOKEN not provided; auth/reporting snapshots skipped\n' >"$OUT_DIR/auth-reporting-skipped.txt"
fi

capture_cmd "$OUT_DIR/logs-llm-worker.txt" compose logs --no-color --tail=500 llm-worker
capture_cmd "$OUT_DIR/logs-page-fetcher.txt" compose logs --no-color --tail=500 page-fetcher
capture_cmd "$OUT_DIR/logs-admin-api.txt" compose logs --no-color --tail=500 admin-api
capture_cmd "$OUT_DIR/logs-policy-engine.txt" compose logs --no-color --tail=500 policy-engine
capture_cmd "$OUT_DIR/logs-event-ingester.txt" compose logs --no-color --tail=500 event-ingester
capture_cmd "$OUT_DIR/logs-haproxy.txt" compose logs --no-color --tail=500 haproxy
capture_cmd "$OUT_DIR/logs-squid.txt" compose logs --no-color --tail=500 squid

if [[ "$REDACT" == "1" ]]; then
  redact_all
fi

printf 'Platform diagnostics collected: %s\n' "$OUT_DIR"
