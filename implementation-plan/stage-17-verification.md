# Stage 17 Verification Log

Date: 2026-04-07

## Implemented in this iteration

- Added timeout diagnostics to `tests/content-pending-smoke.sh`:
  - DB snapshots on timeout (`classification_requests`, `page_contents`, `classifications`)
  - Service logs on timeout (`llm-worker`, `page-fetcher`, `reclass-worker`, `admin-api`)
  - Configurable wait budgets (`WAIT_PAGE_TRIES`, `WAIT_CLASSIFICATION_TRIES`)
  - Artifact directory cleanup at run start to avoid stale evidence confusion
- Added reliability sweep helper `tests/content-pending-reliability.sh`.

## Reliability baseline and analysis

- Baseline command:
  - `RUNS=10 KEEP_STACK=1 BUILD_IMAGES=0 bash tests/content-pending-reliability.sh`
- Baseline result:
  - `pass=4 fail=6 pass_rate=40%`

Dominant failure signatures from baseline:

1. `Timed out waiting for classification for domain:smoke-origin` (most frequent)
   - pending row present, terminal classification absent before timeout window
2. `Pending row still present after classification` (race in immediate pending-clear check)

Observed contributing factors:

- Background `reclass-worker` dispatches many unrelated reclassification jobs, increasing queue contention/noise.
- Diagnostic query fields were partially mismatched with schema (`error_message`, `reason`) reducing signal quality.

## Hardening implemented after baseline

- `tests/content-pending-smoke.sh` updates:
  - Increased classification wait budget default (`WAIT_CLASSIFICATION_TRIES=120`).
  - Added pending-clear wait loop (`WAIT_PENDING_CLEAR_TRIES`) instead of immediate fail.
  - Fixed diagnostic SQL field selection (`fetch_reason`, category/subcategory snapshot columns).
  - Added target-key filtered diagnostics (`grep` by `normalized_key`).
  - Added optional reclass quieting (`QUIET_RECLASS_WORKER=1` default) to reduce unrelated queue churn during smoke runs.

## Post-hardening sweep

- Command:
  - `RUNS=5 KEEP_STACK=1 BUILD_IMAGES=0 bash tests/content-pending-reliability.sh`
- Result:
  - `pass=5 fail=0 pass_rate=100%`

## Evidence capture note

- Reliability and timeout diagnostics are generated under:
  - `tests/artifacts/content-pending-reliability/`
  - `tests/artifacts/content-pending/diag-*`
- Artifacts are runtime evidence and intentionally not committed.

## Remaining gate

- Post-hardening 10-run gate command:
  - `RUNS=10 KEEP_STACK=1 BUILD_IMAGES=0 bash tests/content-pending-reliability.sh`
- Gate result:
  - `pass=10 fail=0 pass_rate=100%`
- Gate status:
  - Reliability pass-rate criterion met (>=90%).
