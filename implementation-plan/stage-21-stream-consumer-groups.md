# Stage 21 Implementation Plan - Stream Consumer-Group Migration

**Status**: Planned  
**Primary Owners**: Backend + Classification + SRE  
**Created**: 2026-04-07

## Objective
- Migrate stream consumers from last-id `XREAD` loops to `XREADGROUP` with ACK/claim semantics for restart-safe backlog handling.

## Scope
1. Consumer-group bootstrap and configuration.
2. Read/ack loop migration.
3. Pending/claim handling for stalled deliveries.
4. Poison-message policy and dead-letter handling.

## Work Breakdown
| Task ID | Description | Owner | Status | Notes |
| --- | --- | --- | --- | --- |
| S21-T1 | Inventory all Redis stream consumers and current semantics | Backend | [ ] | classification-jobs/page-fetch-jobs and related flows. |
| S21-T2 | Add consumer-group bootstrap (idempotent `XGROUP CREATE`) | Backend | [ ] | Startup-safe even if group exists. |
| S21-T3 | Replace read loop with `XREADGROUP` + ACK on success | Backend | [ ] | Preserve current batching/blocking behavior. |
| S21-T4 | Add claim/recovery for stale pending deliveries | Backend + SRE | [ ] | `XPENDING` + `XAUTOCLAIM`/`XCLAIM` policy. |
| S21-T5 | Add poison message policy (max deliveries + DLQ/terminal) | Backend | [ ] | Avoid stream stalls from malformed jobs. |
| S21-T6 | Add restart simulation integration tests | QA | [ ] | Prove no loss across restart windows. |

## Evidence
- Integration artifacts: `tests/artifacts/stream-cg-migration/*`
- Verification log: `implementation-plan/stage-21-verification.md`
