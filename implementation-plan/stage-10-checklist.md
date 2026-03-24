# Stage 10 Frontend Management Parity Checklist

This checklist tracks implementation progress for Stage 10 and maps directly to `rfc/stage-10-frontend-management-parity.md` and `implementation-plan/stage-10-frontend-management-parity.md`.

## Phase A - Foundation and Auth Session Hardening

- [x] Add Stage 10 RFC and implementation plan documents.
- [x] Add a dedicated execution checklist for Stage 10.
- [x] Wire `/policies/new` route to remove dead navigation path from Policies page.
- [x] Add a shared Admin API client helper for typed requests and consistent error handling.
- [ ] Move existing hooks to React Query with normalized cache keys.
- [ ] Harden auth session lifecycle (token expiry UX, refresh flow, forced re-auth handling).

## Phase B - Core Management CRUD Parity

- [x] Implement policy draft creation flow in UI (`/policies/new`).
- [x] Implement policy publish action in policy detail page.
- [x] Implement review resolve actions from Review Queue page.
- [x] Implement override create/edit/delete flows.
- [x] Implement taxonomy category/subcategory live CRUD.

## Phase C - Diagnostics and Investigations Parity

- [x] Keep pending classifications workflow fully live and role-safe.
- [x] Add page content inspector (show + history).
- [x] Add cache key diagnostics (lookup + delete).
- [x] Add CLI logs/audit viewer.

## Phase D - Reporting and Operations Parity

- [x] Surface reporting traffic endpoint with range/top filters.
- [x] Add operations status panel (worker/provider health and key metrics).
- [x] Add environment-aware fallback behavior that is explicit in UI state.

## Phase E - Quality and Rollout

- [x] Add/expand unit tests for new hooks/pages.
- [x] Add/expand Cypress flows for policy/review/diagnostics parity.
- [x] Add accessibility pass for forms, dialogs, and action tables.
- [ ] Update operator/user docs with screenshots and runbook steps.

## Acceptance Tracking

- [ ] Every currently exposed Admin API management route has a corresponding UI workflow.
- [ ] No placeholder management action remains in production routes.
- [ ] Role-based nav/action availability aligns with backend-enforced permissions.
- [ ] Web admin test suite (unit + e2e) and integration smoke remain green.
