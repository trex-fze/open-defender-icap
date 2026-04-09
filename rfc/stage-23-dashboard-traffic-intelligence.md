# Stage 23 RFC - Dashboard Traffic Intelligence (Client-IP Centric)

**Status**: Implemented  
**Primary Scope**: `web-admin` dashboard analytics + Admin API reporting endpoint + event-ingester field enrichment  
**Decision Owner**: Platform + Frontend + SRE  
**Requester Priority**: High (operator visibility)

## 1. Problem Statement

The current dashboard is mostly static and lacks rich operational analytics. Operators need actionable visuals for:

- number of clients
- bandwidth usage
- hourly usage
- frequently accessed domains
- blocked domains
- requesters of blocked domains

The approved requester identity dimension is **`client.ip`**.

## 2. Goals

1. Deliver a rich, real-time Dashboard view using platform telemetry.
2. Add client/IP-centric analytics with clear trends and top-N breakdowns.
3. Preserve existing `/reports` behavior while adding dashboard-specific data shape.
4. Keep RBAC enforcement backend-authoritative (`ROLE_REPORTING_VIEW`).
5. Support graceful degradation when historical data lacks newer fields.

## 3. Non-Goals

- Replacing Kibana for deep SIEM workflows.
- Introducing user-account attribution as primary identity (this stage uses `client.ip`).
- Rewriting existing reports route contracts.

## 4. Key Decisions

- Requester identity for blocked requests: **`client.ip`**.
- Add additive endpoint: `GET /api/v1/reporting/dashboard`.
- Enrich ingest to persist Squid byte counter as `network.bytes`.
- Dashboard uses `recharts` for responsive visualizations.

## 5. Data Model and Telemetry Requirements

### Existing fields used

- `client.ip`
- `destination.domain`
- `http.response.status_code`
- `recommended_action` / `recommended_action_inferred`
- `@timestamp`

### New field required

- `network.bytes` (long) from Squid access log byte column.

### Compatibility

Older indices may not include `network.bytes`; API returns coverage metadata and avoids hard failure.

## 6. API Proposal

### New endpoint

`GET /api/v1/reporting/dashboard`

Query params:

- `range` (default `24h`)
- `top_n` (default `10`, max `50`)
- `bucket` (optional; auto-selected if absent)

Response sections:

- `overview`
  - `total_requests`
  - `allow_requests`
  - `blocked_requests`
  - `block_rate`
  - `unique_clients`
  - `total_bandwidth_bytes`
- `hourly_usage[]`
  - `timestamp`
  - `total_requests`
  - `blocked_requests`
  - `bandwidth_bytes`
- `top_domains[]`
- `top_blocked_domains[]`
- `top_blocked_requesters[]` (key = `client.ip`)
- `top_clients_by_bandwidth[]` (key = `client.ip`)
- `coverage`
  - field coverage for `client.ip`, `destination.domain`, `network.bytes`

Security: same role gate as existing reporting endpoints (`ROLE_REPORTING_VIEW`).

## 7. UI Proposal (Dashboard)

- KPI cards:
  - Unique Clients
  - Total Bandwidth
  - Block Rate
  - Total Requests
- Hourly usage graph (requests + blocked + bandwidth)
- Top frequently accessed domains
- Top blocked domains
- Top blocked requesters (`client.ip`)
- Top clients by bandwidth
- Data quality panel (coverage + warnings)

Controls:

- range selector (1h, 6h, 24h, 7d, 30d)
- top-N selector
- refresh trigger
- drill-link to `/reports`

## 8. Risks and Mitigations

- **Missing bytes on old data**: include coverage and fallback to zero values.
- **Docker Desktop source-IP artifacts**: display recorded `client.ip` and include runbook caveat.
- **Aggregation cost**: cap `top_n`, bounded histogram intervals, monitor endpoint latency.

## 9. Testing Requirements

- Backend unit tests for aggregation parsing and response shape.
- Hook/component tests for dashboard data states.
- Cypress dashboard flow with mocked API payloads and accessibility checks.
- Build/test gates remain mandatory.

## 10. Acceptance Criteria

- Dashboard shows all requested analytics, including requester-by-`client.ip`.
- No regressions in existing `/reports` experience.
- Endpoint and UI are role-safe and test-covered.
- Docs and runbook updated with operator verification steps.
