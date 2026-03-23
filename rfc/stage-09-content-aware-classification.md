# Stage 9 RFC – Content-Aware URL Classification

**Parent Sections:** `docs/engine-adaptor-spec.md` §§35–38 (new)

## Motivation

Stage 8 introduced hybrid provider routing but still classifies requests using only URL metadata (hostname, normalized key, trace id). That limits accuracy for categories that depend on actual page content (malware payloads, phishing pages, hate speech, etc.). Operators want the LLM worker to reason about the rendered text/HTML and to retain evidence for playbooks and escalations.

## Goals

1. Fetch and persist sanitized page content (HTML + extracted text) for every Stage 6 ingest event that requires LLM review.
2. Enrich the classification job payload with the captured content or a deterministic summary so the LLM can base decisions on the real page.
3. Provide observability, storage, and retention knobs so large content blobs do not overwhelm Redis/Postgres/Elasticsearch.
4. Maintain operator controls for disabling content fetches per policy (e.g., when URLs are internal or require auth).

## Non-Goals

- Full browser emulation or JavaScript execution (Phase 2 idea). Stage 9 relies on HTTP GET + basic parsing.
- Media/file downloads (PDFs, binaries). We only fetch text/HTML up to a configurable size limit (default 512 KB).
- Advanced ML summarization. We rely on deterministic truncation + optional boilerplate stripping.

## Architecture Overview

### Data Flow (Happy Path)

1. **Event Ingester** receives a Stage 6 sample, normalizes `normalized_key`, and writes both to Elasticsearch and Redis (`classification-jobs`).
2. New **Page Fetch Scheduler** component (in event-ingester) inspects the request and enqueues a job on `page-fetch-jobs` (Redis stream) with URL, headers, normalized key, and fetch policy metadata.
3. **Page Fetcher Worker** (new Rust binary) consumes `page-fetch-jobs`, performs HTTP GET with configurable timeouts, sanitizes the response (strip scripts/styles, collapse whitespace), and stores the result in Postgres table `page_contents` keyed by normalized key + version.
4. The fetcher publishes a `page-content-ready` event, or the scheduler writes the sanitized blob back into Redis as part of the classification payload (optionally referencing the Postgres row id to avoid large messages).
5. **LLM Worker** reads the enriched payload, injects the page text snippet (first N chars + summary metadata) into the prompt, and stores the classification + `content_version` for traceability.

### Components

- **Page Fetch Scheduler (event-ingester extension)**
  - Determines whether URL is eligible (scheme http/https, not on denylist, size unknown, policy allows fetch).
  - Deduplicates by normalized key; schedules refresh based on TTL (default 6 hours) to avoid repeated fetches.
  - Provides metrics (`page_fetch_jobs_total`, `page_fetch_skipped_total{reason}`).

- **Page Fetcher Worker (`workers/page-fetcher`)**
  - Async Tokio service using reqwest with configurable concurrency.
  - Normalizes text via `lol_html`/`html5ever` or `scraper` crate; stores both raw HTML (compressed) and extracted text snippet (UTF-8) in Postgres.
  - Emits Prometheus metrics (latency, status codes, bytes, failures) and publishes completion to Redis.
  - Handles max redirects, TLS errors, robots.txt respect (configurable).

- **Storage Schema**
  - New table `page_contents` (`id UUID`, `normalized_key`, `fetch_version`, `content_type`, `content_hash`, `raw_bytes BYTEA`, `text_excerpt TEXT`, `fetched_at`, `ttl_seconds`).
  - Indexed by normalized key + fetch_version; LLM worker queries the latest row within TTL.

- **Classification Payload Changes**
  - Extend `ClassificationJobPayload` with optional `content_excerpt`, `content_hash`, `content_version`, `content_language`.
  - LLM worker updates `build_prompt` to include e.g.:
    ```
    Page Excerpt (first 1200 chars):
    "..."
    Content Hash: sha256-...
    ```
  - Provide fallbacks when content unavailable (still classify based on URL metadata).

### Security & Privacy

- Respect allow/deny lists (no intranet fetches by default). Add config `CONTENT_FETCH_ALLOW_PRIVATE=false`.
- Cap size + sanitize (strip scripts/styles) before storing to mitigate XSS when viewing via admin UI.
- Record fetch headers + IPs for auditing but keep secrets in `.env`.
- Optional AES-GCM encryption at rest for `raw_bytes` (future enhancement, tracked as open question).

### Observability

- Prometheus metrics: `page_fetch_latency_seconds`, `page_fetch_failures_total{reason}`, `page_content_cached_total`.
- New Kibana dashboard panel referencing `page_contents` table (via admin-api endpoint) for analysts.
- CLI commands: `odctl pages fetch-status --key <normalized_key>`.

### Rollout Strategy

1. Migrate database (admin-api migration) to add `page_contents` table.
2. Deploy page-fetcher behind feature flag `OD_ENABLE_PAGE_FETCHER=false`.
3. When enabled, start with low concurrency (e.g., 2 workers) and small TTL to validate.
4. Update Stage 6 smoke/perf scripts to assert content presence.

## Open Questions

1. Should we store full HTML or only sanitized text to control storage cost?
2. Do we need to obey robots.txt strictly, or allow override per customer contract?
3. How to handle pages requiring auth (cookies, javascript)? Out of scope for Stage 9.
4. What retention period is acceptable for captured content (default 30 days)?

## Acceptance Criteria

1. Every classification job includes either a content excerpt or a reason why content is missing (timeout, blocked domain, etc.).
2. LLM worker prompt logs demonstrate the added excerpt, and Stage 8/9 smoke tests assert improved accuracy.
3. Page fetcher metrics/alerts exist and integrate with Prometheus.
4. Admin/CLI endpoints allow an operator to view content fetch status per normalized key.
5. Documentation covers configuration, privacy controls, and runbooks for the new workflow.
