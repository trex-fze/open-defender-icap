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
| S9-T2 | Extend event-ingester with page fetch scheduler + policies | Backend | S9-T1 | ⬜ | Emit `page-fetch-jobs` Redis stream, TTL logic |
| S9-T3 | Implement `workers/page-fetcher` binary + metrics | Backend | S9-T1 | ⬜ | HTTP fetch, sanitization, Postgres writes, completion events |
| S9-T4 | Update `ClassificationJobPayload` + LLM worker prompt to include `content_excerpt` | Backend | S9-T1 | ⬜ | Backward compatible payload changes, new prompt template |
| S9-T5 | CLI/Admin endpoints for content inspection | CLI + Admin | S9-T1 | ⬜ | `odctl pages fetch-status`, admin-api route to retrieve excerpt |
| S9-T6 | Observability & docs (Prometheus, runbooks, configs) | SRE/Docs | S9-T2–S9-T4 | ⬜ | Metrics panels, README/integration updates, evidence checklist |
| S9-T7 | Testing & evidence (unit, integration, Stage 6/8 smoke updates) | QA | S9-T2–S9-T4 | ⬜ | Add fixtures for content fetch, extend tests to assert excerpt presence |

## Milestones

1. **M1 – Storage & Scheduler Ready** (S9-T1/T2)
2. **M2 – Page Fetcher + LLM Prompt Integration** (S9-T3/T4)
3. **M3 – Tooling, Observability, and Evidence** (S9-T5–S9-T7)

## Risks & Mitigations

| Risk | Mitigation |
| --- | --- |
| Excessive storage usage from raw HTML | Compress raw bytes, configurable TTL + max size, allow text-only mode |
| Fetching sensitive/internal URLs | Denylist private IP ranges by default; allow explicit overrides |
| Slow or malicious pages causing hangs | Enforce timeouts, concurrency limits, and total bytes caps |
| LLM prompt overflow due to large excerpts | Truncate to configurable length (e.g., 1500 chars) and include hash/context only |

## Testing Strategy

- Unit tests for scheduler policy decisions, HTML sanitization, and LLM prompt builder.
- Integration tests spinning up page-fetcher with mock HTTP server to verify storage + completion events.
- Update Stage 6 ingest smoke to assert `content_excerpt` is present for allowed domains.
- Extend Stage 8/9 security perf scripts to include cases where content fetch fails (timeout) and ensure fallback reason is recorded.

## Deliverables

- RFC + plan (this document) committed.
- New worker crate + migrations.
- Documentation updates across README, integration plan, testing docs, and evidence checklist.
