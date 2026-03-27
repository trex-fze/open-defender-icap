# Stage 9 Implementation Plan – Content-Aware URL Classification

**Status:** ✅ Completed

## Objectives

- Capture and store deterministic homepage HTML context for URLs that flow through the ingestion pipeline.
- Enrich classification jobs with canonical taxonomy + homepage HTML context so LLM providers make decisions on real page data.
- Provide observability, controls, and evidence for the new workflow.

## Work Breakdown

| Task ID | Description | Owner | Dependencies | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| S9-T1 | Design DB schema + migrations for `page_contents` | Backend | None | ✅ | `services/admin-api/migrations/0005_page_contents.sql` plus fallback DDL in worker tests; `page_contents` drives Admin API + odctl endpoints. |
| S9-T2 | Extend event-ingester with page fetch scheduler + policies | Backend | S9-T1 | ✅ | `event-ingester` publishes `PageFetchJob` to Redis once per eligible ingest event and exports `page_fetch_*` metrics/config. |
| S9-T3 | Integrate Crawl4AI service + `workers/page-fetcher` orchestrator | Backend | S9-T1/S9-T2 | ✅ | `services/crawl4ai-service` FastAPI wrapper + `workers/page-fetcher` crate orchestrate Crawl4AI calls, storage, Prometheus metrics. |
| S9-T4 | Enforce strict Crawl4AI-only fetch path | Backend | S9-T3 | ✅ | Removed direct HTTP fallback path; failures remain pending/retry until Crawl4AI recovery. |
| S9-T5 | Update LLM prompt contract for canonical IDs + HTML context | Backend | S9-T3 | ✅ | `workers/llm-worker` injects canonical taxonomy IDs, normalized key/domain, and `[HEAD]/[TITLE]/[BODY]` context; non-canonical responses are retried. |
| S9-T6 | CLI/Admin endpoints for content inspection | CLI + Admin | S9-T1 | ✅ | Added `/api/v1/page-contents/:key[/history]` plus `odctl page show/history` commands for operators. |
| S9-T7 | Observability & docs (Prometheus, runbooks, configs) | SRE/Docs | S9-T2–S9-T5 | ✅ | Compose/env docs expose `OD_PAGE_FETCH_*`, metrics exported (`page_fetch_latency_seconds`, etc.), Crawl4AI health surfaces. |
| S9-T8 | Testing & evidence (unit, integration, Stage 6/8 smoke updates) | QA | S9-T2–S9-T6 | ✅ | `tests/page-fetch-flow.sh` + integration wiring, reclass/llm-worker integration tests, stage smoke scripts updated. |

## Milestones

1. **M1 – Storage & Scheduler Ready** (S9-T1/T2) ✅
2. **M2 – Page Fetcher + LLM Prompt Integration** (S9-T3/T4) ✅
3. **M3 – Tooling, Observability, and Evidence** (S9-T6–S9-T8) ✅

## Risks & Mitigations

| Risk | Mitigation |
| --- | --- |
| Excessive storage usage from raw HTML | Compress raw bytes, configurable TTL + max size, allow text-only mode |
| Fetching sensitive/internal URLs | Denylist private IP ranges by default; allow explicit overrides |
| Slow or malicious pages causing hangs | Enforce Crawl4AI timeouts, concurrency limits, and total bytes caps |
| Crawl4AI image/framework drift | Pin version, provide healthcheck + CLI to verify compatibility |
| LLM prompt overflow due to large HTML context | Cap persisted context (`max_html_context_chars`) and include content hash/version for traceability |

## Testing Strategy

- Unit tests for scheduler policy decisions, payload serialization, and LLM prompt builder.
- Integration tests spinning up Crawl4AI container (or mocked API) alongside page-fetcher to verify storage + completion events.
- Update Stage 6 ingest/content smokes to assert stored `[HEAD]/[TITLE]/[BODY]` context is present for allowed domains.
- Extend Stage 8/9 security/perf scripts to include Crawl4AI failure recovery and pending-state persistence checks.

Implementation status: `cargo test -p reclass-worker`, `cargo test -p llm-worker`, and `cargo test -p admin-api` cover unit/integration paths; docker-compose runner executes `tests/stage06_ingest.sh` plus the new `tests/page-fetch-flow.sh` to validate the full ingestion → Crawl4AI → LLM prompt loop.

## Deliverables

- RFC + plan (this document) committed.
- New worker crate + migrations.
- Documentation updates across README, integration plan, testing docs, and evidence checklist.

## Evidence & Verification

- **Compose smoke:** `tests/integration.sh` runs Stage 6 ingest and page-fetch checks, asserting page fetch jobs are created, content lands in `page_contents`, and `odctl page show/history` expose HTML context + metadata.
- **CLI/admin tooling:** Admin API routes `/api/v1/page-contents/:normalized_key` + `/history` (see `services/admin-api/src/page_contents.rs`) and the `odctl page` subcommands offer operator evidence with JSON output for runbooks.
- **Metrics:** Page-fetcher and event-ingester export `page_fetch_latency_seconds`, `page_fetch_failures_total{reason}`, `crawl4ai_requests_total`, etc.; scraping is handled by the existing Prometheus job in the docker stack. Use `/metrics` on each worker to verify counters increase during `tests/page-fetch-flow.sh`.
- **Prompt inspection:** `workers/llm-worker/src/main.rs` logs include `html_context_present` plus head/title/body character counts, and classification rows record `content_version`/`content_hash` for cross-reference.
- **Configuration knobs:** `.env.example`, `config/page-fetcher.json`, and compose env vars (`OD_PAGE_FETCH_*`) document how to enable/disable content fetch, TTL, and Crawl4AI endpoints.

## Recent Hardening (2026-03)

- Strict mode is now the default: no non-Crawl4AI fallback fetch path is available in `workers/page-fetcher` or `services/crawl4ai-service`.
- Crawl4AI UA handling is stabilized with a default browser user-agent to avoid `NoneType` crawl failures.
- `tests/security/facebook-e2e-smoke.sh` now verifies the full CONNECT flow from pending classification to final canonical verdict with richer stage artifacts.
