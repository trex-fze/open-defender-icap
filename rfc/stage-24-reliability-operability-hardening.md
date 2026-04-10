# Stage 24 RFC - Reliability and Operability Hardening Program

**Status**: Proposed  
**Primary Scope**: platform-wide hardening across config, queues, diagnostics, auth, and deployment workflows  
**Decision Owner**: Platform + Security + SRE  
**Priority**: Critical path to production trust

## 1. Problem Statement

The platform has strong feature velocity and solid operational docs, but reliability still depends too heavily on operator expertise and cross-service tribal knowledge. We need a hardening program to reduce configuration drift, queue fragility, auth risk, and deployment ambiguity.

## 2. Goals

1. Establish one authoritative config contract with fail-fast validation before service startup.
2. Raise queue processing from restart-safe to production-resilient (idempotent, observable, replayable).
3. Deliver one-command incident diagnostics bundles for first-response triage.
4. Harden auth/session lifecycle for human and service principals.
5. Define and enforce a single golden-path deployment profile for local and prod-like operations.

## 3. Non-Goals

- Replacing all existing env vars or rewriting all services at once.
- Replacing Redis Streams with another queue technology.
- Building full enterprise IAM federation beyond current stage scope.
- Replatforming to Kubernetes in this stage.

## 4. Current-State Findings

- Config loading and env merge behavior is implemented per service with mixed strictness.
- Stream groups and stale-claim support exist, but idempotency/replay ergonomics are incomplete.
- Diagnostics automation is strong for content-pending incidents but not yet platform-wide.
- Browser auth/session model relies on local storage access tokens and lacks refresh-token flow.
- Canonical env-file policy is now documented, but there is no single hardened deployment profile.

## 5. Proposed Design

### 5.1 Workstream A - Config Contract and Fail-Fast

- Introduce a shared config schema/validation layer and per-service validators.
- Add startup preflight command (`odctl doctor config`) and service `--check-config` mode.
- Enforce secret hygiene guards (block weak/default secrets outside explicit dev mode).
- Add env alias deprecation map with warning + removal timeline.

### 5.2 Workstream B - Queue Reliability Hardening

- Add idempotency keys for classification/page-fetch processing paths.
- Enforce unique consumer identity defaults (hostname/pod suffix) to avoid accidental shared consumers.
- Expand DLQ envelope metadata (reason, delivery count, first_seen_at, last_seen_at, trace_id).
- Add replay tooling (`odctl queue dlq list|replay|drop`) with dry-run default and scope guards.
- Add stream SLO metrics: lag, pending age percentiles, DLQ growth, replay success/failure.

### 5.3 Workstream C - Unified Ops Diagnostics Bundle

- Add one-command bundle script/command (for example `tests/ops/platform-diagnostics.sh`).
- Capture service health, queue state, DLQ heads, auth/session failures, proxy ACL posture, and reporting coverage snapshots.
- Emit one timestamped artifact tree with redaction option for secrets/tokens.

### 5.4 Workstream D - Auth Hardening

- Add refresh-token flow for local auth sessions (rotating refresh tokens + revocation checks).
- Shift browser session handling to memory-first model with explicit persistence policy.
- Define service-account baseline policy: expiry defaults, mandatory rotation windows, and scoped permissions.
- Add audit events + monitoring for session refresh/revoke anomalies and brute-force lockout patterns.

### 5.5 Workstream E - Golden Path Deployment Profile

- Introduce compose profile(s): `golden-local` and `golden-prodlike`.
- Publish strict required env set and profile-specific defaults.
- Add one-command bootstrap + verify workflow.
- Add known-good smoke matrix tied to golden-profile quality gates.

## 6. Security and Risk Considerations

- Strict startup validation can block misconfigured environments; output must include actionable remediation.
- Replay tooling can be dangerous if unguarded; keep dry-run default and require explicit scope.
- Refresh tokens increase auth complexity; enforce revocation, expiry, and auditing from day one.

## 7. Testing Strategy

- **Unit**: config schema/validators, auth token lifecycle, queue idempotency behavior.
- **Integration**: restart/duplication/replay chaos tests for streams.
- **E2E**: golden profile bring-up + auth + queue + reporting health.
- **Security**: expanded authz matrix + brute-force lockout and token replay tests.

## 8. Acceptance Criteria

1. All runtime services support validated startup preflight with actionable error output.
2. Queue processing shows bounded duplicates and deterministic DLQ replay behavior in restart chaos tests.
3. One-command diagnostics bundle captures the top incident classes in under 5 minutes.
4. Auth supports session refresh and token revocation with audited events.
5. Golden profile starts/stops/verifies with a single documented command path and passes smoke gates.
