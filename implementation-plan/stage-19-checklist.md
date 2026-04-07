# Stage 19 Checklist - Taxonomy Enforcement Parity

## Parity Matrix
- [ ] Define canonical/alias/fallback test matrix.
- [ ] Include activation-disabled category and subcategory cases.

## Service Tests
- [ ] llm-worker canonicalization + persistence tests.
- [ ] reclass-worker canonicalization + persistence tests.
- [ ] policy-engine decision parity tests.
- [ ] admin-api manual/import persistence validation tests.

## Invariants
- [ ] Ensure persisted category/subcategory values are canonical IDs only.
- [ ] Ensure fallback reason metadata appears consistently when remapping occurs.

## Documentation
- [ ] Update runbook with parity validation commands.
- [ ] Record evidence in verification log.

## Completion
- [ ] Mark Stage 19 complete in `implementation-plan/stage-plan.md`.
