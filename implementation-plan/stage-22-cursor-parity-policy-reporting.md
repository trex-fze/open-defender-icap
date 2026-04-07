# Stage 22 Implementation Plan - Cursor Parity for Policy and Reporting APIs

**Status**: Planned  
**Primary Owners**: Backend + Frontend + DevTools + QA  
**Created**: 2026-04-07

## Objective
- Convert remaining policy/reporting list APIs that still use page/offset contracts to cursor/keyset parity where volume warrants.

## Scope
1. Endpoint inventory and prioritization.
2. Cursor contract migration (`limit` + `cursor`, `{data, meta}`).
3. Supporting index additions.
4. Web-admin and odctl parity migration.

## Work Breakdown
| Task ID | Description | Owner | Status | Notes |
| --- | --- | --- | --- | --- |
| S22-T1 | Inventory remaining page/offset policy/reporting routes | Backend | [ ] | Includes policy list/history and report-heavy reads. |
| S22-T2 | Add cursor pagination handlers + keyset queries | Backend | [ ] | Keep old params behind compatibility window if needed. |
| S22-T3 | Add DB indexes for keyset traversal | Backend | [ ] | Migration required per endpoint sort key. |
| S22-T4 | Migrate web-admin hooks/components to cursor contract | Frontend | [ ] | Update data layer and pagination UX. |
| S22-T5 | Migrate odctl list commands (`--limit`, `--cursor`) | DevTools | [ ] | Preserve readable table/json output. |
| S22-T6 | Add cursor-chain integration tests and smoke checks | QA | [ ] | Forward traversal, empty boundary, bad cursor handling. |

## Evidence
- Verification log: `implementation-plan/stage-22-verification.md`
