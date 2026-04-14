# Stage 10 Web Admin Operator Runbook

This runbook describes how operators validate Stage 10 management parity in the web admin UI.

## Preconditions

- Stack is running (`make compose-up` or `docker compose --env-file .env -f deploy/docker/docker-compose.yml up -d --build`)
- Admin API is healthy (`http://localhost:19000/health/ready`)
- Web admin is reachable (`https://localhost:19001`)
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
   - Apply manual classification with category/subcategory/reason
   - Confirm update message and queue refresh
   - Test row-level **Delete** for one pending record
   - Test guarded **Delete All Pending** by typing exact phrase `DELETE ALL`
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
   - `/dashboard`: change range/top filters and verify analytics tables
7. **Allow / Deny exchange**
   - `/settings/allow-deny-exchange`: export Allow list and Deny list separately
   - Import line-by-line text in dry-run, then apply merge/replace (replace is action-scoped)
8. **Ops status**
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

Taxonomy parity validation command:

- `bash tests/taxonomy-parity.sh` executes the cross-service canonicalization/activation regression set and writes evidence to `tests/artifacts/taxonomy-parity/<timestamp>/summary.tsv`.

## Stage 13 Canonicalization Controls

- **PSL canonicalization baseline**: canonical domain derivation uses PSL-aware parsing in `common-types`; unexpected cross-domain collapse should be treated as a defect.
- **Collapse-ratio telemetry**: watch `classification_canonicalization_collapse_ratio` with supporting totals (`classification_canonicalization_total`, `classification_canonicalization_collapsed_total`) on ICAP metrics.
- **Conditional tenant exception policy**: keep tenant/domain exception list disabled by default. Open an implementation task only if either (a) repeated collision incidents are confirmed for the same registrable domain family, or (b) collapse ratio anomaly persists for two release cycles with verified policy impact.

Tenant/domain exceptions are now supported via `canonicalization.tenant_domain_exceptions` in `config/icap.json` and `config/admin-api.json`. Keys are tenant identifiers (or `default`/`*`), values are registrable domains that must retain subdomain-level granularity.

## Stage 15 Cursor Performance Baseline

- Capture baseline query plans for high-volume cursor endpoints before release (`EXPLAIN ANALYZE` against representative datasets).
- Track p95 API latency trend for list routes in regression drills; investigate if p95 regresses by >25% versus prior release baseline.
- Keep backward pagination contract explicit: cursor endpoints now emit directional `next_cursor`/`prev_cursor`; clients should prefer server-provided `prev_cursor` for reverse traversal.

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
- Settings Allow / Deny exchange (export + dry-run import summary)
- Dashboard analytics tables
- Dashboard analytics panel (unique clients, bandwidth, hourly trend, blocked domains/requesters)

Store screenshots under `docs/evidence/stage10-web-admin/` using this naming convention:

- `01-policy-draft.png`
- `02-pending-decision.png`
- `03-overrides-crud.png`
- `04-taxonomy-activation.png`
- `05-page-content-diagnostics.png`
- `06-cache-diagnostics.png`
- `07-cli-logs.png`
- `08-allow-deny-exchange.png`
- `09-dashboard-reporting.png`
- `10-dashboard-ops.png`

## Troubleshooting

