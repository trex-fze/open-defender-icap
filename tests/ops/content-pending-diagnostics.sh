#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
COMPOSE_FILE=${COMPOSE_FILE:-"$ROOT_DIR/deploy/docker/docker-compose.yml"}
COMPOSE_ENV_FILE=${COMPOSE_ENV_FILE:-"$ROOT_DIR/.env"}
NORMALIZED_KEY=${NORMALIZED_KEY:-}
HOST_TAG=${HOST_TAG:-unknown}
OUT_DIR=${OUT_DIR:-"$ROOT_DIR/tests/artifacts/ops-triage/content-pending-$(date +%Y%m%d%H%M%S)"}

if [[ -z "$NORMALIZED_KEY" ]]; then
  printf 'ERROR: NORMALIZED_KEY is required (example: domain:example.com)\n' >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

compose() {
  docker compose --env-file "$COMPOSE_ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

psql_admin() {
  local query="$1"
  compose exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"${query}\""
}

safe_key=$(printf '%s' "$NORMALIZED_KEY" | tr ':/' '__')

psql_admin "SELECT normalized_key, status, base_url, requested_at, updated_at, last_error FROM classification_requests WHERE normalized_key='${NORMALIZED_KEY}' ORDER BY updated_at DESC LIMIT 10" >"$OUT_DIR/${safe_key}-classification_requests.txt" || true
psql_admin "SELECT normalized_key, fetch_status, fetch_reason, fetch_version, fetched_at FROM page_contents WHERE normalized_key='${NORMALIZED_KEY}' ORDER BY fetch_version DESC LIMIT 10" >"$OUT_DIR/${safe_key}-page_contents.txt" || true
psql_admin "SELECT normalized_key, recommended_action, primary_category, subcategory, updated_at FROM classifications WHERE normalized_key='${NORMALIZED_KEY}' ORDER BY updated_at DESC LIMIT 10" >"$OUT_DIR/${safe_key}-classifications.txt" || true

compose logs --no-color --tail=400 llm-worker >"$OUT_DIR/${safe_key}-llm-worker.log" || true
compose logs --no-color --tail=400 page-fetcher >"$OUT_DIR/${safe_key}-page-fetcher.log" || true
compose logs --no-color --tail=400 reclass-worker >"$OUT_DIR/${safe_key}-reclass-worker.log" || true
compose logs --no-color --tail=400 admin-api >"$OUT_DIR/${safe_key}-admin-api.log" || true
compose logs --no-color --tail=400 icap-adaptor >"$OUT_DIR/${safe_key}-icap-adaptor.log" || true

grep -F "$NORMALIZED_KEY" "$OUT_DIR/${safe_key}-llm-worker.log" >"$OUT_DIR/${safe_key}-llm-worker-${HOST_TAG}.log" || true
grep -F "$NORMALIZED_KEY" "$OUT_DIR/${safe_key}-page-fetcher.log" >"$OUT_DIR/${safe_key}-page-fetcher-${HOST_TAG}.log" || true
grep -F "$NORMALIZED_KEY" "$OUT_DIR/${safe_key}-admin-api.log" >"$OUT_DIR/${safe_key}-admin-api-${HOST_TAG}.log" || true

compose exec -T redis redis-cli XREVRANGE classification-jobs + - COUNT 200 >"$OUT_DIR/${safe_key}-classification-jobs.txt" || true
compose exec -T redis redis-cli XREVRANGE page-fetch-jobs + - COUNT 200 >"$OUT_DIR/${safe_key}-page-fetch-jobs.txt" || true

cat >"$OUT_DIR/README.txt" <<EOF
Content pending diagnostics bundle

normalized_key: ${NORMALIZED_KEY}
host_tag: ${HOST_TAG}
generated_at: $(date -Iseconds)

Key files:
- ${safe_key}-classification_requests.txt
- ${safe_key}-page_contents.txt
- ${safe_key}-classifications.txt
- ${safe_key}-llm-worker-${HOST_TAG}.log
- ${safe_key}-page-fetcher-${HOST_TAG}.log
- ${safe_key}-admin-api-${HOST_TAG}.log
EOF

printf 'Diagnostics collected: %s\n' "$OUT_DIR"
