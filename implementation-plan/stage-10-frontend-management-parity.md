# Stage 10 Implementation Plan - Frontend Management Parity

**Status**: Planned  
**Depends on**: Stage 5/6/7/9 delivered APIs and compose integration  
**Primary Targets**: `web-admin/`, plus optional Admin API aggregation endpoints where required

## 1) Delivery Strategy

Implement in five phases to reduce risk and keep a continuously usable UI:

- Phase A: foundation and auth/session hardening
- Phase B: core management CRUD parity
- Phase C: diagnostics parity (cache/page/audit/pending)
- Phase D: reporting plus operations visibility parity
- Phase E: hardening, tests, docs, rollout

## 2) Work Breakdown

| Task ID | Description | Area | Dependencies | Output |
| --- | --- | --- | --- | --- |
| S10-T1 | Build typed API client layer plus shared error envelope handling | FE Core | none | `api/` module with request/response adapters |
| S10-T2 | Introduce query/mutation state standardization (React Query) | FE Core | S10-T1 | replace ad-hoc fetch hooks |
| S10-T3 | Auth session hardening (token expiry, refresh hooks, logout hygiene) | FE Auth | S10-T1 | robust `AuthContext` and auth guards |
| S10-T4 | Fix route inconsistencies and placeholders (for example `/policies/new`) | FE Routing | S10-T1 | clean route map and nav gating |
| S10-T5 | Policy create/update/validate/publish flows | Policies | S10-T2 | full policy lifecycle UI |
| S10-T6 | Override CRUD plus filters/search | Overrides | S10-T2 | create/edit/delete workflows |
| S10-T7 | Review queue resolve actions plus notes and filters | Review | S10-T2 | resolve workflows |
| S10-T8 | Taxonomy category/subcategory CRUD | Taxonomy | S10-T2 | live taxonomy management |
| S10-T9 | Pending queue hardening plus richer actions UX | Pending | S10-T2 | robust unblock/retry experience |
| S10-T10 | Page content inspector (latest plus history) | Diagnostics | S10-T2 | new diagnostics page |
| S10-T11 | Cache entry lookup plus evict controls | Diagnostics | S10-T2 | role-gated cache page |
| S10-T12 | CLI logs/audit viewer with filters | Audit | S10-T2 | auditor/admin visibility page |
| S10-T13 | Reporting traffic view plus filter controls | Reporting | S10-T2 | `/reports` parity for traffic route |
| S10-T14 | Operations status panel (workers/providers/health) | Ops | S10-T2 (+ optional backend agg) | dashboard ops cards |
| S10-T15 | Accessibility pass and keyboard action support | QA/UX | S10-T5..T14 | WCAG-focused improvements |
| S10-T16 | E2E and integration test expansion | QA | S10-T5..T15 | Cypress scenarios for all management flows |
| S10-T17 | Documentation updates and operator runbook refresh | Docs | S10-T16 | updated `docs/user-guide.md` and frontend guide |

## 3) Phase Plan

### Phase A - Foundation (Sprint 1)

- Scope: S10-T1, S10-T2, S10-T3, S10-T4
- Exit criteria:
  - unified API client in place
  - consistent loading/error states
  - no broken routes/placeholders without guardrails

### Phase B - Core CRUD Parity (Sprint 2)

- Scope: S10-T5, S10-T6, S10-T7, S10-T8
- Exit criteria:
  - policy/override/review/taxonomy fully manageable via UI
  - mutation auditability observable through backend data

### Phase C - Diagnostics Parity (Sprint 3)

- Scope: S10-T9, S10-T10, S10-T11, S10-T12
- Exit criteria:
  - SOC can investigate pending/classification context end-to-end in UI

### Phase D - Reporting/Ops Parity (Sprint 4)

- Scope: S10-T13, S10-T14
- Exit criteria:
  - dashboard and reports cover aggregate + traffic + service posture

### Phase E - Hardening (Sprint 5)

- Scope: S10-T15, S10-T16, S10-T17
- Exit criteria:
  - expanded automated coverage
  - docs and runbooks reflect real workflows

## 4) API Mapping Checklist (UI Parity)

- [ ] `GET/POST/PUT /api/v1/policies`, `POST /api/v1/policies/validate`, `POST /api/v1/policies/:id/publish`
- [ ] `GET/POST/PUT/DELETE /api/v1/overrides`
- [ ] `GET /api/v1/review-queue`, `POST /api/v1/review-queue/:id/resolve`
- [ ] `GET/POST/PUT/DELETE /api/v1/taxonomy/categories`
- [ ] `GET/POST/PUT/DELETE /api/v1/taxonomy/subcategories`
- [ ] `GET /api/v1/classifications/pending`, `POST /api/v1/classifications/:normalized_key/unblock`
- [ ] `GET /api/v1/page-contents/:normalized_key`, `GET /api/v1/page-contents/:normalized_key/history`
- [ ] `GET/DELETE /api/v1/cache-entries/:cache_key`
- [ ] `GET /api/v1/cli-logs`
- [ ] `GET /api/v1/reporting/aggregates`, `GET /api/v1/reporting/traffic`

## 5) Test Plan

### Unit/Component

- Hook contract tests for each route adapter.
- Mutation tests for optimistic + rollback behavior.
- Auth guard and role matrix tests.

### End-to-End

- Policy draft create/edit/validate/publish.
- Override create/update/delete.
- Review resolve.
- Taxonomy create/update/delete.
- Pending unblock.
- Page content lookup + history.
- Cache inspect + delete (admin role).
- CLI logs read (auditor/admin).
- Reporting aggregate + traffic filters.

### Smoke

- Keep existing integration suite.
- Add a web-admin smoke subset for critical management operations.

## 6) Rollout and Safety

- Feature flags per module (policy editor, diagnostics pages, ops panel).
- Staged deployment:
  1. read-only views first
  2. mutations for editor/admin roles
  3. ops dashboard enhancements
- Maintain CLI fallback runbook for any temporarily disabled UI mutations.

## 7) Definition of Done

- Feature parity achieved for all current Admin API management capabilities.
- Role enforcement verified in both UI behavior and backend responses.
- No major placeholder action remains in production routes.
- Documentation and runbooks updated with screenshots and operator workflows.
