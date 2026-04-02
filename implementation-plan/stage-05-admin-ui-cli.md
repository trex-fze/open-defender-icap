# Stage 5 Implementation Plan – Admin API, UI & CLI

**Status**: Planned

## Objectives
- Expose admin APIs, React UI, and CLI per Spec §§18–19.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status |
| --- | --- | --- | --- | --- |
| S5-T1 | Finalize API contracts + auth model | Backend Architect | Stage 3 data | ✅ | RFC now documents endpoints, RBAC, token flows |
| S5-T2 | Implement admin API routes (policies, overrides, reports) | Backend Eng | S5-T1 | ✅ | Added policy/taxonomy/reporting/cache/cli log endpoints + pagination |
| S5-T3 | Build React navigation + pages | Frontend Eng | S5-T1 | ✅ | Shell + role-aware routing, live Admin API hooks landed |
| S5-T4 | Add CLI commands + test harness | DevTools Eng | S5-T2 | ✅ | odctl rebuilt on clap with policy/override/report/cache flows; `cargo test -p odctl` green |
| S5-T5 | RBAC enforcement across API/UI/CLI | Security Eng | S5-T2/T3 | ✅ | Admin API validates HS256 JWTs, React AuthProvider persists tokens, odctl ships OIDC device flow + refresh |
| S5-T6 | UI/CLI e2e + accessibility tests | QA | S5-T3/T4 | ✅ | Vitest + odctl integration tests green; Cypress suite runs login/dashboard/investigations/policies/overrides/reports with axe serious+critical. Scrollable tables now focusable w/ WCAG AA contrast tokens |
