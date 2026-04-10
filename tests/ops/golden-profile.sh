#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
COMPOSE_BASE=${COMPOSE_BASE:-"$ROOT_DIR/deploy/docker/docker-compose.yml"}
COMPOSE_PROFILES=${COMPOSE_PROFILES:-"$ROOT_DIR/deploy/docker/docker-compose.golden-profiles.yml"}
COMPOSE_ENV_FILE=${COMPOSE_ENV_FILE:-"$ROOT_DIR/.env"}
PROFILE=${PROFILE:-golden-local}
COMMAND=${1:-verify}

if [[ "$PROFILE" != "golden-local" && "$PROFILE" != "golden-prodlike" ]]; then
  echo "PROFILE must be golden-local or golden-prodlike" >&2
  exit 1
fi

compose() {
  docker compose \
    --env-file "$COMPOSE_ENV_FILE" \
    -f "$COMPOSE_BASE" \
    -f "$COMPOSE_PROFILES" \
    --profile "$PROFILE" \
    "$@"
}

verify_health() {
  curl -fsS "http://localhost:19000/health/ready" >/dev/null
  curl -fsS "http://localhost:19010/health/ready" >/dev/null
  if [[ "$PROFILE" == "golden-prodlike" ]]; then
    curl -fsS "http://localhost:19100/health/ready" >/dev/null
    curl -fsS "http://localhost:9090/-/ready" >/dev/null
  fi
}

run_smoke() {
  compose run --rm odctl-runner odctl smoke --profile compose
}

case "$COMMAND" in
  up)
    compose up -d --build
    ;;
  down)
    compose down
    ;;
  verify)
    if [[ "${DRY_RUN:-0}" == "1" ]]; then
      compose config >/dev/null
      echo "golden profile config verified: $PROFILE"
      exit 0
    fi
    compose up -d --build
    verify_health
    run_smoke
    echo "golden profile verified: $PROFILE"
    ;;
  *)
    echo "usage: PROFILE=golden-local|golden-prodlike $0 [up|verify|down]" >&2
    exit 1
    ;;
esac
