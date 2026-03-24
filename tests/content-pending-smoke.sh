#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMPOSE_FILE=${COMPOSE_FILE:-"$ROOT_DIR/deploy/docker/docker-compose.yml"}
ARTIFACT_DIR=${ARTIFACT_DIR:-"$ROOT_DIR/tests/artifacts/content-pending"}
TARGET_HOST=${TARGET_HOST:-smoke-origin}
TARGET_URL=${TARGET_URL:-"http://${TARGET_HOST}/"}
NORMALIZED_KEY="domain:${TARGET_HOST}"
ADMIN_TOKEN=${OD_ADMIN_TOKEN:-changeme-admin}
KEEP_STACK=0
BUILD_IMAGES=0
WAIT_HTTP_TRIES=${WAIT_HTTP_TRIES:-120}
PGUSER=${POSTGRES_USER:-defender}
PGADMIN_DB=${PGADMIN_DB:-defender_admin}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --keep-stack)
      KEEP_STACK=1
      shift
      ;;
    --build)
      BUILD_IMAGES=1
      shift
      ;;
    --target-host)
      TARGET_HOST="$2"
      TARGET_URL="http://${TARGET_HOST}/"
      NORMALIZED_KEY="domain:${TARGET_HOST}"
      shift 2
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

mkdir -p "$ARTIFACT_DIR"

log() {
  printf '[content-pending] %s\n' "$*"
}

die() {
  log "ERROR: $*"
  exit 1
}

compose() {
  docker compose -f "$COMPOSE_FILE" "$@"
}

start_stack() {
  if [[ $BUILD_IMAGES -eq 1 ]]; then
    compose up -d --build
  else
    compose up -d
  fi
}

stop_stack() {
  if [[ $KEEP_STACK -eq 0 ]]; then
    compose down >/dev/null 2>&1 || true
  fi
}

exec_pg() {
  local sql="$1"
  compose exec -T postgres bash -lc "psql -U ${PGUSER} -d ${PGADMIN_DB} -At -c \"${sql}\""
}

exec_redis() {
  compose exec -T redis redis-cli "$@"
}

exec_runner() {
  compose exec -T odctl-runner bash -lc "$1"
}

wait_for_http() {
  local url="$1"
  local name="$2"
  local scope="${3:-host}"
  local tries=$WAIT_HTTP_TRIES
  for ((i = 1; i <= tries; i++)); do
    if [[ "$scope" == "service" ]]; then
      if compose exec -T odctl-runner bash -lc "curl -sf $url >/dev/null"; then
        log "$name ready ($url)"
        return 0
      fi
    else
      if curl -sf "$url" >/dev/null; then
        log "$name ready ($url)"
        return 0
      fi
    fi
    sleep 2
  done
  die "Timed out waiting for $name at $url"
}

cleanup_state() {
  log "Cleaning previous rows for $NORMALIZED_KEY"
  exec_pg "DELETE FROM classification_requests WHERE normalized_key = '${NORMALIZED_KEY}'"
  exec_pg "DELETE FROM page_contents WHERE normalized_key = '${NORMALIZED_KEY}'"
  exec_pg "DELETE FROM classifications WHERE normalized_key = '${NORMALIZED_KEY}'"
  exec_redis DEL "$NORMALIZED_KEY" >/dev/null || true
}

verify_pending_entry() {
  local row
  row=$(exec_pg "SELECT status, base_url FROM classification_requests WHERE normalized_key = '${NORMALIZED_KEY}'")
  if [[ -z "$row" ]]; then
    die "Expected pending row for ${NORMALIZED_KEY}"
  fi
  printf '%s\n' "$row" >"$ARTIFACT_DIR/pending-before.txt"
}

wait_for_page_content() {
  log "Waiting for Crawl4AI/page-fetcher to store content"
  for ((i = 1; i <= 60; i++)); do
    local status
    status=$(exec_pg "SELECT fetch_status FROM page_contents WHERE normalized_key = '${NORMALIZED_KEY}' ORDER BY fetch_version DESC LIMIT 1") || status=""
    if [[ "$status" == "ok" ]]; then
      exec_pg "SELECT fetch_status, fetch_version, fetched_at FROM page_contents WHERE normalized_key = '${NORMALIZED_KEY}' ORDER BY fetch_version DESC LIMIT 1" >"$ARTIFACT_DIR/page_contents.txt"
      return 0
    fi
    sleep 2
  done
  die "Timed out waiting for page_contents for ${NORMALIZED_KEY}"
}

