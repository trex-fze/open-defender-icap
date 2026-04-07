# Stage 10 Web Admin Operator Runbook

This runbook describes how operators validate Stage 10 management parity in the web admin UI.

## Preconditions

- Stack is running (`make compose-up` or `docker compose up -d --build`)
- Admin API is healthy (`http://localhost:19000/health/ready`)
- Web admin is reachable (`http://localhost:19001`)
- User has a token with at least `policy-admin` for full workflow testing

## Environment setup

For local UI testing:

```bash
cd web-admin
npm install
npm run dev
```

For test suites:

```bash
cd web-admin
npm test
npx start-server-and-test "npm run dev" http://127.0.0.1:19001 "npx cypress run --spec cypress/e2e/stage10-parity.cy.ts,cypress/e2e/accessibility.cy.ts"
```

## Operator workflow checklist

1. **Policy management**
   - Go to `/policies/new`
   - Create a draft with name/version/notes
   - Confirm redirect to policy detail
   - Publish draft and verify success state
2. **Pending classifications**
   - Go to `/classifications/pending`
   - Open manual decision panel
   - Apply decision with action/risk/reason
   - Confirm update message and queue refresh
3. **Allow / Deny list CRUD**
   - Go to `/overrides`
   - Create domain allow/block entry, edit it, then delete it
   - Confirm each mutation with success feedback
4. **Taxonomy activation**
   - Go to `/taxonomy`
   - Toggle a category and a subcategory checkbox (locked entries should remain disabled)
   - Click **Save** and confirm the success banner
   - Click **Reset** and verify the state matches the persisted profile
5. **Diagnostics**
   - `/diagnostics/page-content`: lookup key and verify latest + history
   - `/diagnostics/cache`: lookup key and evict cache entry
6. **Audit and reporting**
   - `/settings/rbac`: load CLI logs (optional operator filter)
   - `/reports`: change dimension/range/top filters, verify traffic summary, export CSV
7. **Ops status**
   - `/dashboard`: verify pending count and ops source badge (`live`, `partial`, or `mock`)

## Taxonomy Lockdown Monitoring (Stage 12)

Operators are responsible for confirming that the Stage 12 canonical taxonomy lock stays healthy during each deploy:

- **Metrics**
  - `taxonomy_fallback_total{reason="unknown_label"}` (emitted by LLM/reclass workers) should remain near zero; non-zero spikes mean upstream sources are still emitting legacy categories. Investigate by sampling `svc-llm-worker` logs for `taxonomy.fallback` events.
  - `taxonomy_activation_changes_total` (admin-api) should increment only when an operator intentionally clicks **Save** on the taxonomy page. Unexpected increments suggest automation or scripts are mutating activation state.
- **Audit stream**
  - Search admin-api logs for `taxonomy.mutation.blocked`; that event confirms locked CRUD routes are still being called (for example by an old CLI). Coordinate with the caller to remove the request; do **not** re-enable mutations.
- **UI verification**
  - `/taxonomy` must show the canonical version/updated metadata banner. Unknown/Unclassified should always be present; toggling it off should immediately disable all nested subcategories.

If fallback or blocked-mutation metrics climb steadily for more than 5 minutes, halt any rollout, collect the offending payloads, and page the taxonomy owner.

## Stage 12 Rollout / Rollback Procedure

1. **Pre-flight**
   - Confirm `config/canonical-taxonomy.json` change (if any) is reviewed and versioned.
   - Ensure `OD_TAXONOMY_MUTATION_ENABLED` is **unset/false** in all environments before deploying.
2. **Rollout steps**
   - Deploy admin-api, policy-engine, llm-worker, and reclass-worker together (they all read the canonical taxonomy).
   - After deploy, hit `GET /api/v1/taxonomy` and verify the returned `version`, `updated_at`, and `activation` flags match the canonical file.
   - Watch `taxonomy_fallback_total` and `taxonomy_activation_changes_total` for 10 minutes; if both are flat, the rollout is healthy.