- **UI shows mock mode unexpectedly**: in compose HTTPS mode, keep `VITE_ADMIN_API_URL`/`VITE_ADMIN_API_FALLBACK` empty so requests use same-origin `/api/*`; for standalone dev ensure they resolve to the live Admin API. Confirm browser auth state exists in local storage (`od.admin.tokens`) and `VITE_ADMIN_TOKEN_MODE` matches your token type (`auto` is recommended).
- **Policy Version History shows `Failed to load version history: Failed to fetch`**: confirm `http://localhost:19000/health/ready` is healthy, verify `OD_ADMIN_CORS_ALLOW_ORIGIN` allows the web-admin origin (`https://localhost:19001` by default), and check browser devtools for blocked CORS/network requests.
- **Recorded action differs from effective action in Classifications**: this is expected when overrides, policy updates, or taxonomy activation changed after persistence. Use `effective_action` and `effective_decision_source` as current enforcement truth.
- **Unexpected block with `Review` action**: current runtime enforces `Review` as a blocked response with review-specific message text. Confirm policy intent in draft/version history before publish.
- **Scope change on Allow / Deny still appears to affect old hosts**: verify override update completed, then re-test. Updated runtime now invalidates both previous and new scope cache keys on edit; if behavior persists, inspect cache diagnostics for stale `domain:*` / `subdomain:*` entries tied to the old scope.
- **Sites stay on holding page (`ContentPending`)**: verify `classification_requests` row exists for the canonical domain key, and confirm both `classification-jobs` and `page-fetch-jobs` streams receive entries.
- **Delayed terminalization from `ContentPending`**: run `tests/content-pending-smoke.sh` and inspect generated diagnostics under `tests/artifacts/content-pending/diag-*`; validate `classification_requests.status` progression (`waiting_content` -> cleared/failed), confirm latest `page_contents.fetch_status` for the same normalized key, then check key-filtered worker logs (`diag-llm-worker-*-<host>.log`, `diag-page-fetcher-*-<host>.log`) for retry exhaustion or fetch failures before escalating.
- **One-command pending diagnostics**: run `NORMALIZED_KEY=domain:example.com HOST_TAG=local bash tests/ops/content-pending-diagnostics.sh`; inspect bundle under `tests/artifacts/ops-triage/...` for DB snapshots, queue tails, and key-filtered service logs.
- **Unified platform diagnostics bundle**: run `HOST_TAG=local REDACT=1 bash tests/ops/platform-diagnostics.sh`; bundle is written under `tests/artifacts/ops-triage/platform-*` with health, queue, auth, proxy, and reporting snapshots.
- **Traffic dashboard shows container IP instead of end-user IP**: use `client.ip` as the primary user field and `source.ip` as immediate peer fallback. Enable `OD_TRUST_PROXY_HEADERS=true` only with strict `OD_TRUSTED_PROXY_CIDRS` and ingress header overwrite; then `client.ip` resolves from `Forwarded`/`X-Forwarded-For` and `od.client_ip_source` records provenance. If browsing fails with proxy refused/`TCP_DENIED/403`, ensure the client source IP is included in `OD_SQUID_ALLOWED_CLIENT_CIDRS` and the client points to `<docker-host-lan-ip>:<OD_HAPROXY_BIND_PORT>` (not `localhost` from another machine). On Docker Desktop/macOS, source IP can be rewritten before HAProxy/Squid ACL evaluation; if HAProxy logs show `<NOSRV> ... 403`, use dev profile `OD_SQUID_ALLOWED_CLIENT_CIDRS=0.0.0.0/0` and restrict host port `3128` to LAN.
- **Diagnostics interpretation quick guide**: `*-classification_requests.txt` shows pending age/status transitions, `*-page_contents.txt` confirms Crawl4AI state (`fetch_status`/`fetch_reason`), `*-classifications.txt` confirms terminal persistence on canonical key, and stream tails (`*-classification-jobs.txt`/`*-page-fetch-jobs.txt`) expose publish gaps.
- **Platform triage mapping**: `health-*.txt` -> service readiness baseline, `redis-xinfo-*`/`redis-xpending-*` -> queue lag/consumer issues, `auth-whoami.txt` -> auth/session failures, `logs-haproxy.txt` + `logs-squid.txt` -> proxy ACL/transport faults, `reporting-*.txt` -> analytics pipeline/reporting gaps.
- **Escalation thresholds**: escalate after 2 consecutive reliability failures for the same key within 30 minutes; escalate immediately for pending rows older than 15 minutes across 3+ unrelated domains; escalate immediately if stream tails are empty while ICAP traffic is active.
- **Platform escalation thresholds**: escalate immediately if any core readiness endpoint is non-200 for >5 minutes, if `redis-xpending-*` shows growing pending with near-zero completions for two consecutive captures, if DLQ heads keep growing for 10+ minutes, if proxy logs show sustained `TCP_DENIED` across unrelated destinations, or if reporting endpoints fail/empty for two consecutive captures.
- **Reliability gate command**: `RUNS=10 bash tests/security/facebook-e2e-reliability.sh` (auto-collects failure diagnostics into `tests/artifacts/ops-triage/`).
- **403 from mutations**: verify role claims include required permissions.
- **Dashboard analytics panels empty/misaligned**: verify `/api/v1/reporting/dashboard` returns non-empty `overview`/`hourly_usage` payload for the selected range, then confirm ingest writes `network.bytes` and `client.ip` in fresh traffic events; old indices may show lower bandwidth coverage until new events arrive.
- **Dashboard time buckets appear shifted for selected range**: verify `OD_TIMEZONE` / `OD_REPORTING_TIMEZONE` settings, confirm `/api/v1/reporting/dashboard` response includes expected `timezone` and `bucket_interval`, and ensure Postgres/containers were restarted after timezone env changes.
- **Dashboard operations telemetry panel shows `unavailable`/`partial`**: verify Prometheus readiness (`curl http://localhost:9090/-/ready`) and Admin API telemetry proxy (`GET /api/v1/reporting/ops-summary`). Ensure `OD_PROMETHEUS_URL` points to reachable Prometheus from the admin-api container.
- **LLM Outcomes panel empty or `partial`**: verify `GET /api/v1/reporting/ops-llm-series?range=15m` returns provider series; if `errors[]` is populated, validate Prometheus target health for `llm-worker` and confirm counter families (`llm_provider_success_total`, `llm_provider_failures_total`, `llm_provider_timeouts_total`, `llm_provider_failure_class_total`) are present at scrape time.
- **Non-retryable HTTP 400 spike appears on dashboard**: interpret as model/provider contract failure (payload/model/schema mismatch) rather than crawl failure; correlate with `llm-worker` logs for the provider and verify failover policy (`safe` skips fallback for non-retryable classes).
- **Recurring `domain:prompt-injection.*` pending rows**: historical synthetic smoke keys can be re-enqueued by pending reconciliation. Run `tests/ops/cleanup-synthetic-pending.sh` first in dry run mode (`DRY_RUN=1` default), then execute cleanup with `DRY_RUN=0 tests/ops/cleanup-synthetic-pending.sh`. Use `PURGE_ALL_PENDING=1` only when a full queue reset is intended.
- **No CLI logs shown**: ensure admin API has `audit` access and data exists.
- **Ops provider list empty**: verify Admin API proxy endpoint `GET /api/v1/ops/llm/providers` succeeds (token-auth required) and set `OD_LLM_PROVIDERS_URL` if llm-worker is not reachable at the default `http://llm-worker:19015/providers`. Use `VITE_LLM_PROVIDERS_URL` only as an explicit frontend override.
