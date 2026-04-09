#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
COMPOSE_FILE=${COMPOSE_FILE:-"$ROOT_DIR/deploy/docker/docker-compose.yml"}
COMPOSE_ENV_FILE=${COMPOSE_ENV_FILE:-"$ROOT_DIR/.env"}
ADMIN_API_URL=${ADMIN_API_URL:-"http://localhost:19000"}
ADMIN_TOKEN=${ADMIN_TOKEN:-${OD_ADMIN_TOKEN:-}}
PATTERN_CLASS=${PATTERN_CLASS:-"domain:prompt-injection.%"}
PATTERN_SUB=${PATTERN_SUB:-"subdomain:prompt-injection.%"}
DRY_RUN=${DRY_RUN:-1}
PURGE_ALL_PENDING=${PURGE_ALL_PENDING:-0}

compose() {
  docker compose --env-file "$COMPOSE_ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

urlencode() {
  python3 - "$1" <<'PY'
import sys, urllib.parse
print(urllib.parse.quote(sys.argv[1], safe=''))
PY
}

log() {
  printf '[cleanup-synthetic-pending] %s\n' "$*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'missing required command: %s\n' "$1" >&2
    exit 1
  }
}

require_cmd docker
require_cmd curl
require_cmd python3

if [[ -z "$ADMIN_TOKEN" ]]; then
  ADMIN_TOKEN=$(compose exec -T admin-api printenv OD_ADMIN_TOKEN 2>/dev/null || true)
fi

if [[ -z "$ADMIN_TOKEN" ]]; then
  printf 'ADMIN_TOKEN is required (set ADMIN_TOKEN/OD_ADMIN_TOKEN or ensure admin-api exposes OD_ADMIN_TOKEN)\n' >&2
  exit 1
fi

SQL_KEYS="SELECT normalized_key FROM classifications WHERE normalized_key LIKE '${PATTERN_CLASS}' OR normalized_key LIKE '${PATTERN_SUB}'
UNION
SELECT normalized_key FROM classification_requests WHERE normalized_key LIKE '${PATTERN_CLASS}' OR normalized_key LIKE '${PATTERN_SUB}'
ORDER BY 1;"

KEYS=()
while IFS= read -r line; do
  [[ -n "$line" ]] && KEYS+=("$line")
done < <(compose exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"${SQL_KEYS}\"")

if [[ ${#KEYS[@]} -eq 0 ]]; then
  log "No synthetic keys matched (${PATTERN_CLASS}, ${PATTERN_SUB}). Nothing to do."
  exit 0
fi

log "Matched ${#KEYS[@]} synthetic keys"
for key in "${KEYS[@]}"; do
  log " - ${key}"
done

if [[ "$DRY_RUN" == "1" ]]; then
  log "DRY_RUN=1, no deletes executed."
  log "To execute: DRY_RUN=0 $0"
  exit 0
fi

if [[ "$PURGE_ALL_PENDING" == "1" ]]; then
  log "Purging entire pending queue via DELETE /api/v1/classifications/pending"
  curl -fsS -X DELETE \
    -H "X-Admin-Token: ${ADMIN_TOKEN}" \
    "${ADMIN_API_URL}/api/v1/classifications/pending" >/dev/null
fi

deleted=0
for key in "${KEYS[@]}"; do
  encoded=$(urlencode "$key")
  status=$(curl -sS -o /tmp/cleanup-key-response.json -w "%{http_code}" \
    -X DELETE \
    -H "X-Admin-Token: ${ADMIN_TOKEN}" \
    "${ADMIN_API_URL}/api/v1/classifications/${encoded}")
  if [[ "$status" == "204" || "$status" == "200" ]]; then
    deleted=$((deleted + 1))
    log "deleted ${key}"
  else
    log "failed ${key} (status=${status})"
  fi
done

SQL_REMAINING="SELECT count(*) FROM classifications WHERE normalized_key LIKE '${PATTERN_CLASS}' OR normalized_key LIKE '${PATTERN_SUB}';
SELECT count(*) FROM classification_requests WHERE normalized_key LIKE '${PATTERN_CLASS}' OR normalized_key LIKE '${PATTERN_SUB}';"

COUNTS=()
while IFS= read -r line; do
  COUNTS+=("$line")
done < <(compose exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"${SQL_REMAINING}\"")
remaining_class=${COUNTS[0]:-0}
remaining_pending=${COUNTS[1]:-0}

log "Deleted ${deleted}/${#KEYS[@]} keys"
log "Remaining classifications: ${remaining_class}"
log "Remaining pending rows: ${remaining_pending}"

if [[ "$remaining_class" != "0" || "$remaining_pending" != "0" ]]; then
  log "Some synthetic rows remain. Re-run or inspect API/log output."
  exit 2
fi

log "Cleanup complete."
