# Stage 14 Implementation Plan - Pending Hardening and Output-Invalid Fallback

## Objective

Eliminate infinite pending loops by persisting terminal crawl failures, enabling deterministic no-content progression, and adding online verification fallback for local output-invalid failures.

## Completed Work

- [x] Persist page-fetch terminal failures in `page_contents` with normalized `fetch_status` and `fetch_reason`.
- [x] Restrict content freshness checks to `fetch_status='ok'` entries.
- [x] Use metadata-only threshold path without requeue loop (`metadata_only_requeue_for_content=false`).
- [x] Add output-invalid-aware failover routing in LLM worker.
- [x] Attempt online metadata-only verification when local output fails JSON/schema contract.
- [x] Terminalize unresolved output-invalid cases to `unknown-unclassified / insufficient-evidence`.
- [x] Add metrics for primary output-invalid detection, online verification outcomes, and terminal insufficient-evidence classifications.

## Verification Checklist

- [x] `cargo test -p page-fetcher --no-run`
- [x] `cargo test -p llm-worker --no-run`
- [x] `cargo test -p llm-worker`
- [x] `cargo test -p page-fetcher`
- [x] Runtime smoke: pending rows reduced and keys no longer loop indefinitely in `waiting_content` under repeated fetch/output failures.

## Follow-Up Recommendations

- [ ] Migrate streams to consumer groups (`XREADGROUP` + ACK) for restart-safe backlog handling.
- [ ] Add pending-age alerts keyed by `fetch_reason`/terminalization path.
- [ ] Add operator UI chips for fallback provenance flags in Classifications table.
