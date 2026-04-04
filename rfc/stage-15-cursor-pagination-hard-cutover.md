# Stage 15 RFC - Cursor Pagination Hard Cutover

**Parent Sections:** `docs/engine-adaptor-spec.md` §§20.1-20.5, 20.7  
**Status:** Implemented (2026-04)

## Motivation

High-volume list endpoints were using mixed response shapes (array, limit-only, and page/page_size), which creates scaling risk and client inconsistency. This stage introduces a single cursor-based contract for heavy-read admin surfaces so latency stays stable as datasets grow and clients share one traversal model.

## Goals

1. Standardize targeted list APIs on cursor pagination using `limit` + `cursor`.
2. Standardize responses to `{ data, meta }` with `has_more` and `next_cursor`.
3. Use deterministic keyset ordering with tie-breakers to avoid page drift.
4. Cut over backend, web-admin, and odctl in one delivery to avoid dual contracts.
5. Add DB indexes aligned with seek predicates and sort order.

## Non-Goals

- Converting legacy policy/reporting page-based routes in the same stage.
- Adding exact total-count semantics to every list response.
- Introducing `/api/v2` versioning for this change set.

## Behavior Contract

1. Converted list routes accept `limit` (default 50, clamp 1..200) and optional opaque `cursor`.
2. Converted list routes return:
   - `data`: row array
   - `meta.limit`: effective page size
   - `meta.has_more`: whether another page exists
   - `meta.next_cursor`: opaque token for the next page when `has_more=true`
   - `meta.prev_cursor`: reserved (currently `null`)
3. Invalid cursor values return `400` (`INVALID_CURSOR` where endpoint uses `ApiError`; generic `400` for status-only handlers).
4. All converted routes use deterministic ordering with a stable tie-breaker (e.g., `(updated_at, normalized_key)` or `(created_at, id)`).

## Endpoints Covered

- `GET /api/v1/classifications`
- `GET /api/v1/classifications/pending`
- `GET /api/v1/overrides`
- `GET /api/v1/cli-logs`
- `GET /api/v1/iam/users`
- `GET /api/v1/iam/groups`
- `GET /api/v1/iam/service-accounts`
- `GET /api/v1/iam/audit`

## Acceptance Criteria

1. Converted endpoints no longer return bare arrays.
2. Web-admin tables for converted endpoints use cursor traversal and bounded row windows.
3. odctl list commands support `--limit` and `--cursor` for converted routes.
4. Query plans use supporting indexes for keyset pagination paths.
5. Runtime smoke confirms first-page and next-cursor traversal on live containers.
