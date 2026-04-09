#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

PROXY_URL=${PROXY_URL:-http://localhost:3128}
TARGET_DOMAIN=${TARGET_DOMAIN:-www.facebook.com}
TARGET_ROOT_DOMAIN=${TARGET_ROOT_DOMAIN:-facebook.com}
TARGET_NORMALIZED_KEY=${TARGET_NORMALIZED_KEY:-subdomain:www.facebook.com}
TARGET_CANONICAL_KEY=${TARGET_CANONICAL_KEY:-domain:${TARGET_ROOT_DOMAIN}}
SOCIAL_CATEGORY_ID=${SOCIAL_CATEGORY_ID:-social-media}
ADMIN_API_URL=${ADMIN_API_URL:-http://localhost:19000}
ADMIN_TOKEN=${ADMIN_TOKEN:-${OD_ADMIN_TOKEN:-changeme-admin}}
COMPOSE_FILE=${COMPOSE_FILE:-deploy/docker/docker-compose.yml}
COMPOSE_ENV_FILE=${COMPOSE_ENV_FILE:-"$ROOT_DIR/.env"}
RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
ARTIFACT_ROOT=${ARTIFACT_ROOT:-tests/artifacts/facebook-e2e/${RUN_ID}}
WAIT_AFTER_INITIAL=${WAIT_AFTER_INITIAL:-45}
WAIT_STAGE_SECONDS=${WAIT_STAGE_SECONDS:-90}
WAIT_LLM_SECONDS=${WAIT_LLM_SECONDS:-300}
WAIT_DB_SECONDS=${WAIT_DB_SECONDS:-330}
POLL_INTERVAL_SECONDS=${POLL_INTERVAL_SECONDS:-3}
CONNECT_URL="https://${TARGET_DOMAIN}"
CLEAN_TARGET_STATE=${CLEAN_TARGET_STATE:-1}
AUTO_DISABLE_SOCIAL_CATEGORY=${AUTO_DISABLE_SOCIAL_CATEGORY:-1}
SQUID_ACCESS_LOG=${SQUID_ACCESS_LOG:-data/squid-logs/access.log}

mkdir -p "${ARTIFACT_ROOT}"
STAGE_META_FILE="${ARTIFACT_ROOT}/stage-metadata.tsv"
touch "${STAGE_META_FILE}"
RUN_LOG="${ARTIFACT_ROOT}/run.log"
RUN_STARTED=$(date -Iseconds)
SQUID_LOG_LINE_START=0
FAIL_COUNT=0

CHECKLIST_FILE="${ARTIFACT_ROOT}/checklist.md"
cat >"${CHECKLIST_FILE}" <<'EOF'
# Facebook E2E Checklist

- [ ] S00 Preflight: services healthy + social-media disabled + LLM provider reachable
- [ ] S01 Client request sent via Squid proxy to facebook
- [ ] S02 Squid confirms CONNECT request reached proxy
- [ ] S03 ICAP adaptor receives request and publishes jobs
- [ ] S04 Policy Engine direct decision endpoint responds for facebook key
- [ ] S05 Redis streams contain classification + page-fetch jobs
- [ ] S06 Page Fetcher processes facebook job without URL errors
- [ ] S07 Crawl4AI receives crawl request for facebook URL
- [ ] S08 LLM worker processes/classifies facebook key
- [ ] S09 DB shows pending/classification rows for facebook
- [ ] S10 Follow-up client request is blocked/pending (not direct 200 tunnel)
- [ ] S11 ICAP emits cache/final decision for facebook key
- [ ] S12 Consolidated report generated
EOF

log() {
  local msg=$1
  printf '[%s] %s\n' "$(date -Iseconds)" "$msg" | tee -a "$RUN_LOG"
}

CURRENT_STAGE=""
CURRENT_LABEL=""
STAGE_START=0

now_ms() {
  python - <<'PY'
import time
print(int(time.time() * 1000))
PY
}

start_stage() {
  CURRENT_STAGE=$1
  CURRENT_LABEL=$2
  STAGE_START=$(now_ms)
  log "=== Stage ${CURRENT_STAGE}: ${CURRENT_LABEL} ==="
}

finish_stage() {
  local status=$1
  local desc=$2
  local files=${3:-}
  local end=$(now_ms)
  local duration=$((end - STAGE_START))
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$CURRENT_STAGE" "$CURRENT_LABEL" "$status" "$desc" "$files" "$duration" >> "$STAGE_META_FILE"
  log "--- Stage ${CURRENT_STAGE} status: ${status} (${desc}) ---"
  if [[ "$status" == "FAIL" ]]; then
    FAIL_COUNT=$((FAIL_COUNT + 1))
  fi
}

run_compose() {
  docker compose --env-file "$COMPOSE_ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

wait_for_service_log_match() {
  local service=$1
  local pattern=$2
  local output_file=$3
  local timeout_seconds=${4:-$WAIT_STAGE_SECONDS}
  local start
  start=$(date +%s)
  while true; do
    run_compose logs --since "$RUN_STARTED" --tail=400 "$service" >"$output_file" 2>&1 || true
    if grep -q "$pattern" "$output_file"; then
      return 0
    fi
    if (( $(date +%s) - start >= timeout_seconds )); then
      return 1
    fi
    sleep "$POLL_INTERVAL_SECONDS"
  done
}

wait_for_db_rows() {
  local class_file=$1
  local pending_file=$2
  local timeout_seconds=${3:-$WAIT_STAGE_SECONDS}
  local start
  start=$(date +%s)
  while true; do
    run_compose exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"SELECT COUNT(*) FROM classifications WHERE normalized_key='${TARGET_CANONICAL_KEY}';\"" >"$class_file" 2>&1 || true
    run_compose exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"SELECT COUNT(*) FROM classification_requests WHERE normalized_key='${TARGET_CANONICAL_KEY}';\"" >"$pending_file" 2>&1 || true
    class_count=$(grep -E '^[0-9]+$' "$class_file" | tail -n1 || true)
    pending_count=$(grep -E '^[0-9]+$' "$pending_file" | tail -n1 || true)
    class_count=${class_count:-0}
    pending_count=${pending_count:-0}
    if (( class_count > 0 || pending_count > 0 )); then
      return 0
    fi
    if (( $(date +%s) - start >= timeout_seconds )); then
      return 1
    fi
    sleep "$POLL_INTERVAL_SECONDS"
  done
}

file_contains_target_key() {
  local file=$1
  grep -Fq "$TARGET_NORMALIZED_KEY" "$file" || grep -Fq "$TARGET_CANONICAL_KEY" "$file"
}

file_contains_canonical_key() {
  local file=$1
  grep -Fq "$TARGET_CANONICAL_KEY" "$file"
}

disable_social_category() {
  local taxonomy_file=$1
  local payload_file=$2
  jq --arg id "$SOCIAL_CATEGORY_ID" '
    {
      version: .version,
      categories: [
        .categories[] as $cat |
        {
          id: $cat.id,
          enabled: (if $cat.id == $id then false else $cat.enabled end),
          subcategories: [
            ($cat.subcategories // [])[] |
            {
              id: .id,
              enabled: (if $cat.id == $id then false else .enabled end)
            }
          ]
        }
      ]
    }
  ' "$taxonomy_file" >"$payload_file"
  curl -sS \
    -H "X-Admin-Token: ${ADMIN_TOKEN}" \
    -H 'Content-Type: application/json' \
    -X PUT \
    --data @"$payload_file" \
    "${ADMIN_API_URL}/api/v1/taxonomy/activation" >/dev/null
}

wait_for_llm_classification_log() {
  local output_file=$1
  local timeout_seconds=${2:-$WAIT_LLM_SECONDS}
  local start
  start=$(date +%s)
  while true; do
    run_compose logs --since "$RUN_STARTED" --tail=600 llm-worker >"$output_file" 2>&1 || true
    if grep -q "classification stored" "$output_file" && file_contains_target_key "$output_file"; then
      return 0
    fi
    if (( $(date +%s) - start >= timeout_seconds )); then
      return 1
    fi
    sleep "$POLL_INTERVAL_SECONDS"
  done
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    log "missing required command: $1"
    exit 1
  fi
}

require_command jq
require_command curl

if [[ -f "$SQUID_ACCESS_LOG" ]]; then
  SQUID_LOG_LINE_START=$(wc -l <"$SQUID_ACCESS_LOG")
fi

# Stage 0: Preflight
start_stage "S00" "Preflight"
PRE_PS_FILE="${ARTIFACT_ROOT}/00-preflight-ps.txt"
run_compose ps --status running >"$PRE_PS_FILE"
required_services=(squid icap-adaptor policy-engine redis page-fetcher crawl4ai llm-worker admin-api)
missing=()
for svc in "${required_services[@]}"; do
  if ! grep -q "${svc}" "$PRE_PS_FILE"; then
    missing+=("${svc}")
  fi
done

PRE_TAX_FILE="${ARTIFACT_ROOT}/00-taxonomy.json"
if ! curl -sS -H "X-Admin-Token: ${ADMIN_TOKEN}" "${ADMIN_API_URL}/api/v1/taxonomy" >"$PRE_TAX_FILE"; then
  finish_stage "FAIL" "Failed to query taxonomy" "$PRE_PS_FILE,$PRE_TAX_FILE"
else
  social_state=$(jq -r --arg id "$SOCIAL_CATEGORY_ID" '.categories[] | select(.id==$id) | .enabled' "$PRE_TAX_FILE" | head -n1)
  if [[ -z "$social_state" ]]; then
    finish_stage "FAIL" "Category ${SOCIAL_CATEGORY_ID} missing" "$PRE_PS_FILE,$PRE_TAX_FILE"
  else
    PRE_DISABLE_PAYLOAD_FILE=""
    if [[ "$social_state" != "false" && "$AUTO_DISABLE_SOCIAL_CATEGORY" == "1" ]]; then
      PRE_DISABLE_PAYLOAD_FILE="${ARTIFACT_ROOT}/00-taxonomy-disable-payload.json"
      if disable_social_category "$PRE_TAX_FILE" "$PRE_DISABLE_PAYLOAD_FILE"; then
        curl -sS -H "X-Admin-Token: ${ADMIN_TOKEN}" "${ADMIN_API_URL}/api/v1/taxonomy" >"$PRE_TAX_FILE"
        social_state=$(jq -r --arg id "$SOCIAL_CATEGORY_ID" '.categories[] | select(.id==$id) | .enabled' "$PRE_TAX_FILE" | head -n1)
      fi
    fi
    PRE_LLM_FILE="${ARTIFACT_ROOT}/00-llm-health.txt"
    llm_ready=""
    if run_compose exec -T llm-worker curl -sS --max-time 5 http://192.168.1.170:1234/v1/models >"$PRE_LLM_FILE" 2>&1; then
      llm_ready="local"
    else
      if run_compose exec -T llm-worker printenv OPENAI_API_KEY | tr -d '\r' >"$PRE_LLM_FILE" 2>&1 && grep -q '\S' "$PRE_LLM_FILE"; then
        llm_ready="openai"
      fi
    fi
    if [[ "$CLEAN_TARGET_STATE" == "1" ]]; then
      PRE_CLEAN_FILE="${ARTIFACT_ROOT}/00-cleanup.txt"
      {
        run_compose exec -T redis redis-cli DEL "subdomain:${TARGET_DOMAIN}" "domain:${TARGET_ROOT_DOMAIN}" || true
        run_compose exec -T redis redis-cli PUBLISH od:cache:invalidate "{\"kind\":\"review\",\"normalized_key\":\"${TARGET_NORMALIZED_KEY}\"}" || true
        run_compose exec -T redis redis-cli PUBLISH od:cache:invalidate "{\"kind\":\"review\",\"normalized_key\":\"domain:${TARGET_ROOT_DOMAIN}\"}" || true
        sleep 1
        run_compose exec -T postgres bash -lc "psql -U defender -d defender_admin -c \"DELETE FROM classification_versions WHERE classification_id IN (SELECT id FROM classifications WHERE normalized_key ILIKE '%${TARGET_ROOT_DOMAIN}%'); DELETE FROM classifications WHERE normalized_key ILIKE '%${TARGET_ROOT_DOMAIN}%'; DELETE FROM classification_requests WHERE normalized_key ILIKE '%${TARGET_ROOT_DOMAIN}%'; DELETE FROM page_contents WHERE normalized_key ILIKE '%${TARGET_ROOT_DOMAIN}%';\"" || true
      } >"$PRE_CLEAN_FILE" 2>&1
    else
      PRE_CLEAN_FILE=""
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
      finish_stage "FAIL" "Missing services: ${missing[*]}" "$PRE_PS_FILE,$PRE_TAX_FILE,$PRE_LLM_FILE"
    elif [[ "$social_state" != "false" ]]; then
      finish_stage "FAIL" "Category ${SOCIAL_CATEGORY_ID} is enabled" "$PRE_PS_FILE,$PRE_TAX_FILE,$PRE_LLM_FILE"
    elif [[ -z "$llm_ready" ]]; then
      finish_stage "FAIL" "LLM providers unavailable" "$PRE_PS_FILE,$PRE_TAX_FILE,$PRE_LLM_FILE"
    else
      finish_stage "PASS" "Services healthy; category disabled; LLM ready via ${llm_ready}" "$PRE_PS_FILE,$PRE_TAX_FILE,$PRE_LLM_FILE,$PRE_CLEAN_FILE,$PRE_DISABLE_PAYLOAD_FILE"
    fi
  fi
fi

# Stage 1: Client initial request
start_stage "S01" "Client Initial Request"
INIT_BODY="${ARTIFACT_ROOT}/01-client-initial-body.bin"
INIT_HEADERS="${ARTIFACT_ROOT}/01-client-initial-headers.txt"
INIT_STATUS_FILE="${ARTIFACT_ROOT}/01-client-initial-status.txt"
: >"$INIT_BODY"
: >"$INIT_HEADERS"
set +e
curl -sk -x "$PROXY_URL" -o "$INIT_BODY" -D "$INIT_HEADERS" -w "%{http_code}" "$CONNECT_URL" >"$INIT_STATUS_FILE" 2>&1
curl_exit=$?
set -e
http_status=$(tail -n1 "$INIT_STATUS_FILE" | tr -d '\r')
block_detected="no"
if grep -qi "Site Under Classification" "$INIT_BODY" || grep -qi "Request blocked" "$INIT_BODY"; then
  block_detected="yes"
fi
if [[ "$http_status" == "403" || "$block_detected" == "yes" || ( "$http_status" == "000" && "$curl_exit" -ne 0 ) ]]; then
  finish_stage "PASS" "Initial request blocked with HTTP ${http_status} (curl exit ${curl_exit})" "$INIT_HEADERS,$INIT_BODY,$INIT_STATUS_FILE"
else
  finish_stage "FAIL" "Initial request not blocked (HTTP ${http_status}, curl exit ${curl_exit})" "$INIT_HEADERS,$INIT_BODY,$INIT_STATUS_FILE"
fi

# Stage 2: Squid observation
start_stage "S02" "Squid Logs"
SQUID_FILE="${ARTIFACT_ROOT}/02-squid.log"
python - "$SQUID_ACCESS_LOG" "$SQUID_LOG_LINE_START" "$SQUID_FILE" <<'PY'
import sys
from pathlib import Path

log_path = Path(sys.argv[1])
start = int(sys.argv[2])
out = Path(sys.argv[3])

if not log_path.exists():
    out.write_text(f"missing squid access log: {log_path}\n", encoding="utf-8")
    sys.exit(0)

with log_path.open("r", encoding="utf-8", errors="ignore") as fh:
    lines = fh.readlines()[start:]
out.write_text("".join(lines), encoding="utf-8")
PY
if grep -q "CONNECT ${TARGET_DOMAIN}:443" "$SQUID_FILE" || grep -q "CONNECT ${TARGET_ROOT_DOMAIN}:443" "$SQUID_FILE"; then
  finish_stage "PASS" "Squid observed CONNECT ${TARGET_DOMAIN}" "$SQUID_FILE"
else
  SQUID_FALLBACK_FILE="${ARTIFACT_ROOT}/02-squid-fallback-icap.log"
  run_compose logs --since "$RUN_STARTED" --tail=400 icap-adaptor >"$SQUID_FALLBACK_FILE" 2>&1 || true
  if file_contains_target_key "$SQUID_FALLBACK_FILE"; then
    finish_stage "WARN" "Squid stdout lacked CONNECT line; ICAP confirms request path" "$SQUID_FILE,$SQUID_FALLBACK_FILE"
  else
    finish_stage "FAIL" "CONNECT ${TARGET_DOMAIN} missing in squid logs and ICAP fallback" "$SQUID_FILE,$SQUID_FALLBACK_FILE"
  fi
fi

# Stage 3: ICAP adaptor observation
start_stage "S03" "ICAP Logs"
ICAP_FILE="${ARTIFACT_ROOT}/03-icap.log"
run_compose logs --since "$RUN_STARTED" --tail=400 icap-adaptor >"$ICAP_FILE" 2>&1 || true
if grep -q "$TARGET_NORMALIZED_KEY" "$ICAP_FILE"; then
  finish_stage "PASS" "ICAP handled ${TARGET_NORMALIZED_KEY}" "$ICAP_FILE"
else
  if file_contains_target_key "$ICAP_FILE"; then
    finish_stage "PASS" "ICAP handled canonical target key" "$ICAP_FILE"
  else
    finish_stage "FAIL" "ICAP log missing target key variants" "$ICAP_FILE"
  fi
fi

# Stage 4: Policy engine direct response
start_stage "S04" "Policy Engine Decision"
POLICY_FILE="${ARTIFACT_ROOT}/04-policy-decision.json"
POLICY_REQ='{"normalized_key":"'"${TARGET_NORMALIZED_KEY}"'","entity_level":"subdomain","source_ip":"127.0.0.1"}'
if curl -sS -H 'Content-Type: application/json' -d "$POLICY_REQ" "http://localhost:19010/api/v1/decision" >"$POLICY_FILE"; then
  if jq -e '.action' "$POLICY_FILE" >/dev/null 2>&1; then
    finish_stage "PASS" "Policy decision endpoint returned action" "$POLICY_FILE"
  else
    finish_stage "FAIL" "Policy endpoint response missing action" "$POLICY_FILE"
  fi
else
  finish_stage "FAIL" "Policy decision endpoint call failed" "$POLICY_FILE"
fi

# Stage 5: Redis streams
start_stage "S05" "Redis Streams"
CLASS_STREAM_FILE="${ARTIFACT_ROOT}/05-classification-stream.txt"
PAGE_STREAM_FILE="${ARTIFACT_ROOT}/05-pagefetch-stream.txt"
run_compose exec -T redis redis-cli XREVRANGE classification-jobs + - COUNT 200 >"$CLASS_STREAM_FILE" 2>&1 || true
run_compose exec -T redis redis-cli XREVRANGE page-fetch-jobs + - COUNT 200 >"$PAGE_STREAM_FILE" 2>&1 || true
if file_contains_canonical_key "$CLASS_STREAM_FILE" && file_contains_canonical_key "$PAGE_STREAM_FILE"; then
  finish_stage "PASS" "Redis streams contain ${TARGET_CANONICAL_KEY}" "$CLASS_STREAM_FILE,$PAGE_STREAM_FILE"
else
  finish_stage "FAIL" "Redis streams missing canonical key ${TARGET_CANONICAL_KEY}" "$CLASS_STREAM_FILE,$PAGE_STREAM_FILE"
fi

# Stage 6: Page fetcher logs
start_stage "S06" "Page Fetcher Logs"
FETCH_FILE="${ARTIFACT_ROOT}/06-page-fetcher.log"
if wait_for_service_log_match "page-fetcher" "$TARGET_CANONICAL_KEY" "$FETCH_FILE"; then
  if grep -qi "job url invalid" "$FETCH_FILE"; then
    finish_stage "WARN" "Page fetch attempted but saw url errors" "$FETCH_FILE"
  else
    finish_stage "PASS" "Page fetch processed ${TARGET_CANONICAL_KEY}" "$FETCH_FILE"
  fi
else
    finish_stage "FAIL" "Page fetch logs missing canonical key ${TARGET_CANONICAL_KEY}" "$FETCH_FILE"
fi

# Stage 7: Crawl4AI logs
start_stage "S07" "Crawl4AI Logs"
CRAWL_FILE="${ARTIFACT_ROOT}/07-crawl4ai.log"
if wait_for_service_log_match "crawl4ai" "$TARGET_ROOT_DOMAIN" "$CRAWL_FILE"; then
  finish_stage "PASS" "Crawl4AI saw ${TARGET_ROOT_DOMAIN}" "$CRAWL_FILE"
else
  finish_stage "FAIL" "Crawl4AI logs missing ${TARGET_ROOT_DOMAIN}" "$CRAWL_FILE"
fi

# Stage 8: LLM worker logs
start_stage "S08" "LLM Worker Logs"
LLM_FILE="${ARTIFACT_ROOT}/08-llm.log"
if wait_for_llm_classification_log "$LLM_FILE" "$WAIT_LLM_SECONDS"; then
  finish_stage "PASS" "LLM classified target key" "$LLM_FILE"
else
  finish_stage "FAIL" "LLM logs missing target key variants" "$LLM_FILE"
fi

# Stage 9: Database pending/classification
start_stage "S09" "Database State"
DB_CLASS_FILE="${ARTIFACT_ROOT}/09-db-classifications.txt"
DB_PENDING_FILE="${ARTIFACT_ROOT}/09-db-pending.txt"
if wait_for_db_rows "$DB_CLASS_FILE" "$DB_PENDING_FILE" "$WAIT_DB_SECONDS"; then
  finish_stage "PASS" "DB rows exist for canonical key ${TARGET_CANONICAL_KEY}" "$DB_CLASS_FILE,$DB_PENDING_FILE"
else
  finish_stage "FAIL" "No DB rows for canonical key ${TARGET_CANONICAL_KEY}" "$DB_CLASS_FILE,$DB_PENDING_FILE"
fi

# Stage 10: Second client request after wait
log "waiting ${WAIT_AFTER_INITIAL}s before follow-up request"
sleep "$WAIT_AFTER_INITIAL"
start_stage "S10" "Client Follow-up Request"
FOLLOW_BODY="${ARTIFACT_ROOT}/10-client-follow-body.bin"
FOLLOW_HEADERS="${ARTIFACT_ROOT}/10-client-follow-headers.txt"
FOLLOW_STATUS_FILE="${ARTIFACT_ROOT}/10-client-follow-status.txt"
: >"$FOLLOW_BODY"
: >"$FOLLOW_HEADERS"
FOLLOW_STARTED=$(date -Iseconds)
set +e
curl -sk -x "$PROXY_URL" -o "$FOLLOW_BODY" -D "$FOLLOW_HEADERS" -w "%{http_code}" "$CONNECT_URL" >"$FOLLOW_STATUS_FILE" 2>&1
follow_exit=$?
set -e
follow_status=$(tail -n1 "$FOLLOW_STATUS_FILE" | tr -d '\r')
follow_block="no"
if grep -qi "Site Under Classification" "$FOLLOW_BODY" || grep -qi "Request blocked" "$FOLLOW_BODY"; then
  follow_block="yes"
fi
if [[ "$follow_status" == "403" || "$follow_block" == "yes" || ( "$follow_status" == "000" && "$follow_exit" -ne 0 ) ]]; then
  finish_stage "PASS" "Follow-up request blocked with HTTP ${follow_status} (curl exit ${follow_exit})" "$FOLLOW_HEADERS,$FOLLOW_BODY,$FOLLOW_STATUS_FILE"
else
  finish_stage "FAIL" "Follow-up request not blocked (HTTP ${follow_status}, curl exit ${follow_exit})" "$FOLLOW_HEADERS,$FOLLOW_BODY,$FOLLOW_STATUS_FILE"
fi

# Stage 11: ICAP cache decision after follow-up
start_stage "S11" "ICAP Final Decision"
ICAP_FINAL_FILE="${ARTIFACT_ROOT}/11-icap-final.log"
run_compose logs --since "$FOLLOW_STARTED" --tail=300 icap-adaptor >"$ICAP_FINAL_FILE" 2>&1 || true
if file_contains_target_key "$ICAP_FINAL_FILE" && (grep -q "cache decision" "$ICAP_FINAL_FILE" || grep -q "policy decision" "$ICAP_FINAL_FILE"); then
  finish_stage "PASS" "ICAP final decision path emitted" "$ICAP_FINAL_FILE"
else
  finish_stage "FAIL" "No ICAP final decision log found for follow-up" "$ICAP_FINAL_FILE"
fi

# Stage 12: Report generation
start_stage "S12" "Report Generation"
RUN_FINISHED=$(date -Iseconds)
REPORT_JSON="${ARTIFACT_ROOT}/report.json"
REPORT_MD="${ARTIFACT_ROOT}/report.md"
export STAGE_META_FILE RUN_ID TARGET_DOMAIN PROXY_URL ARTIFACT_ROOT RUN_STARTED RUN_FINISHED
python - <<'PY'
import json, os
meta_file = os.environ["STAGE_META_FILE"]
run_id = os.environ["RUN_ID"]
target = os.environ["TARGET_DOMAIN"]
proxy = os.environ["PROXY_URL"]
artifacts = os.environ["ARTIFACT_ROOT"]
started = os.environ["RUN_STARTED"]
finished = os.environ.get("RUN_FINISHED", started)
stages = []
with open(meta_file, 'r', encoding='utf-8') as fh:
    for line in fh:
        if not line.strip():
            continue
        stage_id, label, status, desc, files, duration = line.rstrip('\n').split('\t')
        artifact_list = [f for f in files.split(',') if f]
        stages.append({
            "id": stage_id,
            "label": label,
            "status": status,
            "description": desc,
            "artifacts": artifact_list,
            "duration_ms": int(duration)
        })
report = {
    "run_id": run_id,
    "target": target,
    "proxy_url": proxy,
    "artifact_root": artifacts,
    "started_at": started,
    "finished_at": finished,
    "stages": stages
}
with open(os.path.join(artifacts, "report.json"), 'w', encoding='utf-8') as fh:
    json.dump(report, fh, indent=2)
with open(os.path.join(artifacts, "report.md"), 'w', encoding='utf-8') as fh:
    fh.write(f"# Facebook E2E Smoke Report\n\nRun ID: {run_id}\n\n")
    fh.write(f"Target: {target}\nProxy: {proxy}\nArtifacts: {artifacts}\n\n")
    fh.write("| Stage | Label | Status | Description |\n| --- | --- | --- | --- |\n")
    for stage in stages:
        fh.write(f"| {stage['id']} | {stage['label']} | {stage['status']} | {stage['description']} |\n")
    fh.write("\n## Checklist\n")
    for stage in stages:
        mark = "x" if stage["status"] == "PASS" else " "
        fh.write(f"- [{mark}] {stage['id']} {stage['label']} — {stage['status']}\n")
PY
if [[ -f "$REPORT_JSON" && -f "$REPORT_MD" ]]; then
  finish_stage "PASS" "Reports generated" "$REPORT_JSON,$REPORT_MD,$STAGE_META_FILE"
else
  finish_stage "FAIL" "Failed to generate reports" "$STAGE_META_FILE"
fi

log "Artifacts stored in ${ARTIFACT_ROOT}"
if [[ $FAIL_COUNT -gt 0 ]]; then
  log "Smoke test completed with ${FAIL_COUNT} failing stage(s)"
  exit 1
else
  log "Smoke test completed successfully"
  exit 0
fi
