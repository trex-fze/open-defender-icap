# Stage 9 Implementation Plan – Content-Aware URL Classification

**Status:** ✅ Completed

## Objectives

- Capture and store sanitized page content for URLs that flow through the ingestion pipeline.
- Enrich classification jobs with the captured content so LLM providers make decisions on real page data.
- Provide observability, controls, and evidence for the new workflow.

## Work Breakdown

| Task ID | Description | Owner | Dependencies | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| S9-T1 | Design DB schema + migrations for `page_contents` | Backend | None | ✅ | `services/admin-api/migrations/0005_page_contents.sql` plus fallback DDL in worker tests; `page_contents` drives Admin API + odctl endpoints. |
| S9-T2 | Extend event-ingester with page fetch scheduler + policies | Backend | S9-T1 | ✅ | `event-ingester` publishes `PageFetchJob` to Redis once per eligible ingest event and exports `page_fetch_*` metrics/config. |
| S9-T3 | Integrate Crawl4AI service + `workers/page-fetcher` orchestrator | Backend | S9-T1/S9-T2 | ✅ | `services/crawl4ai-service` FastAPI wrapper + `workers/page-fetcher` crate orchestrate Crawl4AI calls, storage, Prometheus metrics. |
| S9-T4 | Provide fallback lightweight fetcher (optional) | Backend | S9-T3 | ✅ (flagged) | Feature flag documented; default path uses Crawl4AI, fallback stub remains off but codepath prepared. |
| S9-T5 | Update `ClassificationJobPayload` + LLM worker prompt to include `content_excerpt` | Backend | S9-T3 | ✅ | `workers/llm-worker` now hydrates excerpt/hash/version, enriches prompts, persists `content_version`, includes unit tests. |
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
| LLM prompt overflow due to large excerpts | Truncate to configurable length (e.g., 1500 chars) and include hash/context only |

## Testing Strategy

- Unit tests for scheduler policy decisions, payload serialization, and LLM prompt builder.
- Integration tests spinning up Crawl4AI container (or mocked API) alongside page-fetcher to verify storage + completion events.
- Update Stage 6 ingest smoke to assert `content_excerpt` is present for allowed domains.
- Extend Stage 8/9 security perf scripts to include cases where content fetch fails (timeout) and ensure fallback reason is recorded.

Implementation status: `cargo test -p reclass-worker`, `cargo test -p llm-worker`, and `cargo test -p admin-api` cover unit/integration paths; docker-compose runner executes `tests/stage06_ingest.sh` plus the new `tests/page-fetch-flow.sh` to validate the full ingestion → Crawl4AI → LLM prompt loop.

## Deliverables

- RFC + plan (this document) committed.
- New worker crate + migrations.
- Documentation updates across README, integration plan, testing docs, and evidence checklist.

## Evidence & Verification

- **Compose smoke:** `tests/integration.sh` now runs the Stage 6 ingest script followed by `tests/page-fetch-flow.sh`, asserting that page fetch jobs are created, content lands in `page_contents`, and `odctl page show/history` expose excerpts + metadata.
- **CLI/admin tooling:** Admin API routes `/api/v1/page-contents/:normalized_key` + `/history` (see `services/admin-api/src/page_contents.rs`) and the `odctl page` subcommands offer operator evidence with JSON output for runbooks.
- **Metrics:** Page-fetcher and event-ingester export `page_fetch_latency_seconds`, `page_fetch_failures_total{reason}`, `crawl4ai_requests_total`, etc.; scraping is handled by the existing Prometheus job in the docker stack. Use `/metrics` on each worker to verify counters increase during `tests/page-fetch-flow.sh`.
- **Prompt inspection:** `workers/llm-worker/src/main.rs` logs include the excerpt section, and classification rows now record `content_version`/`content_hash` ensuring analysts can cross-reference stored page content.
- **Configuration knobs:** `.env.example`, `config/page-fetcher.json`, and compose env vars (`OD_PAGE_FETCH_*`) document how to enable/disable content fetch, TTL, and Crawl4AI endpoints.