3. **Rollback plan**
   - If a bad taxonomy update ships, revert the commit touching `config/canonical-taxonomy.json`, redeploy the stack, and reload the UI.
   - Only if an emergency structural edit is required, set `OD_TAXONOMY_MUTATION_ENABLED=true` on admin-api, redeploy **that service only**, perform the minimal mutation through the legacy endpoint, then immediately flip the flag back to false and redeploy to restore the lock. Record the event in the incident log.
   - If LLM/reclass fall back continuously because of a faulty canonical entry, disable the affected category via `/taxonomy`, redeploy the corrected taxonomy file, then re-enable the category once validation passes.

Document every rollout/rollback in the release notes with the taxonomy version string and activation snapshot for auditability.

## Screenshot capture checklist

Capture one screenshot for each of the following and attach to release evidence:

- Policy draft create form and publish confirmation
- Pending decision panel before and after apply
- Allow / Deny form and table row operations
- Taxonomy category/subcategory edit state
- Page content diagnostics with history table
- Cache diagnostics lookup + evict confirmation
- Settings CLI audit logs table
- Reports traffic summary cards and top tables
- Dashboard ops source indicator and queue counts

Store screenshots under `docs/evidence/stage10-web-admin/` using this naming convention:

- `01-policy-draft.png`
- `02-pending-decision.png`
- `03-overrides-crud.png`
- `04-taxonomy-activation.png`
- `05-page-content-diagnostics.png`
- `06-cache-diagnostics.png`
- `07-cli-logs.png`
- `08-reports-traffic.png`
- `09-dashboard-ops.png`

## Troubleshooting

- **UI shows mock mode unexpectedly**: verify `VITE_ADMIN_API_URL`/`VITE_ADMIN_API_FALLBACK` resolve to the live Admin API and that a bootstrap token (`VITE_ADMIN_TOKEN` or `VITE_DEFAULT_ADMIN_TOKEN`) is present in local storage.
- **Policy Version History shows `Failed to load version history: Failed to fetch`**: confirm `http://localhost:19000/health/ready` is healthy, verify `OD_ADMIN_CORS_ALLOW_ORIGIN` allows the web-admin origin (`http://localhost:19001` by default), and check browser devtools for blocked CORS/network requests.
- **Recorded action differs from effective action in Classifications**: this is expected when overrides, policy updates, or taxonomy activation changed after persistence. Use `effective_action` and `effective_decision_source` as current enforcement truth.
- **Unexpected block with `Review` action**: current runtime enforces `Review` as a blocked response with review-specific message text. Confirm policy intent in draft/version history before publish.
- **Sites stay on holding page (`ContentPending`)**: verify `classification_requests` row exists for the canonical domain key, and confirm both `classification-jobs` and `page-fetch-jobs` streams receive entries.
- **Delayed terminalization from `ContentPending`**: run `tests/content-pending-smoke.sh` and inspect generated diagnostics under `tests/artifacts/content-pending/diag-*`; validate `classification_requests.status` progression (`waiting_content` -> cleared/failed), confirm latest `page_contents.fetch_status` for the same normalized key, then check key-filtered worker logs (`diag-llm-worker-*-<host>.log`, `diag-page-fetcher-*-<host>.log`) for retry exhaustion or fetch failures before escalating.
- **One-command pending diagnostics**: run `NORMALIZED_KEY=domain:example.com HOST_TAG=local bash tests/ops/content-pending-diagnostics.sh`; inspect bundle under `tests/artifacts/ops-triage/...` for DB snapshots, queue tails, and key-filtered service logs.
- **403 from mutations**: verify role claims include required permissions.
- **No CLI logs shown**: ensure admin API has `audit` access and data exists.
- **Ops provider list empty**: set `VITE_LLM_PROVIDERS_URL` to worker providers endpoint.
