# Operational Completion Matrix

This matrix converts recurring post-stage activities into explicit ownership, cadence, pass/fail criteria, and evidence paths.

| Control Area | Owner | Cadence | Gate / Threshold | Evidence Path |
| --- | --- | --- | --- | --- |
| Stage 18 reliability burn-in | SWG + Classification + QA | Release candidate + weekly | `RUNS=10` reliability harness pass rate >= 90%; failures produce diagnostics bundles | `tests/artifacts/content-pending-reliability/`, `tests/artifacts/ops-triage/` |
| Stage 20 diagnostics drill | SRE on-call | Weekly drill + incident response | `tests/ops/content-pending-diagnostics.sh` succeeds and captures DB + queue + logs for sampled key | `tests/artifacts/ops-triage/content-pending-*` |
| Taxonomy parity guard | Classification + Policy + QA | Weekly + before taxonomy release | `tests/taxonomy-parity.sh` passes with canonicalization/activation parity | `tests/artifacts/taxonomy-parity/` |
| Stream consumer restart parity | Backend + SRE | Release candidate | `tests/stream-consumer-restart-smoke.sh` passes with no stuck pending entries | `tests/artifacts/stream-consumer-restart/` |
| Cursor parity regression | Backend + Frontend + DevTools | Release candidate | `tests/policy-cursor-smoke.sh` passes; no cursor-chain gaps/duplicates in sampled endpoints | `tests/artifacts/policy-cursor-smoke/` |
| Stage 24 auth/ops hardening gate | Platform + Security + SRE | Release candidate | stage24 config gate workflow green; diagnostics bundle and auth checks present | `.github/workflows/stage24-config-gate.yml`, `tests/artifacts/ops-triage/platform-*` |

## Release Checklist Integration

- A release is blocked when any matrix gate fails without approved risk sign-off.
- Evidence artifacts must be attached to release notes or linked from verification logs.
- Incident-triggered runs must include the triggering key/domain and remediation notes.
