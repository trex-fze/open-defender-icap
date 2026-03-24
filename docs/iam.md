# IAM Compatibility & Resolver Guide

The Stage 11 rollout introduces a database-backed identity and access-management (IAM) model for the Admin API, policy engine, CLI, and ICAP adaptor. This document captures the compatibility switches and operational guidance you’ll need while migrating from static tokens or JWT-embedded roles to the persistent store.

## Resolver Topology

* The Admin API is the source of truth for roles. Every request passes through `AdminAuth`, which resolves service accounts, users, and group-derived roles.
* The policy engine no longer evaluates JWT claims locally. Instead, set `auth.resolver_url` in `config/policy-engine.json` (or `OD_IAM_RESOLVER_URL`) to point to the Admin API’s `/api/v1/iam/whoami`. Whatever Authorization/X-Admin-Token headers the policy engine receives are forwarded to this endpoint.
* `odctl` now auto-detects bearer tokens vs. static tokens and sends the appropriate header so IAM commands reuse the same resolver path.

## Fallback Modes

During migration you may need to honor legacy JWT claim roles for principals that haven’t yet been provisioned in the IAM database. Use the `allow_claim_fallback` flag inside `auth` in `config/admin-api.json`:

```json
{
  "auth": {
    "allow_claim_fallback": true
  }
}
```

* When `true` (default), callers with valid JWTs but no IAM record inherit the roles embedded in their token (`roles` claim or `scope`).
* When `false`, every authenticated caller must exist in the IAM database (user or service account). Missing principals receive `403 Forbidden`.

You can also override the flag with `OD_ALLOW_CLAIM_FALLBACK` if you add that env var to your deployment manifests.

## Static Token Compatibility

Static admin tokens still work, but they are now modeled as service accounts. If you keep `admin_token` configured, the Admin API creates an in-memory “static-admin” principal seeded with all builtin roles. Rotate the token by adding a real service account and migrating off the config value.

## Rollout Checklist

1. Seed users/groups/roles via `odctl iam ...` or the `/settings/iam` UI.
2. Deploy the Admin API with `allow_claim_fallback = true` so JWT-only users continue to function.
3. Point the policy engine (and any direct API callers) at the resolver URL.
4. Cut over automation to service accounts (tokens are only shown once on creation/rotation).
5. Flip `allow_claim_fallback` to `false` once every operator has an IAM entry.

## Observability

* `/api/v1/iam/audit` returns the last 500 IAM mutations.
* Existing `audit_events` rows also receive IAM events, so your Elastic/Promtail pipelines automatically capture them.
* The policy engine logs resolver errors (5xx) so you can detect misconfigurations quickly.

For a complete endpoint reference, see `docs/api-catalog.md` (Identity & Access Management section).

## Builtin Roles

| Role | Description | Key Permissions |
| --- | --- | --- |
| `policy-admin` | Full administrative access across overrides, policies, IAM, and reporting. | `iam:manage`, `policy:edit`, `overrides:write`, `audit:view`. |
| `policy-editor` | Create/update policies, taxonomy, and overrides. | `policy:edit`, `overrides:write`, `taxonomy:edit`. |
| `policy-viewer` | Read-only access to policies, overrides, reporting. | `policy:view`, `overrides:view`, `reporting:view`. |
| `review-approver` | Resolve review queue items and manual classifications. | `review:view`, `review:resolve`. |
| `auditor` | Retrieve audit logs and reporting dashboards. | `audit:view`, `reporting:view`. |

Service accounts inherit whichever roles you assign at creation/rotation time.

## Default Admin Bootstrap

Fresh environments still need one operator who can finish wiring real identities. Rather than shipping a plaintext username/password, we bootstrap a `default-admin` service account and hand the generated token to the on-call engineer.

### Create the Bootstrap Account

```bash
# Requires policy-admin on the caller (static token or JWT)
      --token "$ADMIN_TOKEN" \
      iam service-accounts create \
      --name default-admin \
      --description "Bootstrap admin" \
      --role policy-admin
```

The command prints JSON similar to:

```json
{
  "account": { "name": "default-admin", ... },
  "token": "svc.token.value",
  "roles": ["policy-admin"]
}
```

Copy the `token` immediately into your secrets manager (for example `DEFAULT_ADMIN_TOKEN`). It is **not** stored in plaintext anywhere else; you must rotate the service account if the token is lost.

### Using the Default Admin Token

* CLI/API: supply `--token "$DEFAULT_ADMIN_TOKEN"` (odctl) or `X-Admin-Token: $DEFAULT_ADMIN_TOKEN` for curl.
* Frontend: if you rely on OIDC, also create an IAM user with the same email/subject and assign `policy-admin` so that your own identity can log in. The default service account is meant for break-glass automation, not daily browsing.

### Clean-up

Once real administrators exist, rotate or delete the `default-admin` service account (`odctl iam service-accounts disable <id>`). Document the new source of truth for administrator access in your runbooks.
