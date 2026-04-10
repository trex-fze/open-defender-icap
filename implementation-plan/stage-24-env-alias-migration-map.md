# Stage 24 Env Alias Migration Map

This map defines the current legacy env aliases accepted at runtime and the canonical variable operators must migrate to.

## Active Alias Map

| Service | Legacy Alias | Canonical Variable | Runtime Behavior |
| --- | --- | --- | --- |
| `admin-api` | `DATABASE_URL` | `OD_ADMIN_DATABASE_URL` | Alias accepted with startup warning.
| `policy-engine` | `DATABASE_URL` | `OD_POLICY_DATABASE_URL` | Alias accepted with startup warning.
| `policy-engine` (taxonomy activation DB) | `OD_ADMIN_DATABASE_URL` | `OD_TAXONOMY_DATABASE_URL` | Alias accepted with startup warning.

## Migration Guidance

1. Set canonical variables in root `/.env` for all deployment profiles.
2. Remove legacy aliases from compose overrides and shell wrappers.
3. Run `odctl doctor config` and each service `--check-config` mode before startup.

## Removal Timeline

- Stage 24: warn on alias usage and document migration path.
- Stage 25 target: make alias usage a hard startup failure in golden profile CI.
- Stage 26 target: remove alias fallback handling from runtime services.
