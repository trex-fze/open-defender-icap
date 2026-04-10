# Synthetic Pending Cleanup and Prevention Runbook

## Scope

Use this runbook when synthetic/test keys (for example `domain:prompt-injection.*`) repeatedly appear in `Pending Sites` as `waiting_content`.

Typical cause:
- A historical synthetic key remains in classification/pending state.
- Pending reconciliation re-enqueues stale pending records.
- Page fetch keeps failing for invalid synthetic hostnames, so the key loops.

## Quick cleanup (automated)

From repo root:

```bash
# Preview only
DRY_RUN=1 tests/ops/cleanup-synthetic-pending.sh

# Execute cleanup
DRY_RUN=0 tests/ops/cleanup-synthetic-pending.sh
```

Default behavior:
- Targets `domain:prompt-injection.%` and `subdomain:prompt-injection.%`.
- Deletes classification + pending + page-content state through Admin API key delete endpoint.
- Verifies remaining counts in Postgres.

Optional full pending queue reset (use with care):

```bash
DRY_RUN=0 PURGE_ALL_PENDING=1 tests/ops/cleanup-synthetic-pending.sh
```

## Manual cleanup fallback

1. Pending queue only:
   - `DELETE /api/v1/classifications/pending`
2. Full key state removal (recommended for synthetic keys):
   - `DELETE /api/v1/classifications/:normalized_key`

## Prevention controls

### 1) Keep synthetic security smokes isolated

- Run security smoke scripts in isolated ephemeral environments (or dedicated DB) rather than long-lived shared dev stacks.
- Avoid leaving synthetic artifacts in shared environments.

### 2) Tune pending reconciliation for dev

If synthetic churn is noisy in local dev, tune or disable reconciler:

- `OD_PENDING_RECONCILE_ENABLED=false` (disables periodic stale requeue loop)
- or reduce aggressiveness:
  - increase `OD_PENDING_RECONCILE_STALE_MINUTES`
  - decrease `OD_PENDING_RECONCILE_BATCH`

After changing env values, recreate `llm-worker`.

### 3) Add post-test cleanup discipline

- After running synthetic security tests, run cleanup script as part of teardown.
- Keep `DRY_RUN=1` in CI checks and run `DRY_RUN=0` only in teardown jobs.

### 4) Monitor for recurrence

Add periodic check for keys matching:

- `domain:prompt-injection.%`
- `subdomain:prompt-injection.%`

Escalate if these remain in `waiting_content` for extended windows.

## Verification

Confirm no synthetic rows remain:

```bash
docker compose --env-file .env -f deploy/docker/docker-compose.yml exec -T postgres \
  bash -lc "psql -U defender -d defender_admin -P pager=off -c \"SELECT count(*) FROM classifications WHERE normalized_key LIKE 'domain:prompt-injection.%'; SELECT count(*) FROM classification_requests WHERE normalized_key LIKE 'domain:prompt-injection.%';\""
```

Expected: both counts should be `0`.
