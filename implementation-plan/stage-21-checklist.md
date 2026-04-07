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
- [ ] Add max delivery / poison-message handling.
- [ ] Add dead-letter stream or terminal status fallback.
- [ ] Validate idempotent persistence under redelivery.

## Testing
- [ ] Add restart simulation tests (kill consumer mid-flight).
- [ ] Verify no dropped jobs and bounded duplicates.

## Completion
- [ ] Mark Stage 21 complete in `implementation-plan/stage-plan.md`.
