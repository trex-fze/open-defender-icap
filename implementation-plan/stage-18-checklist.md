# Stage 18 Checklist - Facebook E2E Reliability Hardening

## Harness
- [x] Add multi-run wrapper for facebook e2e smoke.
- [x] Capture run duration + pass/fail in summary output.
- [x] Support configurable run count and output directory.

## Diagnostics
- [ ] Ensure each failed run links to stage-level artifacts.
- [ ] Add optional auto-collection of cross-service diagnostics on failure.
- [ ] Verify no stale artifact contamination between runs.

## Reliability Gate
- [ ] Run baseline matrix (`RUNS=10`) and record pass rate.
- [ ] Apply hardening and re-run gate (`RUNS=10`).
- [ ] Confirm gate >= 90% pass.

## Documentation
- [ ] Add command examples to runbook and testing docs.
- [ ] Add verification summary with failure signature breakdown.

## Completion
- [ ] Mark Stage 18 complete in `implementation-plan/stage-plan.md`.
