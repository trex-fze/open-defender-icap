# Stage 23 Implementation Plan - Dashboard Traffic Intelligence

**Status**: Complete  
**Depends on**: Stage 6/10/22 reporting and frontend parity foundations  
**Execution checklist**: `implementation-plan/stage-23-checklist.md`

## 1) Delivery Strategy

Implement in five phases:

- Phase A: telemetry field readiness
- Phase B: backend dashboard endpoint
- Phase C: frontend hook + dashboard UI charts
- Phase D: test hardening
- Phase E: docs/runbook and rollout verification

## 2) Work Breakdown

| Task ID | Description | Area | Dependencies | Output |
| --- | --- | --- | --- | --- |
| S23-T1 | Persist Squid bytes as `network.bytes` in event-ingester | Ingest | none | enriched indexed events |
| S23-T2 | Extend ES template mapping with `network.bytes: long` | Elastic | S23-T1 | mapping compatibility |
| S23-T3 | Add `GET /api/v1/reporting/dashboard` route + handler | Admin API | S23-T1/S23-T2 | new API endpoint |
| S23-T4 | Implement dashboard ES aggregations and response model | Admin API | S23-T3 | overview + trends + top-N sets |
| S23-T5 | Add frontend hook `useDashboardReportData` + query keys | Web Admin | S23-T3/S23-T4 | typed query adapter |
| S23-T6 | Replace dashboard mock KPIs with live data cards | Web Admin | S23-T5 | live command deck KPIs |
| S23-T7 | Add charts/tables for hourly usage, domains, blocked requesters (`client.ip`) | Web Admin | S23-T6 | rich analytics panels |
| S23-T8 | Add dashboard filters (range, top-N, refresh) | Web Admin | S23-T7 | interactive analytics |
| S23-T9 | Add backend tests for reporting dashboard parser/contract | QA Backend | S23-T4 | reliable API behavior |
| S23-T10 | Add frontend unit tests + Cypress dashboard e2e | QA Frontend | S23-T8 | UI confidence |
| S23-T11 | Update API catalog, user guide, and runbook | Docs | S23-T10 | operator-ready docs |

## 3) Phase Plan

### Phase A - Telemetry Readiness

- Files:
  - `services/event-ingester/src/elastic.rs`
  - `deploy/elastic/index-template.json`
- Exit criteria:
  - `network.bytes` populated on new events
  - no ingest regression

### Phase B - API Surface

- Files:
  - `services/admin-api/src/main.rs`
  - `services/admin-api/src/reporting.rs`
  - `services/admin-api/src/reporting_es.rs`
- Exit criteria:
  - `/api/v1/reporting/dashboard` returns full response shape
  - role checks enforce reporting access

### Phase C - Dashboard UX

- Files:
  - `web-admin/src/hooks/useDashboardReportData.ts` (new)
  - `web-admin/src/hooks/queryKeys.ts`
  - `web-admin/src/pages/DashboardPage.tsx`
  - `web-admin/src/components/dashboard/*` (new)
  - `web-admin/src/styles.css`
  - `web-admin/package.json` (`recharts`)
- Exit criteria:
  - requested analytics visible and responsive on desktop/mobile
  - requester breakdown uses `client.ip`

### Phase D - Validation

- Backend: `cargo test -p admin-api --quiet`
- Frontend: `npm test`, `npm run build`
- Cypress: dashboard analytics + accessibility

### Phase E - Documentation and Rollout

- Files:
  - `docs/api-catalog.md`
  - `docs/user-guide.md`
  - `docs/runbooks/stage10-web-admin-operator-runbook.md`
- Exit criteria:
  - runbook includes dashboard verification steps
  - field coverage caveats documented

## 4) Acceptance Tracking

- [ ] Dashboard includes clients, bandwidth, hourly usage, top domains, blocked domains, blocked requesters (`client.ip`).
- [ ] Endpoint latency remains acceptable for default range.
- [ ] Existing `/reports` behavior remains unchanged.
- [ ] Unit/e2e tests pass.
- [ ] Docs/runbook updated and reviewed.
