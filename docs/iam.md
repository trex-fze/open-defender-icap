# IAM Local Authentication Guide

Stage 11 now supports **local username/password auth** without an IdP. RBAC still comes from IAM users, groups, and roles; only the authentication mechanism changed.

## Auth Modes

Configure Admin API auth mode in `config/admin-api.json` (`auth.mode`) or via `OD_AUTH_MODE`:

* `local` (default): accept locally issued JWTs + service account tokens.
* `hybrid`: accept both local JWTs and OIDC JWTs.
* `oidc`: accept OIDC JWTs + service account tokens.

For local/hybrid modes, set `OD_LOCAL_AUTH_JWT_SECRET` so the API can issue and validate local bearer tokens.

## Required Environment Variables

* `OD_DEFAULT_ADMIN_PASSWORD` – bootstrap password for the first local admin user (`admin`).
* `OD_LOCAL_AUTH_JWT_SECRET` – HMAC secret for local JWT issuance.

Optional hardening knobs:

* `OD_LOCAL_AUTH_TTL_SECONDS` (default `3600`)
* `OD_LOCAL_AUTH_MAX_FAILED_ATTEMPTS` (default `5`)
* `OD_LOCAL_AUTH_LOCKOUT_SECONDS` (default `900`)

## Default Admin Bootstrap

On startup (local/hybrid mode), the Admin API checks whether any active `policy-admin` user exists.

If none exists, it creates:

* `username`: `admin`
* `email`: `admin@local`
* `display_name`: `Default Admin`
* `password`: value of `OD_DEFAULT_ADMIN_PASSWORD` (stored as Argon2 hash)
* role binding: `policy-admin`

This operation is idempotent and runs after migrations.

## Login Flow

`POST /api/v1/auth/login`

Request body:

```json
{
  "username": "admin",
  "password": "your-password"
}
```

Response:

```json
{
  "access_token": "<jwt>",
  "expires_in": 3600,
  "user": {
    "id": "...",
    "username": "admin",
    "email": "admin@local",
    "roles": ["policy-admin"],
    "permissions": ["iam:manage", "policy:edit"],
    "must_change_password": true
  }
}
```

Use the token as `Authorization: Bearer <access_token>` for Admin API calls and in the web frontend.

## Security Notes

* Failed logins increment counters and temporarily lock accounts after the configured threshold.
* Keep `OD_DEFAULT_ADMIN_PASSWORD` in a secret manager; rotate it immediately after first login.
* Service account tokens remain available for machine-to-machine automation (`X-Admin-Token`).
