#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMPOSE_FILE=${COMPOSE_FILE:-"$ROOT_DIR/deploy/docker/docker-compose.yml"}
ARTIFACT_DIR=${ARTIFACT_DIR:-"$ROOT_DIR/tests/artifacts/policy-runtime"}
ADMIN_TOKEN=${OD_ADMIN_TOKEN:-changeme-admin}
KEEP_STACK=${KEEP_STACK:-0}
BUILD_IMAGES=${BUILD_IMAGES:-0}
WAIT_HTTP_TRIES=${WAIT_HTTP_TRIES:-120}

mkdir -p "$ARTIFACT_DIR"

log() {
  printf '[policy-runtime-smoke] %s\n' "$*"
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

wait_for_http() {
  local url="$1"
  local name="$2"
  local tries=$WAIT_HTTP_TRIES
  for ((i = 1; i <= tries; i++)); do
    if curl -sf "$url" >/dev/null; then
      log "$name ready ($url)"
      return 0
    fi
    sleep 2
  done
  die "Timed out waiting for $name at $url"
}

admin_post() {
  local path="$1"
  local body="$2"
  curl -sf -X POST \
    -H "Content-Type: application/json" \
    -H "X-Admin-Token: ${ADMIN_TOKEN}" \
    "http://localhost:19000${path}" \
    -d "$body"
}

admin_get() {
  local path="$1"
  curl -sf -H "X-Admin-Token: ${ADMIN_TOKEN}" "http://localhost:19000${path}"
}

policy_get() {
  local path="$1"
  curl -sf -H "X-Admin-Token: ${ADMIN_TOKEN}" "http://localhost:19010${path}"
}

main() {
  local ts version draft_name policy_id
  ts=$(date +%Y%m%d%H%M%S)
  version="release-smoke-${ts}"
  draft_name="Policy Runtime Smoke ${ts}"

  log 'Starting docker compose stack'
  start_stack
  trap stop_stack EXIT

  wait_for_http "http://localhost:19000/health/ready" 'Admin API'
  wait_for_http "http://localhost:19010/health/ready" 'Policy Engine'

  log 'Creating policy draft via Admin API'
  admin_post "/api/v1/policies" "$(jq -n --arg name "$draft_name" --arg version "draft-${ts}" '{name:$name, version:$version, notes:"smoke test policy", rules:[{id:"smoke-block-user", priority:1, action:"Block", description:"Block smoke user", conditions:{user_ids:["policy-smoke-user"]}}]}')" >"$ARTIFACT_DIR/create-policy.json"
  policy_id=$(jq -r '.id' "$ARTIFACT_DIR/create-policy.json")
  [[ -n "$policy_id" && "$policy_id" != "null" ]] || die 'failed to parse policy id from create response'

  log "Publishing policy ${policy_id} as ${version}"
  admin_post "/api/v1/policies/${policy_id}/publish" "$(jq -n --arg version "$version" --arg notes "smoke publish" '{version:$version, notes:$notes}')" >"$ARTIFACT_DIR/publish-policy.json"

  log 'Checking Admin API runtime sync endpoint'
  admin_get "/api/v1/policies/runtime-sync" | tee "$ARTIFACT_DIR/runtime-sync.json" >/dev/null
  jq -e '.in_sync == true' "$ARTIFACT_DIR/runtime-sync.json" >/dev/null || die 'runtime-sync reported drift'
  jq -e --arg version "$version" '.runtime.version == $version and .control_plane.version == $version' "$ARTIFACT_DIR/runtime-sync.json" >/dev/null || die 'runtime-sync version mismatch'

  log 'Checking policy-engine runtime version'
  policy_get "/api/v1/policies" | tee "$ARTIFACT_DIR/policy-engine-runtime.json" >/dev/null
  jq -e --arg version "$version" '.version == $version' "$ARTIFACT_DIR/policy-engine-runtime.json" >/dev/null || die 'policy-engine version mismatch'

  log 'Checking decision behavior for smoke user'
  curl -sf -X POST "http://localhost:19010/api/v1/decision" \
    -H "Content-Type: application/json" \
    -d '{"normalized_key":"domain:example.com","entity_level":"domain","source_ip":"192.0.2.10","user_id":"policy-smoke-user"}' \
    | tee "$ARTIFACT_DIR/decision.json" >/dev/null
  jq -e '.action == "Block"' "$ARTIFACT_DIR/decision.json" >/dev/null || die 'decision did not reflect published smoke policy'

  log "PASS: runtime sync and decision propagation verified for ${policy_id}"
}

main "$@"
