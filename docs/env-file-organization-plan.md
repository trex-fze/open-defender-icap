# Environment File Organization Plan

## Objective

Eliminate environment file ambiguity across local development, docker compose, tests, and web-admin so operators have one predictable source of truth.

## Current findings

- Multiple local env files are used in practice:
  - `/.env` (root, not tracked)
  - `deploy/docker/.env` (not tracked, but commonly used accidentally)
  - `/.env.example` (tracked template)
- Compose command context (`deploy/docker`) can implicitly prefer `deploy/docker/.env` when `--env-file` is not provided.
- Several scripts previously inferred secrets from `deploy/docker/.env`.
- Frontend `VITE_*` keys were documented with `web-admin/.env` but no template file existed.

## Target model

### Canonical files

- `/.env.example` (tracked): full stack template with safe defaults/placeholders.
- `/.env` (local): only real secrets and environment overrides used by compose/services.
- `web-admin/.env.example` (tracked): standalone frontend-only variables (`VITE_*`).
- `web-admin/.env` (local): optional for standalone Vite development.

### Deprecated local file

- `deploy/docker/.env` should not be used for runtime config.

## Enforced behavior

- All compose entrypoints should explicitly pass `--env-file ../../.env` (or absolute root env path in scripts).
- Test scripts should default to root `/.env` when invoking compose.
- Docs should consistently point to root `/.env` as the compose runtime source.

## Implementation checklist

1. Expand and normalize `/.env.example` to cover compose variables.
2. Add `web-admin/.env.example` for `VITE_*` vars.
3. Ignore local-only env files in git:
   - `/.env`
   - `deploy/docker/.env`
   - `web-admin/.env`
4. Update Makefile compose wrappers to pass explicit env file.
5. Update tests/scripts to pass explicit compose env file and stop reading `deploy/docker/.env`.
6. Update docs for canonical env policy and startup commands.
7. Validate fresh-clone startup and smoke tests with only root `/.env`.

## Operational migration steps

1. If you currently have `deploy/docker/.env`, migrate values into root `/.env`.
2. Remove or archive `deploy/docker/.env` locally after migration.
3. Run `make compose-up` to verify services resolve variables from root env.
4. Re-run smoke checks (`tests/integration.sh`, `tests/policy-cursor-smoke.sh`) to confirm script compatibility.

## Validation matrix

- Compose startup works with root `/.env` only.
- No script depends on `deploy/docker/.env`.
- Standalone web-admin works using `web-admin/.env` copied from `web-admin/.env.example`.
- Docs are consistent across README, user guide, and docker runbooks.
