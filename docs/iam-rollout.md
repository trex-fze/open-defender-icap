# IAM Rollout Runbook

This runbook covers the steps we follow to deploy or roll back the Stage 11 IAM changes. Keep it alongside the Stage 11 checklist so we have a single source of truth when running maintenance windows.

## Preflight

1. Announce the rollout window in the #on-call and #platform channels.
2. Take a database snapshot or verify that the automated backups completed within the last hour.
3. Export the current `audit_events` table for safekeeping: `psql $DATABASE_URL -c "COPY audit_events TO STDOUT WITH CSV" > audit_events_pre_iam.csv`.
4. Ensure `ADMIN_TEST_DATABASE_URL` is set locally so we can run SQLx-backed tests before touching prod: `export ADMIN_TEST_DATABASE_URL=postgres://...`.

## Migration Steps

1. Apply migrations (including `0009_iam_audit_integrity.sql`):

   ```bash
   cargo test -p admin-api --tests   # validates migrations + audit constraints
   docker compose run --rm admin-api cargo sqlx migrate run
   ```

2. Seed initial IAM records (service accounts, operators) via `odctl iam ...` or the `/settings/iam` UI.
3. Deploy the Admin API and policy engine images. Both services are now configured to call `/api/v1/iam/whoami`.
4. Verify the resolver behavior with the smoke matrix in `docs/authz-smoke-matrix.md`.
5. Run the compose integration suite to exercise the full stack (Admin API, policy engine, ICAP adaptor, workers):

   ```bash
   make compose-test   # defined in the repo Makefile
   ```

   The test bundle runs `cargo test`, `npm run build`, and the ICAP→policy happy path inside Docker.

## Compatibility Settings

* Leave `allow_claim_fallback = true` during the first deployment so JWT-only operators can still authenticate.
* After seeding users and service accounts, flip the flag to `false` and restart the Admin API. Document the change in the release notes.

## Rollback

1. If the rollout is less than one hour old, restore the database snapshot taken during preflight.
2. Otherwise, manually drop the IAM tables: `psql $DATABASE_URL -c "DROP TABLE IF EXISTS iam_users CASCADE; ..."`.
3. Redeploy the previous Admin API/Policy Engine images and restore the exported `audit_events` CSV if necessary.
4. Set `allow_claim_fallback = true` and restart services to fall back to legacy JWT roles.

Document every decision in the incident tracker so we can audit the change later.
