# Stage 16 Implementation Plan - Policy Action Outcome Hardening

**Status**: Complete  
**Primary Owners**: Platform Security + Policy Engine + Backend + SWG + Frontend + QA  
**Created**: 2026-04-07

## Objectives
- Make policy action semantics explicit and behaviorally correct for `Allow`, `Monitor`, `Review`, `Block`.
- Eliminate enforcement dead-ends for `ContentPending`.
- Remove silent policy misconfiguration paths.
- Align policy-engine and worker activation enforcement.
- Ensure `/simulate` can match runtime behavior for operator trust.

## Scope (Key Gaps)
1. `Review` currently behaves the same as `Block` at enforcement.
2. `ContentPending` policy action can produce holding-page responses without guaranteed pending/job orchestration.
3. Unknown policy condition keys are silently ignored (misconfiguration risk).
4. Taxonomy activation enforcement differs between policy-engine and worker path.
5. Policy simulation behavior can diverge from live decision path.

## Non-Goals
- No redesign of IAM role model.
- No broad re-architecture of classifier model contracts.
- No UI redesign outside policy outcome visibility and diagnostics needed for this stage.

## Decision Gates (must close before implementation)
| Gate ID | Decision | Default | Status | Notes |
| --- | --- | --- | --- | --- |
| S16-DG1 | `Review` semantics | Distinct review outcome (not generic block) | Closed | Implemented as blocked outcome with review-specific messaging + test coverage. |
| S16-DG2 | `ContentPending` as policy action | Supported with full orchestration | Closed | Explicit `ContentPending` now drives pending orchestration and follow-up enqueue paths. |
| S16-DG3 | `/simulate` mode model | `runtime` default + explicit `policy_only` | Closed | Added `mode` on simulate request/response with runtime default. |

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| S16-T1 | Define canonical action semantics matrix (`Allow/Monitor/Review/Block/ContentPending`) | Policy Eng + Security | S16-DG1, S16-DG2 | [x] | Captured in API catalog, user guide, README, and runbook updates. |
| S16-T2 | Implement distinct `Review` enforcement mapping in ICAP path + source tagging | SWG + Policy Eng | S16-T1 | [x] | `Review` now returns a review-specific blocked response body; unit tests added. |
| S16-T3 | Ensure explicit `ContentPending` action always triggers pending + queue orchestration | SWG + Backend | S16-T1 | [x] | Explicit action now sets pending flow + follow-up enqueue gate includes `ContentPending`. |
| S16-T4 | Harden policy schema validation to reject unknown condition fields | Policy Eng + Backend | S16-T1 | [x] | `Conditions` now uses `deny_unknown_fields`; smoke policy fixture updated to `users`. |
| S16-T5 | Align taxonomy activation checks (category + subcategory) across engine and workers | Policy Eng + Classification Eng | S16-T1 | [x] | Policy evaluator now checks category + subcategory in activation guard. |
| S16-T6 | Add runtime-parity simulation mode (`runtime` vs `policy_only`) | Policy Eng + DevTools | S16-DG3 | [x] | Added `SimulatePolicyRequest.mode`, runtime default, and mode echo in response. |
| S16-T7 | Expand tests (unit/integration/smoke) for all action outcomes and edge paths | QA + Service Owners | S16-T2..S16-T6 | [x] | Unit coverage added; policy-runtime smoke pass recorded; content-pending run captured with pending evidence. |
| S16-T8 | Update docs/runbooks/API catalog with final semantics and operator troubleshooting | Tech Writer + Service Owners | S16-T1..S16-T7 | [x] | Updated `docs/api-catalog.md`, `docs/user-guide.md`, `docs/runbooks/stage10-web-admin-operator-runbook.md`, `README.md`. |
| S16-T9 | Rollout and verify production-like compose stack behavior + evidence capture | SRE + QA | S16-T7, S16-T8 | [x] | Evidence stored under `tests/artifacts/stage16/` with verification summary. |

## Deliverables
- Code changes across policy-engine, icap-adaptor, admin-api, CLI, and selected web-admin surfaces.
- Updated API and operator docs for action semantics and diagnostics.
- Full evidence bundle under `tests/artifacts/stage16/`.

## Evidence Plan
- Unit tests:
  - `cargo test -p policy-engine`
  - `cargo test -p admin-api`
  - `cargo test -p icap-adaptor`
  - `cargo test -p odctl`
- Frontend:
  - `npm run build` (web-admin)
- Integration/smoke:
  - `tests/policy-runtime-smoke.sh`
  - `tests/content-pending-smoke.sh`
  - New/extended explicit `ContentPending` policy-action smoke.
- Artifacts:
  - `tests/artifacts/stage16/*.json|*.log`

## Completion Criteria
- `Review` is behaviorally distinct from `Block` (or explicitly deprecated and rejected at validation).
- `ContentPending` never dead-ends without pending/job orchestration.
- Unknown condition keys are rejected with actionable error messages.
- Taxonomy activation parity verified between policy-engine and worker paths.
- `/simulate` parity mode validated against `/api/v1/decision`.
- All stage checklist items are completed with evidence links.
