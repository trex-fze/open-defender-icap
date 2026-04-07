#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMPOSE_FILE=${COMPOSE_FILE:-"$ROOT_DIR/deploy/docker/docker-compose.yml"}
ADMIN_API_URL=${ADMIN_API_URL:-"http://localhost:19000"}
ADMIN_TOKEN=${ADMIN_TOKEN:-}
ADMIN_BEARER=${ADMIN_BEARER:-}
LOCAL_USERNAME=${LOCAL_USERNAME:-"admin"}
LOCAL_PASSWORD=${LOCAL_PASSWORD:-}
ARTIFACT_DIR=${ARTIFACT_DIR:-"$ROOT_DIR/tests/artifacts/policy-cursor-smoke"}
RUN_ID=${RUN_ID:-"policy-cursor-$(date +%Y%m%d%H%M%S)"}
OUT_DIR="$ARTIFACT_DIR/$RUN_ID"
mkdir -p "$OUT_DIR"

auth_header() {
  if [[ -n "$ADMIN_BEARER" ]]; then
    printf 'Authorization: Bearer %s' "$ADMIN_BEARER"
  else
    printf 'X-Admin-Token: %s' "$ADMIN_TOKEN"
  fi
}

api_get() {
  local path="$1"
  curl -fsS -H "$(auth_header)" "${ADMIN_API_URL}${path}"
}

api_post() {
  local path="$1"
  local body="$2"
  curl -fsS -H "$(auth_header)" -H 'Content-Type: application/json' -X POST --data "$body" "${ADMIN_API_URL}${path}"
}

if [[ -z "$ADMIN_TOKEN" ]]; then
  ADMIN_TOKEN=$(docker compose -f "$COMPOSE_FILE" exec -T admin-api printenv OD_ADMIN_TOKEN 2>/dev/null || true)
fi

if [[ -z "$ADMIN_TOKEN" && -z "$LOCAL_PASSWORD" && -f "$ROOT_DIR/deploy/docker/.env" ]]; then
  LOCAL_PASSWORD=$(grep '^OD_DEFAULT_ADMIN_PASSWORD=' "$ROOT_DIR/deploy/docker/.env" | cut -d'=' -f2- || true)
fi

if [[ -z "$ADMIN_TOKEN" ]]; then
  ADMIN_BEARER=$(curl -fsS -H 'Content-Type: application/json' -X POST \
    --data "$(jq -n --arg username "$LOCAL_USERNAME" --arg password "$LOCAL_PASSWORD" '{username:$username,password:$password}')" \
    "${ADMIN_API_URL}/api/v1/auth/login" | jq -r '.access_token // empty')
  if [[ -z "$ADMIN_BEARER" ]]; then
    echo "failed to obtain local auth token" >&2
    exit 1
  fi
fi

ts=$(date +%s)
for i in 1 2; do
  api_post "/api/v1/policies" "$(jq -n --arg n "cursor-smoke-${ts}-${i}" '{name:$n, version:"draft-smoke", notes:"cursor smoke", rules:[{id:("r-"+$n), priority:10, action:"Allow", conditions:{}}]}')" >"$OUT_DIR/create-${i}.json"
done

api_get "/api/v1/policies?include_drafts=true&limit=1" >"$OUT_DIR/page-1.json"
cursor=$(jq -r '.meta.next_cursor // empty' "$OUT_DIR/page-1.json")
first_id=$(jq -r '.data[0].id // empty' "$OUT_DIR/page-1.json")

if [[ -z "$cursor" ]]; then
  echo "missing next_cursor in first page" >&2
  exit 1
fi

api_get "/api/v1/policies?include_drafts=true&limit=1&cursor=${cursor}" >"$OUT_DIR/page-2.json"
second_id=$(jq -r '.data[0].id // empty' "$OUT_DIR/page-2.json")

if [[ -z "$second_id" || "$second_id" == "$first_id" ]]; then
  echo "cursor chain did not advance to a new record" >&2
  exit 1
fi

docker compose -f "$COMPOSE_FILE" exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"EXPLAIN SELECT id, created_at FROM policies ORDER BY created_at DESC, id DESC LIMIT 1\"" >"$OUT_DIR/explain.txt"
docker compose -f "$COMPOSE_FILE" exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"SELECT indexname FROM pg_indexes WHERE schemaname='public' AND tablename='policies' AND indexname='policies_created_id_idx'\"" >"$OUT_DIR/index-check.txt"

if ! grep -q "policies_created_id_idx" "$OUT_DIR/index-check.txt"; then
  echo "expected policies cursor index not present" >&2
  exit 1
fi

echo "policy cursor smoke pass: $OUT_DIR"
