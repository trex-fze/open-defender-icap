# Continuous Validation Log

## 2026-04-07 regression drill

- `RUNS=10 bash tests/security/facebook-e2e-reliability.sh` -> pass rate 100% (10/10).
- `NORMALIZED_KEY=domain:facebook.com HOST_TAG=drill bash tests/ops/content-pending-diagnostics.sh` -> diagnostics bundle generated successfully.
- `bash tests/taxonomy-parity.sh` -> all parity matrix checks passed.
- `BUILD_IMAGES=0 bash tests/stream-consumer-restart-smoke.sh` -> restart + DLQ smoke passed.
- `bash tests/policy-cursor-smoke.sh` -> cursor chain + index presence smoke passed.

Notes:
- Artifacts were generated under `tests/artifacts/*` during the drill and then cleaned from the working tree to keep the repository tidy.
- This drill satisfies the current Stage 18/19/20/21/22 operational follow-through checks listed in `implementation-plan/stage-plan.md`.
