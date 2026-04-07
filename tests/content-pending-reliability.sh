#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
RUNS=${RUNS:-10}
KEEP_STACK=${KEEP_STACK:-1}
BUILD_IMAGES=${BUILD_IMAGES:-0}
OUT_DIR=${OUT_DIR:-"$ROOT_DIR/tests/artifacts/content-pending-reliability"}

mkdir -p "$OUT_DIR"
SUMMARY_FILE="$OUT_DIR/summary-$(date +%Y%m%d%H%M%S).txt"

pass=0
fail=0

printf 'content-pending reliability run\n' >"$SUMMARY_FILE"
printf 'runs=%s keep_stack=%s build_images=%s\n' "$RUNS" "$KEEP_STACK" "$BUILD_IMAGES" >>"$SUMMARY_FILE"

for ((i = 1; i <= RUNS; i++)); do
  start_ts=$(date +%s)
  run_log="$OUT_DIR/run-${i}.log"
  if KEEP_STACK=$KEEP_STACK BUILD_IMAGES=$BUILD_IMAGES "$ROOT_DIR/tests/content-pending-smoke.sh" >"$run_log" 2>&1; then
    status="PASS"
    pass=$((pass + 1))
  else
    status="FAIL"
    fail=$((fail + 1))
  fi
  end_ts=$(date +%s)
  duration=$((end_ts - start_ts))
  printf 'run=%02d status=%s duration_sec=%s log=%s\n' "$i" "$status" "$duration" "$run_log" | tee -a "$SUMMARY_FILE"
done

rate=$((pass * 100 / RUNS))
printf 'result pass=%s fail=%s pass_rate=%s%%\n' "$pass" "$fail" "$rate" | tee -a "$SUMMARY_FILE"

if [[ $fail -gt 0 ]]; then
  exit 1
fi
