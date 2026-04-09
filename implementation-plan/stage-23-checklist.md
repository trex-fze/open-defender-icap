# Stage 23 Checklist - Dashboard Traffic Intelligence

## Inventory and Design
- [x] Confirm dashboard metric contract with operator requirements.
- [x] Finalize response schema for `/api/v1/reporting/dashboard`.
- [x] Finalize chart panel layout and mobile behavior.

## Telemetry and Mapping
- [x] Parse Squid byte column into `network.bytes` in ingester.
- [x] Update ES index template mapping for `network.bytes`.
- [x] Verify ingest still writes client/domain/action fields.

## Backend API
- [x] Add `GET /api/v1/reporting/dashboard` route.
- [x] Implement reporting handler and query params.
- [x] Implement ES aggregations for overview/trends/top-N/requesters.
- [x] Add coverage section for field presence.

## Frontend Dashboard
- [x] Add `useDashboardReportData` hook.
- [x] Add query keys for dashboard range/top-N/bucket.
- [x] Add KPI cards from live data.
- [x] Add hourly usage graph.
- [x] Add top frequently accessed domains panel.
- [x] Add top blocked domains panel.
- [x] Add top blocked requesters (`client.ip`) panel.
- [x] Add top clients by bandwidth panel.
- [x] Add range/top-N/refresh controls.
- [x] Remove static mock KPI dependency from dashboard.

## Testing
- [x] Add backend reporting parser/contract tests.
- [x] Add hook/component tests for dashboard states.
- [x] Add Cypress dashboard analytics flow.
- [x] Run `cargo test -p admin-api --quiet`.
- [x] Run `npm test` and `npm run build` in `web-admin`.

## Docs and Tracking
- [x] Update `docs/api-catalog.md`.
- [x] Update `docs/user-guide.md`.
- [x] Update `docs/runbooks/stage10-web-admin-operator-runbook.md`.
- [x] Add Stage 23 row in `rfc/stage-plan.md`.
- [x] Add Stage 23 row in `implementation-plan/stage-plan.md`.
