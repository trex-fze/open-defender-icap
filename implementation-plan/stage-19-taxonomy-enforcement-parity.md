# Stage 19 Implementation Plan - Taxonomy Enforcement Parity

**Status**: Planned  
**Primary Owners**: Classification + Policy + Backend + QA  
**Created**: 2026-04-07

## Objective
- Guarantee canonical taxonomy behavior is consistent across worker, policy-engine, and admin-api persistence paths.

## Scope
1. Alias/canonical mapping parity across services.
2. Activation parity (category + subcategory) in all decision paths.
3. Persistence invariants for canonical IDs and fallback metadata.
4. Cross-service contract tests.

## Work Breakdown
| Task ID | Description | Owner | Status | Notes |
| --- | --- | --- | --- | --- |
| S19-T1 | Build parity matrix for alias/canonical/fallback scenarios | QA + Policy | [ ] | Single source test matrix shared by all services. |
| S19-T2 | Add cross-service tests for canonicalization outcomes | Backend + Classification | [ ] | llm-worker/reclass-worker/policy-engine/admin-api parity assertions. |
| S19-T3 | Enforce persistence invariants for category/subcategory IDs | Backend | [ ] | Reject non-canonical writes; verify import/manual flows. |
| S19-T4 | Verify activation-state parity for category+subcategory disable states | Policy + Classification | [ ] | Decision action must converge across paths. |
| S19-T5 | Document taxonomy parity operational checks | SRE + Docs | [ ] | Runbook + verification report updates. |

## Evidence
- Tests: service suites + parity fixtures
- Verification: `implementation-plan/stage-19-verification.md`
