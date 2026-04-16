# Release Gate Smoke Runbook

This runbook defines the production-like end-to-end smoke gate that must pass before release signoff.

## Recommended profile

- Use `golden-prodlike` to match release topology.
- Command:

```bash
PROFILE=golden-prodlike tests/release-gate.sh
```

## What the gate executes

1. `tests/ops/golden-profile.sh up`
2. `tests/integration.sh`
3. `tests/security/authz-smoke.sh`
4. `tests/security/facebook-e2e-smoke.sh`
5. `tests/security/facebook-e2e-reliability.sh`
6. `tests/ops/platform-diagnostics.sh`

Artifacts are collected under `tests/artifacts/release-gate/<run-id>/` including:

- stage logs
- summary (`summary.md`, `summary.json`, `stage-summary.tsv`)
- diagnostics bundle
- reliability outputs

## Required pass criteria

- All stages return `PASS` in `stage-summary.tsv`.
- Facebook reliability pass rate is at least `90%`.
- No unresolved critical issue in diagnostics for required components.

## Common options

- `AUTO_TEARDOWN=1`: bring down golden profile stack when run exits.
- `INTEGRATION_BUILD=0`: reuse existing images for faster reruns.
- `RELIABILITY_RUNS=10`: reliability iterations (increase for stricter gate).
- `RUNBOOK_EVIDENCE_FILE=<path>`: optional manual evidence file that must exist.

Example:

```bash
PROFILE=golden-prodlike \
AUTO_TEARDOWN=1 \
RELIABILITY_RUNS=10 \
RELIABILITY_WAIT_STAGE_SECONDS=45 \
RELIABILITY_WAIT_LLM_SECONDS=120 \
RELIABILITY_WAIT_DB_SECONDS=180 \
tests/release-gate.sh
```

## Failure handling

- On any stage failure, the script captures platform diagnostics automatically.
- Start triage from:
  - `summary.md`
  - failed stage log from `stage-summary.tsv`
  - `ops-triage/` diagnostics
