# Stage 14 RFC - Pending Hardening and Output-Invalid Fallback

**Parent Sections:** `docs/engine-adaptor-spec.md` §§20.3.2, 24  
**Status:** Implemented (2026-04)

## Motivation

Some domains remained indefinitely in `waiting_content` because crawl failures were not persisted as terminal states and local-model output-contract failures could terminate without classification. This stage hardens pending lifecycle so keys always converge to a classified terminal state.

## Goals

1. Persist terminal crawl failures (`fetch_status`, `fetch_reason`) in `page_contents`.
2. Ensure metadata-only threshold progression can use persisted terminal fetch evidence.
3. Route local output-invalid errors through online metadata-only verification.
4. Terminalize unresolved output-invalid paths to `unknown-unclassified / insufficient-evidence`.
5. Prevent metadata-only threshold outcomes from re-entering infinite pending loops.

## Non-Goals

- Replacing primary local-first routing policy.
- Enforcing tenant-domain exception logic.
- Full stream consumer-group migration (tracked separately).

## Behavior Contract

1. Page fetcher persists both success and failure outcomes.
2. LLM worker reads terminal fetch history to trigger metadata-only progression when threshold is met.
3. Local invalid JSON/schema output triggers online metadata-only verification attempt.
4. If online verification is unavailable/fails, worker stores `unknown-unclassified / insufficient-evidence` and clears pending.
5. `metadata_only_requeue_for_content=false` default prevents threshold-derived metadata outcomes from requeue loops.

## Acceptance Criteria

1. Repeated crawl failures do not keep keys in `waiting_content` indefinitely.
2. Local output-invalid keys either classify via online verification or terminalize to insufficient evidence.
3. Pending rows are cleared on terminal fallback paths.
4. Metrics expose output-invalid/fallback/terminalization counts for operator monitoring.
