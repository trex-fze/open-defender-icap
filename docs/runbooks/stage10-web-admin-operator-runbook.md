# Stage 10 Web Admin Operator Runbook

This runbook describes how operators validate Stage 10 management parity in the web admin UI.

## Preconditions

- Stack is running (`make compose-up` or `docker compose up -d --build`)
- Admin API is healthy (`http://localhost:19000/health/ready`)
- Web admin is reachable (`http://localhost:19001`)
- User has a token with at least `policy-admin` and `review-approver` for full workflow testing

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
2. **Review queue decisions**
   - Go to `/review-queue`
   - Approve one item and reject one item
   - Confirm success message + row refresh
3. **Pending classifications**
   - Go to `/classifications/pending`
   - Open manual decision panel
   - Apply decision with action/risk/reason
   - Confirm update message and queue refresh
4. **Overrides CRUD**
   - Go to `/overrides`
   - Create override, edit it, then delete it
   - Confirm each mutation with success feedback
5. **Taxonomy activation**
   - Go to `/taxonomy`
   - Toggle a category and a subcategory checkbox (locked entries should remain disabled)
   - Click **Save** and confirm the success banner
   - Click **Reset** and verify the state matches the persisted profile
6. **Diagnostics**
   - `/diagnostics/page-content`: lookup key and verify latest + history
   - `/diagnostics/cache`: lookup key and evict cache entry
7. **Audit and reporting**
   - `/settings/rbac`: load CLI logs (optional operator filter)
   - `/reports`: change dimension/range/top filters, verify traffic summary, export CSV
8. **Ops status**
   - `/dashboard`: verify pending/review counts and ops source badge (`live`, `partial`, or `mock`)

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
- Review queue action buttons and resolve confirmation
- Pending decision panel before and after apply
- Overrides form and table row operations
- Taxonomy category/subcategory edit state
- Page content diagnostics with history table
- Cache diagnostics lookup + evict confirmation
- Settings CLI audit logs table
- Reports traffic summary cards and top tables
- Dashboard ops source indicator and queue counts

Store screenshots under `docs/evidence/stage10-web-admin/` using this naming convention:

- `01-policy-draft.png`
- `02-review-resolve.png`
- `03-pending-decision.png`
- `04-overrides-crud.png`
- `05-taxonomy-activation.png`
- `06-page-content-diagnostics.png`
- `07-cache-diagnostics.png`
- `08-cli-logs.png`
- `09-reports-traffic.png`
- `10-dashboard-ops.png`

## Troubleshooting

- **UI shows mock mode unexpectedly**: verify `VITE_ADMIN_API_URL` and token state in local storage.
- **403 from mutations**: verify role claims include required permissions.
- **No CLI logs shown**: ensure admin API has `audit` access and data exists.
- **Ops provider list empty**: set `VITE_LLM_PROVIDERS_URL` to worker providers endpoint.
