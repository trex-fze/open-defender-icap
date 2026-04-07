# Stage 20 Checklist - Ops Diagnostics and Runbook Automation

## Collector Script
- [x] Add diagnostics collector for pending/content pipeline.
- [x] Capture DB snapshots (`classification_requests`, `page_contents`, `classifications`).
- [x] Capture worker/service logs (`llm-worker`, `page-fetcher`, `reclass-worker`, `admin-api`, `icap-adaptor`).
- [x] Capture queue snapshots (`classification-jobs`, `page-fetch-jobs`).

## Runbook
- [ ] Add collector usage examples.
- [ ] Add artifact interpretation guide.
- [ ] Add escalation criteria with thresholds.

## Validation
- [x] Run collector on a known key and verify artifact completeness.
- [x] Record sample output in verification log.

## Completion
- [ ] Mark Stage 20 complete in `implementation-plan/stage-plan.md`.