wait_for_classification() {
  log "Waiting for LLM verdict"
  for ((i = 1; i <= 60; i++)); do
    local action
    action=$(exec_pg "SELECT recommended_action FROM classifications WHERE normalized_key = '${NORMALIZED_KEY}'") || action=""
    if [[ -n "$action" ]]; then
      exec_pg "SELECT recommended_action, updated_at FROM classifications WHERE normalized_key = '${NORMALIZED_KEY}'" >"$ARTIFACT_DIR/classification.txt"
      return 0
    fi
    sleep 2
  done
  die "Timed out waiting for classification for ${NORMALIZED_KEY}"
}

check_pending_cleared() {
  local row
  row=$(exec_pg "SELECT status FROM classification_requests WHERE normalized_key = '${NORMALIZED_KEY}'") || row=""
  if [[ -n "$row" ]]; then
    die "Pending row still present after classification"
  fi
}

check_cli_pending() {
  local phase="$1"
  local expect_present="$2"
  local tries=${CLI_PENDING_TRIES:-30}
  local delay=${CLI_PENDING_DELAY:-2}
  local output
  for ((i = 1; i <= tries; i++)); do
    output=$(exec_runner "odctl classification pending --limit 100 --json")
    if [[ "$expect_present" == "yes" ]]; then
      if echo "$output" | jq -e ".[] | select(.normalized_key == \"${NORMALIZED_KEY}\")" >/dev/null; then
        printf '%s\n' "$output" >"$ARTIFACT_DIR/cli-pending-${phase}.json"
        return 0
      fi
    else
      if ! echo "$output" | jq -e ".[] | select(.normalized_key == \"${NORMALIZED_KEY}\")" >/dev/null; then
        printf '%s\n' "$output" >"$ARTIFACT_DIR/cli-pending-${phase}.json"
        return 0
      fi
    fi
    sleep "$delay"
  done
  printf '%s\n' "$output" >"$ARTIFACT_DIR/cli-pending-${phase}.json"
  if [[ "$expect_present" == "yes" ]]; then
    die "CLI pending list missing ${NORMALIZED_KEY} (${phase})"
  else
    die "CLI pending list still contains ${NORMALIZED_KEY} (${phase})"
  fi
}

fetch_admin_pending() {
  local phase="$1"
  curl -sf -H "X-Admin-Token: ${ADMIN_TOKEN}" "http://localhost:19000/api/v1/classifications/pending" | jq '.' >"$ARTIFACT_DIR/api-pending-${phase}.json"
}

send_icap_request() {
  log "Triggering ICAP request for $TARGET_URL"
  python3 - <<PY >"$ARTIFACT_DIR/icap-response.log"
import socket
payload = (
    "REQMOD icap://icap.service/req ICAP/1.0\r\n"
    "Host: icap.service\r\n"
    "X-Trace-Id: content-smoke\r\n"
    "Encapsulated: req-body=0, null-body=0\r\n"
    "\r\n"
    "GET ${TARGET_URL} HTTP/1.1\r\n"
    "Host: ${TARGET_HOST}\r\n"
    "\r\n"
)
with socket.create_connection(("localhost", 1344), timeout=30) as sock:
    sock.sendall(payload.encode("ascii"))
    sock.shutdown(socket.SHUT_WR)
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            break
        print(chunk.decode("latin-1"), end="")
PY
  if ! grep -q "Site Under Classification" "$ARTIFACT_DIR/icap-response.log"; then
    die 'ICAP response did not contain holding page markup'
  fi
}

ensure_stream_entries() {
  exec_redis XRANGE classification-jobs - + >"$ARTIFACT_DIR/classification-stream.txt"
  exec_redis XRANGE page-fetch-jobs - + >"$ARTIFACT_DIR/page-fetch-stream.txt"
  grep -q "$NORMALIZED_KEY" "$ARTIFACT_DIR/classification-stream.txt" || die 'classification job missing in stream'
  grep -q "$NORMALIZED_KEY" "$ARTIFACT_DIR/page-fetch-stream.txt" || die 'page fetch job missing in stream'
}

check_cache_entry() {
  exec_redis GET "$NORMALIZED_KEY" >"$ARTIFACT_DIR/cache-entry.txt"
  if ! grep -q '{' "$ARTIFACT_DIR/cache-entry.txt"; then
    die 'Redis cache entry missing after classification'
  fi
}

main() {
  log 'Starting docker compose stack'
  start_stack
  trap stop_stack EXIT

  wait_for_http "http://localhost:19000/health/ready" 'Admin API'
  wait_for_http "http://localhost:18080/" 'Smoke origin'
  wait_for_http "http://localhost:8085/healthz" 'Crawl4AI'

  cleanup_state
  send_icap_request
  ensure_stream_entries
  check_cli_pending before yes
  fetch_admin_pending before
  verify_pending_entry
  wait_for_page_content
  wait_for_classification
  check_pending_cleared
  check_cli_pending after no
  fetch_admin_pending after
  check_cache_entry

  log "PASS: ${NORMALIZED_KEY} classified with content-backed verdict"
}

main
