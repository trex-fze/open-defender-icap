#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)

PROFILE=${PROFILE:-golden-prodlike}
COMPOSE_ENV_FILE=${COMPOSE_ENV_FILE:-"$ROOT_DIR/.env"}
RUN_ID=${RUN_ID:-"release-gate-$(date +%Y%m%d-%H%M%S)"}
ARTIFACT_ROOT=${ARTIFACT_ROOT:-"$ROOT_DIR/tests/artifacts/release-gate/${RUN_ID}"}
AUTO_TEARDOWN=${AUTO_TEARDOWN:-0}

INTEGRATION_BUILD=${INTEGRATION_BUILD:-1}
RELIABILITY_RUNS=${RELIABILITY_RUNS:-10}
RELIABILITY_WAIT_STAGE_SECONDS=${RELIABILITY_WAIT_STAGE_SECONDS:-45}
RELIABILITY_WAIT_LLM_SECONDS=${RELIABILITY_WAIT_LLM_SECONDS:-120}
RELIABILITY_WAIT_DB_SECONDS=${RELIABILITY_WAIT_DB_SECONDS:-180}
RELIABILITY_PASS_THRESHOLD=${RELIABILITY_PASS_THRESHOLD:-90}
RELIABILITY_OUT_DIR=${RELIABILITY_OUT_DIR:-"$ARTIFACT_ROOT/facebook-e2e-reliability"}

RUNBOOK_EVIDENCE_FILE=${RUNBOOK_EVIDENCE_FILE:-}

mkdir -p "$ARTIFACT_ROOT"
SUMMARY_TSV="$ARTIFACT_ROOT/stage-summary.tsv"
SUMMARY_MD="$ARTIFACT_ROOT/summary.md"
SUMMARY_JSON="$ARTIFACT_ROOT/summary.json"
RUN_LOG="$ARTIFACT_ROOT/release-gate.log"

printf 'stage\tstatus\tduration_sec\tlog\n' >"$SUMMARY_TSV"

log() {
  printf '[release-gate][%s] %s\n' "$(date -Iseconds)" "$*" | tee -a "$RUN_LOG"
}

run_stage() {
  local stage="$1"
  local command="$2"
  local log_file="$ARTIFACT_ROOT/${stage}.log"
  local start end duration status

  log "START ${stage}: ${command}"
  start=$(date +%s)
  if bash -lc "$command" >"$log_file" 2>&1; then
    status="PASS"
  else
    status="FAIL"
  fi
  end=$(date +%s)
  duration=$((end - start))

  printf '%s\t%s\t%s\t%s\n' "$stage" "$status" "$duration" "$log_file" >>"$SUMMARY_TSV"
  log "END ${stage}: ${status} (${duration}s)"

  if [[ "$status" != "PASS" ]]; then
    return 1
  fi
  return 0
}

collect_failure_context() {
  local out_dir="$ARTIFACT_ROOT/ops-triage"
  log "Collecting failure diagnostics"
  OUT_DIR="$out_dir" COMPOSE_ENV_FILE="$COMPOSE_ENV_FILE" \
    bash "$ROOT_DIR/tests/ops/platform-diagnostics.sh" >"$ARTIFACT_ROOT/failure-diagnostics.log" 2>&1 || true
}

write_summary_reports() {
  local overall="PASS"
  if grep -q $'\tFAIL\t' "$SUMMARY_TSV"; then
    overall="FAIL"
  fi

  {
    echo "# Release Gate Summary"
    echo
    echo "- run_id: \\`$RUN_ID\\`"
    echo "- profile: \\`$PROFILE\\`"
    echo "- overall: **$overall**"
    echo "- artifacts: \\`$ARTIFACT_ROOT\\`"
    echo
    echo "| Stage | Status | Duration (s) | Log |"
    echo "| --- | --- | ---: | --- |"
    awk -F '\t' 'NR>1 { printf("| %s | %s | %s | `%s` |\n", $1, $2, $3, $4) }' "$SUMMARY_TSV"
  } >"$SUMMARY_MD"

  python3 - "$SUMMARY_TSV" "$SUMMARY_JSON" "$RUN_ID" "$PROFILE" "$overall" <<'PY'
import json
import pathlib
import sys

tsv_path = pathlib.Path(sys.argv[1])
json_path = pathlib.Path(sys.argv[2])
run_id = sys.argv[3]
profile = sys.argv[4]
overall = sys.argv[5]

stages = []
for line in tsv_path.read_text().splitlines()[1:]:
    stage, status, duration, log_path = line.split("\t")
    stages.append(
        {
            "stage": stage,
            "status": status,
            "duration_sec": int(duration),
            "log": log_path,
        }
    )

payload = {
    "run_id": run_id,
    "profile": profile,
    "overall": overall,
    "stages": stages,
}
json_path.write_text(json.dumps(payload, indent=2))
PY
}

