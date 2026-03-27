# Stage 9 RFC - Content-Aware URL Classification

**Parent Sections:** `docs/engine-adaptor-spec.md` §§35–38 (new)  
**Status:** Implemented and hardened (2026-03)

## Motivation

Stage 8 hybrid routing improved provider resilience but classification quality still depended on metadata-only prompts in some paths. The platform now enforces content-first decisions for `requires_content` jobs so verdicts are grounded in real homepage evidence before access is granted.

## Goals

1. Fetch homepage content through Crawl4AI and persist deterministic HTML context for classification (`[HEAD]`, `[TITLE]`, `[BODY]`).
2. Ensure LLM prompts include canonical taxonomy IDs, normalized domain key, and homepage HTML context.
3. Enforce strict Crawl4AI-only fetching for content-aware classification (no direct HTTP fallback).
4. Add retry handling when providers return non-canonical taxonomy labels.
5. Keep strong observability for pending states, retries, and content hash/version traceability.

## Non-Goals

- Media/file downloads (PDFs, binaries).
- Full site crawling/depth traversal beyond the configured homepage fetch target.
- Replacing Stage 8 provider failover design.

## Architecture Overview

### Data Flow (Current)

1. **ICAP/Policy path** enqueues both `classification-jobs` and `page-fetch-jobs` for uncached keys requiring content verification.
2. **Page Fetcher Worker** calls Crawl4AI `/crawl`, validates the response, extracts `<head>`, `<title>`, and `<body>`, formats them into `[HEAD]...[/HEAD]`, `[TITLE]...[/TITLE]`, `[BODY]...[/BODY]`, and stores the result in `page_contents` with `content_hash` and `content_version`.
3. **Strict mode**: if Crawl4AI fails, the worker records failure state and retries; it does not perform direct HTTP fallback.
4. **LLM Worker gating**: `requires_content=true` jobs wait/requeue until fresh `page_contents` exist.
5. **LLM prompt contract** includes canonical taxonomy IDs, normalized key/domain, and the stored HTML context. Responses must map to canonical taxonomy; non-canonical outputs are logged and retried.
6. **Persistence + cache**: canonical verdicts are persisted and pushed to Redis cache/invalidation channels; pending records are cleared.

### Components

- **Crawl4AI microservice (`services/crawl4ai-service`)**
  - Headless Chromium execution path for homepage fetch.
  - Default browser user-agent fallback to prevent runtime failures when caller omits UA.
  - Structured error logging for crawl failures.

- **Page Fetcher Worker (`workers/page-fetcher`)**
  - Reads `page-fetch-jobs`, calls Crawl4AI, stores HTML context payloads in `page_contents`.
  - Enforces `max_html_context_chars` cap.
  - Emits fetch success/failure observability with normalized key context.

- **LLM Worker (`workers/llm-worker`)**
  - Reads classification jobs, gates on `requires_content` availability.
  - Builds canonical prompt with taxonomy IDs + HTML context.
  - Retries non-canonical provider output before fallback handling.
  - Requeues classification/page-fetch jobs when transient processing errors occur.

## Security and Privacy

- Classification decisions for gated jobs are content-backed before release from pending.
- Crawl content persisted is bounded and traceable via hash/version.
- No alternate fetch path bypasses Crawl4AI in strict mode.

## Acceptance Criteria

1. `requires_content` jobs do not produce metadata-only verdicts.
2. LLM prompts for content-aware jobs include canonical taxonomy IDs, normalized key/domain, and `[HEAD]/[TITLE]/[BODY]` context.
3. Non-canonical LLM outputs are retried and never persisted as-is.
4. Crawl failures keep keys pending until Crawl4AI succeeds (or operator override), with no direct HTTP fallback path.
5. Smoke evidence confirms pending -> Crawl4AI -> canonical classification -> enforcement progression.
