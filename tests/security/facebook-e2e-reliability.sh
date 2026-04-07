#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
RUNS=${RUNS:-10}
OUT_DIR=${OUT_DIR:-"$ROOT_DIR/tests/artifacts/facebook-e2e-reliability"}

mkdir -p "$OUT_DIR"
SUMMARY_FILE="$OUT_DIR/summary-$(date +%Y%m%d%H%M%S).txt"

pass=0
fail=0

printf 'facebook-e2e reliability run\n' >"$SUMMARY_FILE"
printf 'runs=%s\n' "$RUNS" >>"$SUMMARY_FILE"

for ((i = 1; i <= RUNS; i++)); do
  run_id="fb-e2e-$(date +%Y%m%d%H%M%S)-${i}"
  run_log="$OUT_DIR/run-${i}.log"
  start_ts=$(date +%s)
  if RUN_ID="$run_id" ARTIFACT_ROOT="$ROOT_DIR/tests/artifacts/facebook-e2e/${run_id}" bash "$ROOT_DIR/tests/security/facebook-e2e-smoke.sh" >"$run_log" 2>&1; then
    status="PASS"
    pass=$((pass + 1))
  else
    status="FAIL"
    fail=$((fail + 1))
  fi
  end_ts=$(date +%s)
  duration=$((end_ts - start_ts))
  printf 'run=%02d status=%s duration_sec=%s run_id=%s log=%s\n' "$i" "$status" "$duration" "$run_id" "$run_log" | tee -a "$SUMMARY_FILE"
done

rate=$((pass * 100 / RUNS))
printf 'result pass=%s fail=%s pass_rate=%s%%\n' "$pass" "$fail" "$rate" | tee -a "$SUMMARY_FILE"

if (( rate < 90 )); then
  exit 1
fi
