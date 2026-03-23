# Stage 9 Implementation Plan – Content-Aware URL Classification

**Status:** Draft

## Objectives

- Capture and store sanitized page content for URLs that flow through the ingestion pipeline.
- Enrich classification jobs with the captured content so LLM providers make decisions on real page data.
- Provide observability, controls, and evidence for the new workflow.

## Work Breakdown

| Task ID | Description | Owner | Dependencies | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| S9-T1 | Design DB schema + migrations for `page_contents` | Backend | None | ⬜ | Add table + indexes, integrate with admin-api migrations |
| S9-T2 | Extend event-ingester with page fetch scheduler + policies | Backend | S9-T1 | ⬜ | Emit `page-fetch-jobs` Redis stream, TTL logic, enforce crawl policies |
| S9-T3 | Integrate Crawl4AI service + `workers/page-fetcher` orchestrator | Backend | S9-T1/S9-T2 | ⬜ | Package Crawl4AI Docker image, add compose service, implement worker that calls `/crawl`, handles retries/metrics, stores results |
| S9-T4 | Provide fallback lightweight fetcher (optional) | Backend | S9-T3 | ⬜ | Feature-flag simple HTTP GET if Crawl4AI unreachable |
| S9-T5 | Update `ClassificationJobPayload` + LLM worker prompt to include `content_excerpt` | Backend | S9-T3 | ⬜ | Backward compatible payload changes, new prompt template referencing Crawl4AI metadata |
| S9-T6 | CLI/Admin endpoints for content inspection | CLI + Admin | S9-T1 | ⬜ | `odctl pages fetch-status`, admin-api route to retrieve excerpt + crawl metadata |
| S9-T7 | Observability & docs (Prometheus, runbooks, configs) | SRE/Docs | S9-T2–S9-T5 | ⬜ | Metrics panels (crawl4ai latency/failures), README/integration updates, evidence checklist |
| S9-T8 | Testing & evidence (unit, integration, Stage 6/8 smoke updates) | QA | S9-T2–S9-T6 | ⬜ | Spin up Crawl4AI in CI, add fixtures covering success/failure paths, extend smoketests to assert excerpt presence |

## Milestones

1. **M1 – Storage & Scheduler Ready** (S9-T1/T2)
2. **M2 – Page Fetcher + LLM Prompt Integration** (S9-T3/T4)
3. **M3 – Tooling, Observability, and Evidence** (S9-T6–S9-T8)

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

## Deliverables

- RFC + plan (this document) committed.
- New worker crate + migrations.
- Documentation updates across README, integration plan, testing docs, and evidence checklist.