maybe_teardown() {
  if [[ "$AUTO_TEARDOWN" == "1" ]]; then
    log "Tearing down golden profile"
    PROFILE="$PROFILE" COMPOSE_ENV_FILE="$COMPOSE_ENV_FILE" \
      bash "$ROOT_DIR/tests/ops/golden-profile.sh" down >>"$RUN_LOG" 2>&1 || true
  fi
}

main() {
  log "Release gate started (profile=$PROFILE run_id=$RUN_ID)"

  if [[ "$PROFILE" != "golden-prodlike" ]]; then
    log "WARNING: profile is '$PROFILE' (recommended: golden-prodlike)"
  fi

  trap 'maybe_teardown; write_summary_reports' EXIT

  run_stage "01_profile_up" "PROFILE='$PROFILE' COMPOSE_ENV_FILE='$COMPOSE_ENV_FILE' bash '$ROOT_DIR/tests/ops/golden-profile.sh' up"
  run_stage "02_integration" "INTEGRATION_BUILD='$INTEGRATION_BUILD' COMPOSE_ENV_FILE='$COMPOSE_ENV_FILE' bash '$ROOT_DIR/tests/integration.sh'"
  run_stage "03_authz_smoke" "bash '$ROOT_DIR/tests/security/authz-smoke.sh'"
  run_stage "04_facebook_smoke" "COMPOSE_ENV_FILE='$COMPOSE_ENV_FILE' bash '$ROOT_DIR/tests/security/facebook-e2e-smoke.sh'"
  run_stage "05_facebook_reliability" "OUT_DIR='$RELIABILITY_OUT_DIR' RUNS='$RELIABILITY_RUNS' WAIT_STAGE_SECONDS='$RELIABILITY_WAIT_STAGE_SECONDS' WAIT_LLM_SECONDS='$RELIABILITY_WAIT_LLM_SECONDS' WAIT_DB_SECONDS='$RELIABILITY_WAIT_DB_SECONDS' AUTO_STACK_BOOTSTRAP=0 AUTO_STACK_TEARDOWN=0 bash '$ROOT_DIR/tests/security/facebook-e2e-reliability.sh'"
  run_stage "06_platform_diagnostics" "OUT_DIR='$ARTIFACT_ROOT/platform-diagnostics' COMPOSE_ENV_FILE='$COMPOSE_ENV_FILE' bash '$ROOT_DIR/tests/ops/platform-diagnostics.sh'"

  if [[ -n "$RUNBOOK_EVIDENCE_FILE" ]]; then
    run_stage "07_runbook_evidence_check" "test -f '$RUNBOOK_EVIDENCE_FILE'"
  else
    log "Skipping explicit runbook evidence file check (RUNBOOK_EVIDENCE_FILE unset)"
  fi

  local reliability_summary
  reliability_summary=$(ls -t "$RELIABILITY_OUT_DIR"/summary-*.txt 2>/dev/null | head -n1 || true)
  local pass_rate
  pass_rate=""
  if [[ -n "$reliability_summary" ]]; then
    pass_rate=$(awk -F'[=% ]+' '/pass_rate=/{print $7}' "$reliability_summary" | tail -n1 || true)
  fi
  if [[ -n "$pass_rate" && "$pass_rate" -lt "$RELIABILITY_PASS_THRESHOLD" ]]; then
    log "FAIL: reliability pass rate ${pass_rate}% is below threshold ${RELIABILITY_PASS_THRESHOLD}%"
    exit 1
  fi

  log "Release gate completed successfully"
}

if ! main; then
  collect_failure_context
  log "Release gate failed"
  exit 1
fi
