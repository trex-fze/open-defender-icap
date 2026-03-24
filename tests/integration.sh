#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
STACK_DIR="$ROOT/deploy/docker"

echo "[integration] Building and starting docker-compose stack"
pushd "$STACK_DIR" >/dev/null

docker compose up -d --build
echo "[integration] Waiting for services to become healthy"
docker compose run --rm odctl-runner bash -lc "sleep 5"

echo "[integration] Running odctl smoke tests"
docker compose run --rm odctl-runner odctl smoke --profile compose || {
  echo "odctl smoke failed" >&2
  docker compose logs --tail=200
  exit 1
}

echo "[integration] Executing Stage 6 ingest smoke test"
docker compose run --rm odctl-runner bash -lc "INGEST_URL=http://event-ingester:19100 ELASTIC_URL=http://elasticsearch:9200 ADMIN_URL=http://admin-api:19000 tests/stage06_ingest.sh"

echo "[integration] Verifying page fetch flow"
docker compose run --rm odctl-runner bash -lc "INGEST_URL=http://event-ingester:19100 ADMIN_URL=http://admin-api:19000 PAGE_FETCH_TARGET=http://admin-api:19000/health/ready PAGE_FETCH_NORMALIZED_KEY=domain:admin-api tests/page-fetch-flow.sh"

echo "[integration] Collecting health endpoints"
curl -sf http://localhost:19000/health/ready >/dev/null
curl -sf http://localhost:19010/health/ready >/dev/null
curl -sf http://localhost:19100/health/ready >/dev/null

echo "[integration] docker-compose integration tests completed"

docker compose down
popd >/dev/null
