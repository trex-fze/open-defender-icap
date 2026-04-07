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

## Stage status

- Stage 21 verification complete.

## Implemented migration slice

- Updated llm-worker stream consumer to:
  - create stream group idempotently (`XGROUP CREATE ... MKSTREAM`, BUSYGROUP-tolerant)
  - consume with `XREADGROUP`
  - ACK on successful processing (`XACK`)
- Updated page-fetcher stream consumer with the same group/create/read/ack pattern.
- Added stale-entry claim pre-pass in both workers using `XAUTOCLAIM ... JUSTID` before pending/new reads so entries abandoned by inactive consumers are reassigned for processing.
- Added poison/dead-letter fallback in both workers for malformed or missing stream payloads:
  - llm-worker -> `OD_LLM_STREAM_DEAD_LETTER` (default `classification-jobs-dlq`)
  - page-fetcher -> `OD_PAGE_FETCH_STREAM_DEAD_LETTER` (default `page-fetch-jobs-dlq`)
  - entries include `source_stream`, `entry_id`, `reason`, and raw `payload`.
- Validation:
  - `cargo test -p llm-worker` passed
  - `cargo test -p page-fetcher` passed

## Restart simulation

- Command:
  - `BUILD_IMAGES=1 bash tests/stream-consumer-restart-smoke.sh`
- Result:
  - PASS, with DLQ entry observed after worker restart and duplicate guard threshold respected.
- Evidence:
  - `tests/artifacts/stream-restart-smoke/<timestamp>/dlq.txt`
  - `tests/artifacts/stream-restart-smoke/<timestamp>/llm-worker.log`
