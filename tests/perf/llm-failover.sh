#!/usr/bin/env bash
set -euo pipefail

# Simulates hybrid failover by stopping the primary (LM Studio) container, enqueueing
# a classification job, and ensuring it still completes (fallback provider takes over).

STACK_DIR=${STACK_DIR:-"$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)/deploy/docker"}
REDIS_URL=${REDIS_URL:-"redis://127.0.0.1:6379"}
DATABASE_URL=${DATABASE_URL:-"postgres://defender:defender@127.0.0.1:5432/defender_admin"}
WAIT_SECONDS=${WAIT_SECONDS:-30}

command -v redis-cli >/dev/null || { echo "redis-cli required" >&2; exit 1; }
command -v psql >/dev/null || { echo "psql required" >&2; exit 1; }
command -v docker >/dev/null || { echo "docker required" >&2; exit 1; }

compose_stop() {
  local svc="$1"
  (cd "$STACK_DIR" && docker compose stop "$svc")
}

compose_start() {
  local svc="$1"
  (cd "$STACK_DIR" && docker compose start "$svc")
}

compose_ps() {
  local svc="$1"
  (cd "$STACK_DIR" && docker compose ps --services --filter "status=running" | grep -Fx "$svc" >/dev/null)
}

PRIMARY_SERVICE=${PRIMARY_SERVICE:-""}
PRIMARY_STOPPED=0

if [[ -n "$PRIMARY_SERVICE" ]]; then
  if compose_ps "$PRIMARY_SERVICE" >/dev/null 2>&1; then
    echo "[failover] Stopping primary provider container ($PRIMARY_SERVICE)"
    compose_stop "$PRIMARY_SERVICE"
    PRIMARY_STOPPED=1
  else
    echo "[failover] Primary provider container $PRIMARY_SERVICE not found in compose stack"
  fi
else
  echo "[failover] PRIMARY_SERVICE not set; assuming remote/offline provider. Ensure you stop it manually if needed."
fi

JOB_KEY="domain:failover.$(date +%s)"
TRACE_ID="failover-test-$(date +%s)"
PAYLOAD=$(cat <<JSON
{
  "normalized_key": "$JOB_KEY",
  "entity_level": "domain",
  "hostname": "failover.test",
  "full_url": "https://failover.test/",
  "trace_id": "$TRACE_ID"
}
JSON
)

echo "[failover] Enqueueing job $JOB_KEY"
redis-cli -u "$REDIS_URL" XADD classification-jobs '*' payload "$PAYLOAD" >/dev/null

echo "[failover] Waiting up to ${WAIT_SECONDS}s for classification"
end=$((SECONDS + WAIT_SECONDS))
while (( SECONDS < end )); do
  EXISTS=$(psql "$DATABASE_URL" -Atc "SELECT COUNT(*) FROM classifications WHERE normalized_key = '$JOB_KEY'" || echo 0)
  if [[ "$EXISTS" -gt 0 ]]; then
    echo "[failover] Classification stored successfully (fallback provider active)"
    break
  fi
  sleep 2
done

if [[ "$EXISTS" -eq 0 ]]; then
  echo "[failover] ERROR: classification not persisted within timeout" >&2
  [[ "$PRIMARY_STOPPED" -eq 1 ]] && compose_start "$PRIMARY_SERVICE"
  exit 1
fi

if [[ "$PRIMARY_STOPPED" -eq 1 ]]; then
  echo "[failover] Restarting primary provider container ($PRIMARY_SERVICE)"
  compose_start "$PRIMARY_SERVICE"
fi

echo "[failover] Hybrid LLM failover smoke completed"
