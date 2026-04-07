#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
OUT_DIR=${OUT_DIR:-"$ROOT_DIR/tests/artifacts/taxonomy-parity"}
TS=$(date +%Y%m%d%H%M%S)
RUN_DIR="$OUT_DIR/$TS"
mkdir -p "$RUN_DIR"

run_case() {
  local name="$1"
  local cmd="$2"
  local log_file="$RUN_DIR/${name}.log"
  if bash -lc "$cmd" >"$log_file" 2>&1; then
    printf '%s\tPASS\t%s\n' "$name" "$log_file" | tee -a "$RUN_DIR/summary.tsv"
  else
    printf '%s\tFAIL\t%s\n' "$name" "$log_file" | tee -a "$RUN_DIR/summary.tsv"
    return 1
  fi
}

: >"$RUN_DIR/summary.tsv"

run_case "llm-worker-canonical-labels" "cd '$ROOT_DIR' && cargo test -p llm-worker classification_persists_canonical_labels_and_flags"
run_case "reclass-worker-canonicalization" "cd '$ROOT_DIR' && cargo test -p reclass-worker planner_canonicalizes_legacy_labels"
run_case "policy-engine-activation" "cd '$ROOT_DIR' && cargo test -p policy-engine unknown_toggle_controls_decision"
run_case "policy-engine-decision-block" "cd '$ROOT_DIR' && cargo test -p policy-engine decision_blocked_when_category_matches"
run_case "admin-api-classification-keys" "cd '$ROOT_DIR' && cargo test -p admin-api parses_domain_and_subdomain_keys"

printf 'taxonomy parity artifacts: %s\n' "$RUN_DIR"
