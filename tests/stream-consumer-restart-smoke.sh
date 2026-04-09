#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMPOSE_FILE=${COMPOSE_FILE:-"$ROOT_DIR/deploy/docker/docker-compose.yml"}
COMPOSE_ENV_FILE=${COMPOSE_ENV_FILE:-"$ROOT_DIR/.env"}
ARTIFACT_DIR=${ARTIFACT_DIR:-"$ROOT_DIR/tests/artifacts/stream-restart-smoke"}
RUN_ID=${RUN_ID:-"stream-restart-$(date +%Y%m%d%H%M%S)"}
OUT_DIR="$ARTIFACT_DIR/$RUN_ID"
BUILD_IMAGES=${BUILD_IMAGES:-1}
mkdir -p "$OUT_DIR"

compose() {
  docker compose --env-file "$COMPOSE_ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

log() {
  printf '[%s] %s\n' "$(date '+%Y-%m-%dT%H:%M:%S%z')" "$*"
}

log "Starting required services"
if [[ "$BUILD_IMAGES" == "1" ]]; then
  compose up -d --build redis llm-worker
else
  compose up -d redis llm-worker
fi

START_MS=$(python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
)

POISON_PAYLOAD='{"normalized_key":"domain:restart-smoke.invalid","entity_level":"domain"'

log "Publishing poison message to classification-jobs"
compose exec -T redis redis-cli XADD classification-jobs '*' payload "$POISON_PAYLOAD" >"$OUT_DIR/xadd.txt"

log "Restarting llm-worker to exercise consumer recovery"
compose restart llm-worker >"$OUT_DIR/restart.txt"

sleep 6

log "Reading DLQ entries"
for _ in {1..20}; do
  compose exec -T redis redis-cli XRANGE classification-jobs-dlq - + COUNT 100 >"$OUT_DIR/dlq.txt"
  if grep -q "invalid_payload" "$OUT_DIR/dlq.txt"; then
    break
  fi
  sleep 2
done

compose logs --no-color --tail=200 llm-worker >"$OUT_DIR/llm-worker.log" || true

if ! grep -q "classification-jobs" "$OUT_DIR/dlq.txt"; then
  log "FAIL: source stream not found in DLQ"
  exit 1
fi

if ! grep -q "invalid_payload" "$OUT_DIR/dlq.txt"; then
  log "FAIL: invalid_payload reason not found in DLQ"
  exit 1
fi

COUNT=$(grep -c "invalid_payload" "$OUT_DIR/dlq.txt" || true)
if [[ "$COUNT" -gt 2 ]]; then
  log "FAIL: duplicate DLQ entries detected ($COUNT)"
  exit 1
fi

log "PASS: restart smoke validated DLQ handling and bounded duplicates"
log "Artifacts: $OUT_DIR"
