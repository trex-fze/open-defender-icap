#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMPOSE_FILE=${COMPOSE_FILE:-"$ROOT_DIR/deploy/docker/docker-compose.yml"}
COMPOSE_ENV_FILE=${COMPOSE_ENV_FILE:-"$ROOT_DIR/.env"}
OUT_DIR=${OUT_DIR:-"$ROOT_DIR/tests/artifacts/stream-consumer-audit/$(date +%Y%m%d%H%M%S)"}

mkdir -p "$OUT_DIR"

docker compose --env-file "$COMPOSE_ENV_FILE" -f "$COMPOSE_FILE" exec -T redis redis-cli XINFO GROUPS classification-jobs >"$OUT_DIR/classification-jobs-groups.txt" 2>&1 || true
docker compose --env-file "$COMPOSE_ENV_FILE" -f "$COMPOSE_FILE" exec -T redis redis-cli XINFO GROUPS page-fetch-jobs >"$OUT_DIR/page-fetch-jobs-groups.txt" 2>&1 || true

grep -R --line-number "xread_options\|XREADGROUP\|xgroup\|xack" "$ROOT_DIR/workers" --include='*.rs' >"$OUT_DIR/worker-stream-read-patterns.txt" || true

cat >"$OUT_DIR/README.txt" <<EOF
Stream consumer audit artifacts

Generated at: $(date -Iseconds)

Files:
- classification-jobs-groups.txt
- page-fetch-jobs-groups.txt
- worker-stream-read-patterns.txt
EOF

printf 'stream consumer audit artifacts: %s\n' "$OUT_DIR"
