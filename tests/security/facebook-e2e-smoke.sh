#!/usr/bin/env bash

set -euo pipefail

PROXY_URL=${PROXY_URL:-http://localhost:3128}
TARGET_DOMAIN=${TARGET_DOMAIN:-www.facebook.com}
TARGET_ROOT_DOMAIN=${TARGET_ROOT_DOMAIN:-facebook.com}
TARGET_NORMALIZED_KEY=${TARGET_NORMALIZED_KEY:-subdomain:www.facebook.com}
SOCIAL_CATEGORY_ID=${SOCIAL_CATEGORY_ID:-social-media}
ADMIN_API_URL=${ADMIN_API_URL:-http://localhost:19000}
ADMIN_TOKEN=${ADMIN_TOKEN:-${OD_ADMIN_TOKEN:-changeme-admin}}
COMPOSE_FILE=${COMPOSE_FILE:-deploy/docker/docker-compose.yml}
RUN_ID=${RUN_ID:-$(date +%Y%m%d-%H%M%S)}
ARTIFACT_ROOT=${ARTIFACT_ROOT:-tests/artifacts/facebook-e2e/${RUN_ID}}
WAIT_AFTER_INITIAL=${WAIT_AFTER_INITIAL:-45}
CONNECT_URL="https://${TARGET_DOMAIN}"

mkdir -p "${ARTIFACT_ROOT}"
STAGE_META_FILE="${ARTIFACT_ROOT}/stage-metadata.tsv"
touch "${STAGE_META_FILE}"
RUN_LOG="${ARTIFACT_ROOT}/run.log"
RUN_STARTED=$(date -Iseconds)
FAIL_COUNT=0

log() {
  local msg=$1
  printf '[%s] %s\n' "$(date -Iseconds)" "$msg" | tee -a "$RUN_LOG"
}

CURRENT_STAGE=""
CURRENT_LABEL=""
STAGE_START=0

start_stage() {
  CURRENT_STAGE=$1
  CURRENT_LABEL=$2
  STAGE_START=$(date +%s%3N)
  log "=== Stage ${CURRENT_STAGE}: ${CURRENT_LABEL} ==="
}

finish_stage() {
  local status=$1
  local desc=$2
  local files=${3:-}
  local end=$(date +%s%3N)
  local duration=$((end - STAGE_START))
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$CURRENT_STAGE" "$CURRENT_LABEL" "$status" "$desc" "$files" "$duration" >> "$STAGE_META_FILE"
  log "--- Stage ${CURRENT_STAGE} status: ${status} (${desc}) ---"
  if [[ "$status" == "FAIL" ]]; then
    FAIL_COUNT=$((FAIL_COUNT + 1))
  fi
}

run_compose() {
  docker compose -f "$COMPOSE_FILE" "$@"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    log "missing required command: $1"
    exit 1
  fi
}

require_command jq
require_command curl

# Stage 0: Preflight
start_stage "S00" "Preflight"
PRE_PS_FILE="${ARTIFACT_ROOT}/00-preflight-ps.txt"
run_compose ps >"$PRE_PS_FILE"
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
    PRE_LLM_FILE="${ARTIFACT_ROOT}/00-llm-health.txt"
    llm_ready=""
    if run_compose exec -T llm-worker curl -sS --max-time 5 http://192.168.1.170:1234/v1/models >"$PRE_LLM_FILE" 2>&1; then
      llm_ready="local"
    else
      if run_compose exec -T llm-worker printenv OPENAI_API_KEY | tr -d '\r' >"$PRE_LLM_FILE" 2>&1 && grep -q '\S' "$PRE_LLM_FILE"; then
        llm_ready="openai"
      fi
    fi
    if [[ ${#missing[@]} -gt 0 ]]; then
      finish_stage "FAIL" "Missing services: ${missing[*]}" "$PRE_PS_FILE,$PRE_TAX_FILE,$PRE_LLM_FILE"
    elif [[ "$social_state" != "false" ]]; then
      finish_stage "FAIL" "Category ${SOCIAL_CATEGORY_ID} is enabled" "$PRE_PS_FILE,$PRE_TAX_FILE,$PRE_LLM_FILE"
    elif [[ -z "$llm_ready" ]]; then
      finish_stage "FAIL" "LLM providers unavailable" "$PRE_PS_FILE,$PRE_TAX_FILE,$PRE_LLM_FILE"
    else
      finish_stage "PASS" "Services healthy; category disabled; LLM ready via ${llm_ready}" "$PRE_PS_FILE,$PRE_TAX_FILE,$PRE_LLM_FILE"
    fi
  fi
fi

# Stage 1: Client initial request
start_stage "S01" "Client Initial Request"
INIT_BODY="${ARTIFACT_ROOT}/01-client-initial-body.bin"
INIT_HEADERS="${ARTIFACT_ROOT}/01-client-initial-headers.txt"
INIT_STATUS_FILE="${ARTIFACT_ROOT}/01-client-initial-status.txt"
curl -sk -x "$PROXY_URL" -o "$INIT_BODY" -D "$INIT_HEADERS" -w "%{http_code}" "$CONNECT_URL" >"$INIT_STATUS_FILE" 2>&1 || true
http_status=$(tail -n1 "$INIT_STATUS_FILE" | tr -d '\r')
block_detected="no"
if grep -qi "Site Under Classification" "$INIT_BODY" || grep -qi "Request blocked" "$INIT_BODY"; then
  block_detected="yes"
fi
if [[ "$http_status" == "403" || "$block_detected" == "yes" ]]; then
  finish_stage "PASS" "Initial request blocked with HTTP ${http_status}" "$INIT_HEADERS,$INIT_BODY,$INIT_STATUS_FILE"
else
  finish_stage "FAIL" "Initial request not blocked (HTTP ${http_status})" "$INIT_HEADERS,$INIT_BODY,$INIT_STATUS_FILE"
fi

# Stage 2: Squid observation
start_stage "S02" "Squid Logs"
SQUID_FILE="${ARTIFACT_ROOT}/02-squid.log"
run_compose logs --tail=400 squid >"$SQUID_FILE" 2>&1 || true
if grep -q "CONNECT ${TARGET_DOMAIN}:443" "$SQUID_FILE"; then
  finish_stage "PASS" "Squid observed CONNECT ${TARGET_DOMAIN}" "$SQUID_FILE"
else
  finish_stage "FAIL" "CONNECT ${TARGET_DOMAIN} missing in squid logs" "$SQUID_FILE"
fi

# Stage 3: ICAP adaptor observation
start_stage "S03" "ICAP Logs"
ICAP_FILE="${ARTIFACT_ROOT}/03-icap.log"
run_compose logs --tail=400 icap-adaptor >"$ICAP_FILE" 2>&1 || true
if grep -q "$TARGET_NORMALIZED_KEY" "$ICAP_FILE"; then
  finish_stage "PASS" "ICAP handled ${TARGET_NORMALIZED_KEY}" "$ICAP_FILE"
else
  finish_stage "FAIL" "ICAP log missing ${TARGET_NORMALIZED_KEY}" "$ICAP_FILE"
fi

# Stage 4: Policy engine observation
start_stage "S04" "Policy Engine Logs"
POLICY_FILE="${ARTIFACT_ROOT}/04-policy.log"
run_compose logs --tail=400 policy-engine >"$POLICY_FILE" 2>&1 || true
if grep -q "$TARGET_NORMALIZED_KEY" "$POLICY_FILE"; then
  finish_stage "PASS" "Policy decision logged for ${TARGET_NORMALIZED_KEY}" "$POLICY_FILE"
else
  finish_stage "FAIL" "Policy log missing ${TARGET_NORMALIZED_KEY}" "$POLICY_FILE"
fi

# Stage 5: Redis streams
start_stage "S05" "Redis Streams"
CLASS_STREAM_FILE="${ARTIFACT_ROOT}/05-classification-stream.txt"
PAGE_STREAM_FILE="${ARTIFACT_ROOT}/05-pagefetch-stream.txt"
run_compose exec -T redis redis-cli XREVRANGE classification-jobs + - COUNT 200 >"$CLASS_STREAM_FILE" 2>&1 || true
run_compose exec -T redis redis-cli XREVRANGE page-fetch-jobs + - COUNT 200 >"$PAGE_STREAM_FILE" 2>&1 || true
if grep -q "$TARGET_NORMALIZED_KEY" "$CLASS_STREAM_FILE" && grep -q "$TARGET_NORMALIZED_KEY" "$PAGE_STREAM_FILE"; then
  finish_stage "PASS" "Redis streams contain ${TARGET_NORMALIZED_KEY}" "$CLASS_STREAM_FILE,$PAGE_STREAM_FILE"
else
  finish_stage "FAIL" "Redis streams missing ${TARGET_NORMALIZED_KEY}" "$CLASS_STREAM_FILE,$PAGE_STREAM_FILE"
fi

# Stage 6: Page fetcher logs
start_stage "S06" "Page Fetcher Logs"
FETCH_FILE="${ARTIFACT_ROOT}/06-page-fetcher.log"
run_compose logs --tail=400 page-fetcher >"$FETCH_FILE" 2>&1 || true
if grep -q "$TARGET_NORMALIZED_KEY" "$FETCH_FILE"; then
  if grep -qi "job url invalid" "$FETCH_FILE"; then
    finish_stage "WARN" "Page fetch attempted but saw url errors" "$FETCH_FILE"
  else
    finish_stage "PASS" "Page fetch processed ${TARGET_NORMALIZED_KEY}" "$FETCH_FILE"
  fi
else
  finish_stage "FAIL" "Page fetch logs missing ${TARGET_NORMALIZED_KEY}" "$FETCH_FILE"
fi

# Stage 7: Crawl4AI logs
start_stage "S07" "Crawl4AI Logs"
CRAWL_FILE="${ARTIFACT_ROOT}/07-crawl4ai.log"
run_compose logs --tail=400 crawl4ai >"$CRAWL_FILE" 2>&1 || true
if grep -q "$TARGET_ROOT_DOMAIN" "$CRAWL_FILE"; then
  finish_stage "PASS" "Crawl4AI saw ${TARGET_ROOT_DOMAIN}" "$CRAWL_FILE"
else
  finish_stage "WARN" "Crawl4AI logs missing ${TARGET_ROOT_DOMAIN}" "$CRAWL_FILE"
fi

# Stage 8: LLM worker logs
start_stage "S08" "LLM Worker Logs"
LLM_FILE="${ARTIFACT_ROOT}/08-llm.log"
run_compose logs --tail=400 llm-worker >"$LLM_FILE" 2>&1 || true
if grep -q "$TARGET_NORMALIZED_KEY" "$LLM_FILE"; then
  if grep -qi "failed to process job" "$LLM_FILE"; then
    finish_stage "WARN" "LLM saw ${TARGET_NORMALIZED_KEY} but errors present" "$LLM_FILE"
  else
    finish_stage "PASS" "LLM classified ${TARGET_NORMALIZED_KEY}" "$LLM_FILE"
  fi
else
  finish_stage "FAIL" "LLM logs missing ${TARGET_NORMALIZED_KEY}" "$LLM_FILE"
fi

# Stage 9: Database pending/classification
start_stage "S09" "Database State"
DB_CLASS_FILE="${ARTIFACT_ROOT}/09-db-classifications.txt"
DB_PENDING_FILE="${ARTIFACT_ROOT}/09-db-pending.txt"
run_compose exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"SELECT normalized_key,recommended_action,updated_at FROM classifications WHERE normalized_key ILIKE '%${TARGET_ROOT_DOMAIN}%' ORDER BY updated_at DESC LIMIT 10;\"" >"$DB_CLASS_FILE" 2>&1 || true
run_compose exec -T postgres bash -lc "psql -U defender -d defender_admin -At -c \"SELECT normalized_key,status,last_error,updated_at FROM classification_requests WHERE normalized_key ILIKE '%${TARGET_ROOT_DOMAIN}%' ORDER BY updated_at DESC LIMIT 10;\"" >"$DB_PENDING_FILE" 2>&1 || true
if grep -q "$TARGET_ROOT_DOMAIN" "$DB_CLASS_FILE" || grep -q "$TARGET_ROOT_DOMAIN" "$DB_PENDING_FILE"; then
  finish_stage "PASS" "DB rows exist for ${TARGET_ROOT_DOMAIN}" "$DB_CLASS_FILE,$DB_PENDING_FILE"
else
  finish_stage "FAIL" "No DB rows for ${TARGET_ROOT_DOMAIN}" "$DB_CLASS_FILE,$DB_PENDING_FILE"
fi

# Stage 10: Second client request after wait
log "waiting ${WAIT_AFTER_INITIAL}s before follow-up request"
sleep "$WAIT_AFTER_INITIAL"
start_stage "S10" "Client Follow-up Request"
FOLLOW_BODY="${ARTIFACT_ROOT}/10-client-follow-body.bin"
FOLLOW_HEADERS="${ARTIFACT_ROOT}/10-client-follow-headers.txt"
FOLLOW_STATUS_FILE="${ARTIFACT_ROOT}/10-client-follow-status.txt"
curl -sk -x "$PROXY_URL" -o "$FOLLOW_BODY" -D "$FOLLOW_HEADERS" -w "%{http_code}" "$CONNECT_URL" >"$FOLLOW_STATUS_FILE" 2>&1 || true
follow_status=$(tail -n1 "$FOLLOW_STATUS_FILE" | tr -d '\r')
follow_block="no"
if grep -qi "Site Under Classification" "$FOLLOW_BODY" || grep -qi "Request blocked" "$FOLLOW_BODY"; then
  follow_block="yes"
fi
if [[ "$follow_status" == "403" || "$follow_block" == "yes" ]]; then
  finish_stage "PASS" "Follow-up request blocked with HTTP ${follow_status}" "$FOLLOW_HEADERS,$FOLLOW_BODY,$FOLLOW_STATUS_FILE"
else
  finish_stage "FAIL" "Follow-up request not blocked (HTTP ${follow_status})" "$FOLLOW_HEADERS,$FOLLOW_BODY,$FOLLOW_STATUS_FILE"
fi

# Stage 11: ICAP cache decision after follow-up
start_stage "S11" "ICAP Final Decision"
ICAP_FINAL_FILE="${ARTIFACT_ROOT}/11-icap-final.log"
run_compose logs --tail=200 icap-adaptor >"$ICAP_FINAL_FILE" 2>&1 || true
if grep -q "cache decision" "$ICAP_FINAL_FILE" && grep -q "$TARGET_NORMALIZED_KEY" "$ICAP_FINAL_FILE"; then
  finish_stage "PASS" "ICAP cache decision emitted" "$ICAP_FINAL_FILE"
else
  finish_stage "WARN" "No cache decision log found" "$ICAP_FINAL_FILE"
fi

# Stage 12: Report generation
start_stage "S12" "Report Generation"
RUN_FINISHED=$(date -Iseconds)
REPORT_JSON="${ARTIFACT_ROOT}/report.json"
REPORT_MD="${ARTIFACT_ROOT}/report.md"
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
