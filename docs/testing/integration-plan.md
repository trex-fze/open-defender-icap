# Stage 7 – Integration & Smoke Test Plan

This document defines the compose-driven integration validation used for Stage 7 and Stage 9 flows (Spec sections on system integration, security smoke, and content-aware classification).

## Scope and objectives

- Validate end-to-end request handling for the full local stack.
- Prove ICAP smoke, ingest pipeline, page-fetch enrichment, and content-first classification all work together.
- Provide a repeatable operator runbook for transient Docker/BuildKit instability.

## Compose stack baseline

- Compose file: `deploy/docker/docker-compose.yml`
- Requirements:
  - Docker Engine/Compose
  - Repo `.env` configured (`OD_ADMIN_TOKEN`, `ELASTIC_PASSWORD`, ingest secret, etc.)
- Core services covered by integration:
  - Proxy/decision: `squid`, `icap-adaptor`, `policy-engine`
  - Data: `postgres`, `redis`, `elasticsearch`, `kibana`
  - Async workers: `llm-worker`, `reclass-worker`, `page-fetcher`, `crawl4ai`
  - Management/ops: `admin-api`, `web-admin`, `event-ingester`, `filebeat`, `prometheus`, `odctl-runner`
- Default AI provider in compose: `mock-openai` (deterministic offline smoke behavior)

## Primary automation script

- Script: `tests/integration.sh`
- Ordered phases:
  1. Start stack (with optional build)
  2. `odctl smoke --profile compose`
  3. `tests/stage06_ingest.sh` (ingest → Elasticsearch → reporting)
  4. `tests/page-fetch-flow.sh` (event → page fetch → Admin API/CLI)
  5. `tests/content-pending-smoke.sh` (ContentPending → Crawl4AI → LLM verdict)
  6. Health checks (`admin-api`, `policy-engine`, `event-ingester`)
  7. Tear down stack (`docker compose --env-file .env -f deploy/docker/docker-compose.yml down`)

## Build reliability controls

`tests/integration.sh` includes retry and cache-recovery controls for flaky Docker metadata/build states:

- `INTEGRATION_BUILD` (default `1`)
  - `1`: run `docker compose build` before tests
  - `0`: skip rebuild, run `docker compose up -d` with existing images
- `INTEGRATION_BUILD_RETRIES` (default `3`): max build attempts when build fails
- `INTEGRATION_PRUNE_ON_RETRY` (default `1`): run `docker builder prune -f` between failed attempts
- `INTEGRATION_RETRY_DELAY_SECONDS` (default `5`): sleep duration between retries

Recommended runs:

- Fast local recheck (reuse images):
  - `INTEGRATION_BUILD=0 tests/integration.sh`
- CI/full rebuild with retries:
  - `INTEGRATION_BUILD=1 INTEGRATION_BUILD_RETRIES=3 tests/integration.sh`

## Failure triage checklist

If integration fails:

1. Save script output and compose logs:
   - `docker compose --env-file .env -f deploy/docker/docker-compose.yml logs --tail=200`
2. Confirm host and Docker space:
   - `df -h`
   - `docker system df`
3. If build fails with BuildKit metadata/path errors, rerun with retries enabled (defaults already enabled).
4. If needed, run a no-build validation to isolate runtime vs build failures:
   - `INTEGRATION_BUILD=0 tests/integration.sh`
5. Re-run individual stages to isolate failing phase:
   - `odctl smoke --profile compose`
   - `tests/stage06_ingest.sh`
   - `tests/page-fetch-flow.sh`
   - `tests/content-pending-smoke.sh`

## Evidence capture

- Archive integration logs in release evidence (for example `docs/evidence/stage07-integration.log`).
- Preserve `tests/artifacts/content-pending/` outputs for content-first flow verification when investigating failures.
- Capture Kibana/Prometheus screenshots only when release policy requires manual signoff.
