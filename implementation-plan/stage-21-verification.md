# Stage 21 Verification Log

Date: 2026-04-07

## Discovery evidence

- Command:
  - `bash tests/streams-consumer-audit.sh`
- Artifacts:
  - `tests/artifacts/stream-consumer-audit/<timestamp>/classification-jobs-groups.txt`
  - `tests/artifacts/stream-consumer-audit/<timestamp>/page-fetch-jobs-groups.txt`
  - `tests/artifacts/stream-consumer-audit/<timestamp>/worker-stream-read-patterns.txt`
- Notes:
  - Current worker read paths are `XREAD`-based (`xread_options`) in llm-worker and page-fetcher.
  - Group metadata snapshots establish baseline before `XREADGROUP` migration.

## Pending Verification
- Consumer-group migration implementation evidence.
- Restart-safe processing simulation outputs.
- Poison-message handling verification.

## Implemented migration slice

- Updated llm-worker stream consumer to:
  - create stream group idempotently (`XGROUP CREATE ... MKSTREAM`, BUSYGROUP-tolerant)
  - consume with `XREADGROUP`
  - ACK on successful processing (`XACK`)
- Updated page-fetcher stream consumer with the same group/create/read/ack pattern.
- Added stale-entry claim pre-pass in both workers using `XAUTOCLAIM ... JUSTID` before pending/new reads so entries abandoned by inactive consumers are reassigned for processing.
- Validation:
  - `cargo test -p llm-worker` passed
  - `cargo test -p page-fetcher` passed
