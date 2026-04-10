# Stage 24 Finalized Decisions

## 1) Config Contract Taxonomy

Service variables are classified as:

- `required`: startup must fail if missing/invalid (`--check-config` and `odctl doctor config` enforce).
- `optional`: feature toggles and integrations that can be absent without startup failure.
- `advanced`: tuning knobs for queue claim, retries, and provider routing.
- `test-only`: smoke/test harness controls and synthetic-load helpers.

Rules:
- canonical env names are required for production; legacy aliases emit deprecation warnings.
- default/test secrets are rejected where local/hybrid auth relies on secret material.

## 2) Queue SLO and DLQ Replay Policy

Queue SLO (Stage 24 enforcement baseline):
- pending age p95 target: under 10 minutes (alert when above threshold).
- processing stall detection: started rate without corresponding completion rate for 10 minutes.
- DLQ growth detection: any sustained increase over 10-minute window triggers warning.

DLQ replay policy:
- replay/drop commands are scope-gated (`--reason` and/or `--source-stream` required).
- default mode is dry-run; mutation requires explicit `--execute`.
- DLQ envelope includes `reason`, `delivery_count`, `first_seen_at`, `last_seen_at`, `trace_id` for auditability.

## 3) Auth Hardening Threat-Model Decisions

Session/token model:
- short-lived access tokens plus rotating refresh tokens for local/hybrid auth.
- refresh rotation revokes prior token and links replacement; logout revokes refresh tokens.
- token-version checks invalidate stale refresh chains after password/security-sensitive changes.

Abuse/monitoring model:
- lockout and failed-login counters are first-class metrics with alerting.
- refresh failure spikes are monitored as potential abuse or outage signals.

Service-account model:
- service-account tokens carry explicit expiry (`expires_at`).
- default TTL is controlled by `OD_IAM_SERVICE_TOKEN_TTL_DAYS` and enforced on verification.
