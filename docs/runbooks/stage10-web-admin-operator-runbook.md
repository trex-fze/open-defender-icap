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
