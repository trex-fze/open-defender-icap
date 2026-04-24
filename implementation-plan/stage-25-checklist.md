# Stage 25 Checklist - Prompt Injection Hardening

## Inventory and Design
- [ ] Finalize Stage 25 threat model and attack class matrix.
- [ ] Confirm strict visible-only extraction as mandatory crawl behavior.
- [ ] Confirm guardrail policy: high suspicion forces `Review`.
- [ ] Confirm rollout toggles and defaults in decisions log.

## Crawl Boundary Hardening
- [ ] Add parser-backed HTML sanitization for visible-only extraction in Crawl4AI service.
- [ ] Remove hidden/non-visible/non-content nodes before text extraction.
- [ ] Ensure extractor output remains bounded by configured max text limits.
- [ ] Add extraction telemetry fields (`nodes_removed`, suspicious marker counts).
- [ ] Add/refresh unit coverage for hidden-div and CSS-hidden payload cases.

## Classification Guardrails
- [ ] Implement llm-worker prompt-injection marker detection + weighted score.
- [ ] Add guardrail threshold evaluation and structured reason labels.
- [ ] Force `Review` when threshold is reached.
- [ ] Apply confidence cap when guardrail is applied.
- [ ] Persist detection and guardrail metadata in classification flags.
- [ ] Add Prometheus counters for detection and forced-review outcomes.

## Runtime Authority Hardening
- [ ] Remove direct LLM-derived enforcement cache writes.
- [ ] Keep cache invalidation signaling for re-evaluation flow.
- [ ] Verify ICAP behavior remains stable on cache misses and pending transitions.
- [ ] Verify policy-engine authoritative action path remains deterministic.

## Test and Verification
- [ ] Replace prompt-injection smoke vector to use `content_excerpt` payloads.
- [ ] Add canonical-valid coercion payload case and assert forced `Review`.
- [ ] Add integration tests for strict extraction + guardrail + authority shift.
- [ ] Run targeted worker/service test suites.
- [ ] Run Stage 25 security smoke and collect artifacts.

## Docs and Runbooks
- [ ] Update `docs/testing/security-plan.md` with implemented controls and commands.
- [ ] Update `README.md` safety/FAQ entries for Stage 25 behavior.
- [ ] Remove or replace stale references to unimplemented `prompt_filter` semantics.
- [ ] Publish `implementation-plan/stage-25-verification.md` with evidence links.

## Release Gate
- [ ] Stage 25 acceptance criteria all complete.
- [ ] Stage roadmap updated and Stage 25 status documented.
- [ ] Follow-up items (if any) captured for next stage.
