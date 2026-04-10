# Stage 24 Implementation Plan - Reliability and Operability Hardening

**Status**: Proposed  
**Depends on**: Stages 20, 21, and 23 hardening foundations  
**Execution checklist**: `implementation-plan/stage-24-checklist.md`

## 1) Delivery Strategy

Implement in six phases:

- Phase A: baseline inventory and contract definition
- Phase B: config fail-fast framework
- Phase C: queue hardening and replay tooling
- Phase D: unified diagnostics bundle
- Phase E: auth/session hardening
- Phase F: golden deployment profile and rollout docs

## 2) Work Breakdown

| Task ID | Description | Area | Dependencies | Output |
| --- | --- | --- | --- | --- |
| S24-T1 | Publish unified env/config schema and validator interfaces | Platform | none | config contract + validator skeleton |
| S24-T2 | Add `--check-config` to admin-api/policy-engine/event-ingester/llm-worker/page-fetcher | Backend | S24-T1 | fail-fast startup checks |
| S24-T3 | Add `odctl doctor config` preflight command | DevTools | S24-T1 | operator preflight command |
| S24-T4 | Implement alias/deprecation warnings for legacy env vars | Platform | S24-T1 | migration-safe env evolution |
| S24-T5 | Add queue idempotency key handling and duplicate suppression metrics | Workers | none | duplicate-safe processing path |
| S24-T6 | Enforce unique consumer identity default strategy | Workers + SRE | S24-T5 | scale-safe consumer identities |
| S24-T7 | Standardize DLQ envelope and add replay CLI/tooling | Workers + DevTools | S24-T5 | controlled redrive workflows |
| S24-T8 | Add stream SLO metrics and alerts (lag, age, DLQ growth) | SRE | S24-T5 | queue observability baseline |
| S24-T9 | Build unified platform diagnostics bundle command/script | SRE + DevTools | S24-T3, S24-T7 | one-command support bundle |
| S24-T10 | Implement refresh-token lifecycle for local auth | Security + Admin API + Web Admin | none | resilient session model |
| S24-T11 | Add service-account TTL/rotation policy controls and audits | Security + IAM | S24-T10 | hardened automation auth |
| S24-T12 | Add auth abuse guards (rate-limit/lockout telemetry + alerts) | Security + SRE | S24-T10 | auth attack resilience |
| S24-T13 | Add compose golden profiles and bootstrap verify script | Platform + SRE | S24-T2, S24-T8 | deterministic deployment path |
| S24-T14 | Update runbooks/docs and migration guidance | Docs | S24-T2..S24-T13 | operator-ready documentation |
| S24-T15 | Add restart/replay/auth integration stress suite | QA | S24-T5, S24-T10, S24-T13 | release confidence gates |

## 3) Phase Plan

### Phase A - Contract and Baseline

- Inventory runtime config entry points and env alias usage.
- Define strict/optional/advanced/test-only variable classes.
- Exit criteria: signed-off contract table and migration map.

### Phase B - Config Fail-Fast

- Implement shared validator interfaces and service check mode.
- Add clear remediation text for missing/invalid values.
- Exit criteria: deterministic startup failures on invalid config.

### Phase C - Queue Reliability

- Add idempotency markers and duplicate suppression behavior.
- Add DLQ replay controls with dry-run default.
- Exit criteria: restart chaos tests pass with bounded duplicates and deterministic redrive behavior.

### Phase D - Unified Diagnostics

- Build support-bundle command across health, auth, queue, proxy, and reporting signals.
- Standardize artifact structure and redaction.
- Exit criteria: one command produces usable triage bundle in under 5 minutes.

### Phase E - Auth Hardening

- Implement refresh token issue/rotate/revoke flow.
- Add service-account lifecycle policy controls and audit extensions.
- Exit criteria: session continuity + revocation + auditable lifecycle complete.

### Phase F - Golden Path

- Add documented compose profile(s) with strict env contract.
- Add one-command bootstrap and verify workflow.
- Exit criteria: fresh-clone startup and smoke pass on golden profile.

## 4) Validation and Evidence

- Backend: `cargo test --workspace`
- Frontend: `npm test` and `npm run build` in `web-admin`
- Reliability: stream restart/replay suite and diagnostics bundle drills
- Security: authz matrix + token/session negative-path tests

Evidence targets:

- `tests/artifacts/ops-triage/*` (expanded bundle)
- stage-specific verification note under `implementation-plan/`
- updated runbooks and env reference docs

## 5) Acceptance Tracking

- [ ] `odctl doctor config` validates required runtime knobs before startup.
- [ ] Service `--check-config` exits non-zero on misconfig with actionable remediation text.
- [ ] Queue restart/replay stress suite passes within defined SLO bounds.
- [ ] Unified diagnostics bundle is generated in under five minutes.
- [ ] Refresh token flow and service-account rotation policies are enforced and audited.
- [ ] Golden profile documentation and smoke gates are complete.
