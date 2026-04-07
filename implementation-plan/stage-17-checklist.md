# Stage 17 Checklist - ContentPending Reliability and Terminalization

## Baseline and Repro
- [x] Run `tests/content-pending-smoke.sh` >= 10 times and record pass/fail + duration.
- [x] Capture where failures happen (pending enqueue, page fetch, LLM verdict, persistence).

## Diagnostics Coverage
- [x] Smoke timeout captures DB snapshots for `classification_requests`, `page_contents`, `classifications`.
- [x] Smoke timeout captures recent logs from `llm-worker`, `page-fetcher`, `reclass-worker`, `admin-api`.
- [x] Evidence files are linked for each timeout run.

## Runtime Hardening
- [ ] Add/verify bounded retry semantics for classifier paths.
- [ ] Ensure terminal fallback outcomes are persisted when retries exhaust.
- [ ] Add/verify pending-age metric and classification latency metric.

## Smoke/CI Stability
- [x] Make wait budgets configurable (`WAIT_PAGE_TRIES`, `WAIT_CLASSIFICATION_TRIES`).
- [x] Tune default waits to match realistic compose latency envelope.
- [x] Add a multi-run helper script or documented command set for reliability sweeps.

## Documentation
- [ ] Update operator runbook with delayed-terminalization triage steps.
- [x] Add Stage 17 verification log with reliability statistics.

## Completion Gate
- [x] 10-run reliability baseline collected and reviewed.
- [x] >= 90% pass rate achieved (or all failures root-caused with diagnostics).
- [ ] Stage 17 marked complete in `implementation-plan/stage-plan.md`.
