# Stage 25 Checklist - Prompt Injection Hardening

## Inventory and Design
- [x] Finalize Stage 25 threat model and attack class matrix.
- [x] Confirm strict visible-only extraction as mandatory crawl behavior.
- [x] Confirm guardrail policy: high suspicion forces `Review`.
- [x] Confirm rollout toggles and defaults in decisions log.

## Crawl Boundary Hardening
- [x] Add parser-backed HTML sanitization for visible-only extraction in Crawl4AI service.
- [x] Remove hidden/non-visible/non-content nodes before text extraction.
- [x] Ensure extractor output remains bounded by configured max text limits.
- [x] Add extraction telemetry fields (`nodes_removed`, suspicious marker counts).
- [x] Add/refresh unit coverage for hidden-div and CSS-hidden payload cases.

## Classification Guardrails
- [x] Implement llm-worker prompt-injection marker detection + weighted score.
- [x] Add guardrail threshold evaluation and structured reason labels.
- [x] Force `Review` when threshold is reached.
- [x] Apply confidence cap when guardrail is applied.
- [x] Persist detection and guardrail metadata in classification flags.
- [x] Add Prometheus counters for detection and forced-review outcomes.

## Runtime Authority Hardening
- [x] Remove direct LLM-derived enforcement cache writes.
- [x] Keep cache invalidation signaling for re-evaluation flow.
- [x] Verify ICAP behavior remains stable on cache misses and pending transitions.
- [x] Verify policy-engine authoritative action path remains deterministic.

## Test and Verification
- [x] Replace prompt-injection smoke vector to use `content_excerpt` payloads.
- [x] Add canonical-valid coercion payload case and assert forced `Review`.
- [x] Add integration tests for strict extraction + guardrail + authority shift.
- [x] Run targeted worker/service test suites.
- [ ] Run Stage 25 security smoke and collect artifacts.

## Docs and Runbooks
- [x] Update `docs/testing/security-plan.md` with implemented controls and commands.
- [x] Update `README.md` safety/FAQ entries for Stage 25 behavior.
- [x] Remove or replace stale references to unimplemented `prompt_filter` semantics.
- [x] Publish `implementation-plan/stage-25-verification.md` with evidence links.

## Release Gate
- [ ] Stage 25 acceptance criteria all complete.
- [x] Stage roadmap updated and Stage 25 status documented.
- [ ] Follow-up items (if any) captured for next stage.
