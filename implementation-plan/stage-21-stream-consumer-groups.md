# Stage 21 Implementation Plan - Stream Consumer-Group Migration

**Status**: Complete  
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
| S21-T1 | Inventory all Redis stream consumers and current semantics | Backend | [x] | Implemented discovery with `tests/streams-consumer-audit.sh`. |
| S21-T2 | Add consumer-group bootstrap (idempotent `XGROUP CREATE`) | Backend | [x] | Added in llm-worker and page-fetcher startup paths. |
| S21-T3 | Replace read loop with `XREADGROUP` + ACK on success | Backend | [x] | Migrated both workers from `XREAD` loops to group reads with ACK. |
| S21-T4 | Add claim/recovery for stale pending deliveries | Backend + SRE | [x] | Added `XAUTOCLAIM` stale-entry reclaim pre-pass before normal reads. |
| S21-T5 | Add poison message policy (max deliveries + DLQ/terminal) | Backend | [x] | Added dead-letter stream publishing for missing/invalid payloads. |
| S21-T6 | Add restart simulation integration tests | QA | [x] | Added and executed `tests/stream-consumer-restart-smoke.sh`. |

## Evidence
- Integration artifacts: `tests/artifacts/stream-consumer-audit/*`, `tests/artifacts/stream-restart-smoke/*`
- Verification log: `implementation-plan/stage-21-verification.md`
