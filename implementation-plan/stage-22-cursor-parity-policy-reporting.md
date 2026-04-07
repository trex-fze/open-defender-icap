# Stage 22 Implementation Plan - Cursor Parity for Policy and Reporting APIs

**Status**: Complete  
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
| S22-T1 | Inventory remaining page/offset policy/reporting routes | Backend | [x] | Added audit baseline via `tests/cursor-parity-audit.sh`. |
| S22-T2 | Add cursor pagination handlers + keyset queries | Backend | [x] | Converted policy list endpoint to cursor contract with keyset query semantics. |
| S22-T3 | Add DB indexes for keyset traversal | Backend | [x] | Added migration `0022_policy_cursor_indexes.sql`. |
| S22-T4 | Migrate web-admin hooks/components to cursor contract | Frontend | [x] | Updated policies hook/page with cursor + pagination controls. |
| S22-T5 | Migrate odctl list commands (`--limit`, `--cursor`) | DevTools | [x] | Updated odctl policy list response decoding for cursor meta. |
| S22-T6 | Add cursor-chain integration tests and smoke checks | QA | [x] | Added/ran `tests/policy-cursor-smoke.sh` for forward cursor chaining. |

## Evidence
- Verification log: `implementation-plan/stage-22-verification.md`
