# Stage 7 – Unit Test Coverage Checklist

This document captures the modules included in the Stage 7 unit-test sweep and provides the commands required to re-run the suites locally/CI. Use `tests/unit.sh` as the authoritative runner; the table below summarizes ownership.

| Component | Command | Notes |
| --- | --- | --- |
| Workspace Rust crates (admin-api, icap-adaptor, policy-engine, workers, event-ingester, CLI) | `cargo test --workspace` | Runs all Rust unit tests; fails fast on panic. Stage 6 metrics additions verified here. |
| Web Admin (React) | `npm run test` (inside `web-admin/`) | Uses Vitest + RTL for Auth context, ProtectedRoute, hooks. Coverage threshold tracked via Vitest config. |
| CLI integration tests | `cargo test -p odctl --tests` | Executes `tests/cli_integration.rs` (device flow + policy/override commands). Included automatically in `cargo test --workspace`. |
| Stage 6 ingestion helpers | `cargo test -p event-ingester` | Ensures Elasticsearch template utilities + Filebeat envelope parsing remain deterministic. |

## How to run locally

```bash
# from repo root
tests/unit.sh
```

This script executes both the Rust workspace tests and the web-admin Vitest suite (installing NPM deps if needed). Record the run log as evidence for S7‑T1.

## Reporting
- Attach the `tests/unit.sh` console output to the Stage 7 evidence package.
- For CI, add a pipeline step that runs `tests/unit.sh`; failures block merges.
