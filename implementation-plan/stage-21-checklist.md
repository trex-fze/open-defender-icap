# Stage 21 Checklist - Stream Consumer-Group Migration

## Discovery
- [x] List all stream consumers and message schemas.
- [x] Define group naming and consumer identity strategy.

## Migration
- [x] Add idempotent group creation at startup.
- [x] Replace `XREAD` loops with `XREADGROUP`.
- [x] ACK only after successful processing.
- [x] Add stale-claim policy (`XPENDING` + `XAUTOCLAIM`/`XCLAIM`).

## Reliability
- [x] Add max delivery / poison-message handling.
- [x] Add dead-letter stream or terminal status fallback.
- [x] Validate idempotent persistence under redelivery.

## Testing
- [x] Add restart simulation tests (kill consumer mid-flight).
- [x] Verify no dropped jobs and bounded duplicates.

## Completion
- [x] Mark Stage 21 complete in `implementation-plan/stage-plan.md`.
