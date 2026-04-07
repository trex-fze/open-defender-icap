# Stage 16 Checklist - Policy Action Outcome Hardening

## Decision Closure
- [x] S16-DG1 closed: Review semantics finalized.
- [x] S16-DG2 closed: ContentPending policy-action support policy finalized.
- [x] S16-DG3 closed: Simulate mode contract finalized.

## Contract and Design
- [x] Publish action semantics matrix (Allow/Monitor/Review/Block/ContentPending).
- [x] Document precedence and decision source contract (`override`, `policy_rule`, `default`, `taxonomy_disabled`, etc.).
- [x] Define backward-compatibility behavior for legacy clients.

## Backend / Runtime Implementation
- [x] ICAP adaptor maps `Review` with distinct behavior and marker (not generic block).
- [x] ICAP adaptor guarantees pending + queue orchestration when action is explicit `ContentPending`.
- [x] `action_requires_follow_up` includes all required follow-up actions.
- [x] No enqueue dead-end when holding page is served.
- [x] Policy DSL / API validation rejects unknown condition keys.
- [x] Existing known condition keys remain supported (`domains`, `categories`, `users`, `groups`, `source_ips`, `risk_levels`).
- [x] Policy-engine activation enforcement checks subcategory where available.
- [x] Worker and policy-engine activation outcomes are aligned.

## Simulation Parity
- [x] Add `/api/v1/policies/simulate` mode contract (`runtime`, `policy_only`).
- [x] Default simulate mode is runtime parity.
- [x] CLI supports selecting simulate mode (if exposed).
- [x] Runtime parity tests pass for representative scenarios.

## Tests
- [x] Add/extend policy-engine unit tests for all relevant actions.
- [x] Add/extend icap-adaptor tests for response mapping and follow-up orchestration.
- [x] Add/extend admin-api tests for validation and effective action reporting.
- [x] Add/extend/introduce smoke test for explicit ContentPending policy-rule action.
- [x] Ensure regression tests for existing Allow/Block/Monitor paths remain green.

## Documentation
- [x] Update `docs/api-catalog.md` action semantics and simulate behavior.
- [x] Update `docs/user-guide.md` operator semantics and troubleshooting.
- [x] Update runbook with review/content-pending diagnostics and expected signals.
- [x] Update README action behavior summary if needed.

## Evidence and Outcomes
- [x] Capture command outputs/logs in `tests/artifacts/stage16/`.
- [x] Record before/after behavior matrix for each action.
- [x] Verify no unresolved dead-end pending flows in smoke results.
- [x] Verify classification list shows consistent recorded vs effective action outcomes.
- [x] Stage 16 marked complete in stage roadmap once all evidence is linked.

## Outcome Tracking Table
| Outcome ID | Outcome | Metric/Check | Evidence Path | Status |
| --- | --- | --- | --- | --- |
| S16-O1 | Distinct Review semantics | Response behavior + decision_source differs from Block | implementation-plan/stage-16-verification.md | [x] |
| S16-O2 | No ContentPending dead-ends | Holding page always accompanied by pending row + queue jobs | implementation-plan/stage-16-verification.md | [x] |
| S16-O3 | Strict condition validation | Unknown keys rejected with clear error | crates/policy-dsl/src/lib.rs (test) | [x] |
| S16-O4 | Activation parity | Engine and worker final action match same taxonomy state | services/policy-engine/src/evaluator.rs | [x] |
| S16-O5 | Simulate/runtime parity | `simulate(runtime)` equals `/api/v1/decision` for same input | services/policy-engine/src/main.rs + models tests | [x] |
