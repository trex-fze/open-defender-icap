# Stage 24 Checklist - Reliability and Operability Hardening

## Inventory and Design
- [ ] Finalize config contract taxonomy (`required`, `optional`, `advanced`, `test-only`).
- [ ] Finalize queue SLO and DLQ replay policy.
- [ ] Finalize auth hardening threat model and token lifecycle decisions.

## Config Fail-Fast
- [ ] Implement shared config validator module/crate.
- [ ] Add `--check-config` to all runtime services.
- [ ] Add `odctl doctor config` command.
- [ ] Add env alias deprecation warnings and migration map.
- [ ] Add CI gate: config check must pass for golden profile.

## Queue Reliability
- [ ] Implement idempotency keys for classification/page-fetch jobs.
- [ ] Enforce unique stream consumer naming strategy.
- [ ] Add standardized DLQ envelope fields.
- [ ] Add replay tooling with dry-run default and explicit scopes.
- [ ] Add queue lag/pending-age/DLQ growth alert rules.

## Ops Diagnostics
- [ ] Add unified platform diagnostics bundle command/script.
- [ ] Include health, queue, auth, proxy, and reporting snapshots.
- [ ] Add redaction mode for secrets and tokens.
- [ ] Add triage mapping in runbook with escalation thresholds.

## Auth Hardening
- [ ] Add refresh token issue/rotation/revocation APIs.
- [ ] Integrate frontend session refresh and expiry UX.
- [ ] Add service-account expiry defaults and rotation controls.
- [ ] Add login/rate-limit telemetry and alerting.
- [ ] Expand authz smoke matrix and negative-path coverage.

## Golden Deployment Path
- [ ] Add `golden-local` and `golden-prodlike` compose profiles.
- [ ] Add one-command bootstrap/verify helper.
- [ ] Add profile-specific env documentation and defaults.
- [ ] Validate fresh-clone startup and smoke on both profiles.

## Verification
- [ ] Run workspace/unit/frontend validation gates.
- [ ] Run queue restart/replay stress suite.
- [ ] Run auth security smoke matrix.
- [ ] Run golden profile bring-up and teardown drills.
- [ ] Update stage trackers and evidence log.
