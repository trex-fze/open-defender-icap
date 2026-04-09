#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
STACK_DIR="$ROOT/deploy/docker"
COMPOSE_ENV_FILE=${COMPOSE_ENV_FILE:-"$ROOT/.env"}
INTEGRATION_BUILD=${INTEGRATION_BUILD:-1}
INTEGRATION_BUILD_RETRIES=${INTEGRATION_BUILD_RETRIES:-3}
INTEGRATION_RETRY_DELAY_SECONDS=${INTEGRATION_RETRY_DELAY_SECONDS:-5}
INTEGRATION_PRUNE_ON_RETRY=${INTEGRATION_PRUNE_ON_RETRY:-1}

build_stack() {
  local attempt=1
  local max_attempts=$INTEGRATION_BUILD_RETRIES

  while (( attempt <= max_attempts )); do
    echo "[integration] Build attempt ${attempt}/${max_attempts}"
    if docker compose --env-file "$COMPOSE_ENV_FILE" build; then
      docker compose --env-file "$COMPOSE_ENV_FILE" up -d
      return 0
    fi

    if (( attempt == max_attempts )); then
      echo "[integration] Build failed after ${max_attempts} attempts" >&2
      return 1
    fi

    if [[ "$INTEGRATION_PRUNE_ON_RETRY" == "1" ]]; then
      echo "[integration] Build failed; pruning builder cache before retry"
      docker builder prune -f >/dev/null || true
    fi

    echo "[integration] Retrying build in ${INTEGRATION_RETRY_DELAY_SECONDS}s"
    sleep "$INTEGRATION_RETRY_DELAY_SECONDS"
    attempt=$((attempt + 1))
  done
}

echo "[integration] Building and starting docker-compose stack"
pushd "$STACK_DIR" >/dev/null

if [[ "$INTEGRATION_BUILD" == "1" ]]; then
  build_stack
else
  docker compose --env-file "$COMPOSE_ENV_FILE" up -d
fi
echo "[integration] Waiting for services to become healthy"
docker compose --env-file "$COMPOSE_ENV_FILE" run --rm odctl-runner bash -lc "sleep 5"

echo "[integration] Running odctl smoke tests"
docker compose --env-file "$COMPOSE_ENV_FILE" run --rm odctl-runner odctl smoke --profile compose || {
  echo "odctl smoke failed" >&2
  docker compose --env-file "$COMPOSE_ENV_FILE" logs --tail=200
  exit 1
}

echo "[integration] Running LLM provider smoke test"
COMPOSE_FILE=./docker-compose.yml \
  LLM_METRICS_URL=${LLM_METRICS_URL:-http://localhost:19015/metrics} \
  LLM_PROVIDERS_URL=${LLM_PROVIDERS_URL:-http://localhost:19015/providers} \
  "$ROOT/tests/security/llm-prompt-smoke.sh"

echo "[integration] Executing Stage 6 ingest smoke test"
docker compose --env-file "$COMPOSE_ENV_FILE" run --rm odctl-runner bash -lc "INGEST_URL=http://event-ingester:19100 ELASTIC_URL=http://elasticsearch:9200 ADMIN_URL=http://admin-api:19000 tests/stage06_ingest.sh"

echo "[integration] Verifying page fetch flow"
docker compose --env-file "$COMPOSE_ENV_FILE" run --rm odctl-runner bash -lc "INGEST_URL=http://event-ingester:19100 ADMIN_URL=http://admin-api:19000 PAGE_FETCH_TARGET=http://admin-api:19000/health/ready PAGE_FETCH_NORMALIZED_KEY=domain:admin-api tests/page-fetch-flow.sh"

echo "[integration] Running content-first blocking smoke test"
KEEP_STACK=1 "$ROOT/tests/content-pending-smoke.sh"

echo "[integration] Collecting health endpoints"
curl -sf http://localhost:19000/health/ready >/dev/null
curl -sf http://localhost:19010/health/ready >/dev/null
curl -sf http://localhost:19100/health/ready >/dev/null

echo "[integration] docker-compose integration tests completed"

docker compose --env-file "$COMPOSE_ENV_FILE" down
popd >/dev/null
