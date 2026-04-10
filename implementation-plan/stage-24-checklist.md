# Stage 24 Checklist - Reliability and Operability Hardening

## Inventory and Design
- [x] Finalize config contract taxonomy (`required`, `optional`, `advanced`, `test-only`).
- [x] Finalize queue SLO and DLQ replay policy.
- [x] Finalize auth hardening threat model and token lifecycle decisions.

## Config Fail-Fast
- [x] Implement shared config validator module/crate.
- [x] Add `--check-config` to all runtime services.
- [x] Add `odctl doctor config` command.
- [x] Add env alias deprecation warnings and migration map.
- [x] Add CI gate: config check must pass for golden profile.

## Queue Reliability
- [x] Implement idempotency keys for classification/page-fetch jobs.
- [x] Enforce unique stream consumer naming strategy.
- [x] Add standardized DLQ envelope fields.
- [x] Add replay tooling with dry-run default and explicit scopes.
- [x] Add queue lag/pending-age/DLQ growth alert rules.

## Ops Diagnostics
- [x] Add unified platform diagnostics bundle command/script.
- [x] Include health, queue, auth, proxy, and reporting snapshots.
- [x] Add redaction mode for secrets and tokens.
- [x] Add triage mapping in runbook with escalation thresholds.

## Auth Hardening
- [x] Add refresh token issue/rotation/revocation APIs.
- [x] Integrate frontend session refresh and expiry UX.
- [x] Add service-account expiry defaults and rotation controls.
- [x] Add login/rate-limit telemetry and alerting.
- [x] Expand authz smoke matrix and negative-path coverage.

## Golden Deployment Path
- [x] Add `golden-local` and `golden-prodlike` compose profiles.
- [x] Add one-command bootstrap/verify helper.
- [x] Add profile-specific env documentation and defaults.
- [x] Validate fresh-clone startup and smoke on both profiles.

## Verification
- [x] Run workspace/unit/frontend validation gates.
- [x] Run queue restart/replay stress suite.
- [x] Run auth security smoke matrix.
- [x] Run golden profile bring-up and teardown drills.
- [x] Update stage trackers and evidence log.
