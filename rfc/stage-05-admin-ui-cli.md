# Stage 5 RFC Addendum – Admin API, UI & CLI

**Parent Sections**: `docs/engine-adaptor-spec.md` §§13, 14, 18, 19, 23.

## Objectives
1. Expand Admin API with policy, override, review, reporting, cache inspection endpoints.
2. Build React admin UI routes (dashboards, investigations, policy mgmt, review queue, health).
3. Implement CLI (`odctl`) commands for env validation, policy/override import-export, cache ops, smoke tests.
4. Enforce RBAC/SSO (OIDC) across API/UI/CLI.

## Checklist
- [ ] Admin API endpoints per Spec §23 (policy decision support, override CRUD, reporting queries).
- [ ] React UI navigation + role-aware views per Spec §18.
- [ ] CLI command tree per Spec §19 with table/json output.
- [ ] Auth integration (OIDC flows, token storage) – Spec §§18–19.
- [ ] Unit + e2e tests (React Testing Library, Cypress, CLI integration) – Spec §25.
- [ ] Accessibility + UX guidelines (fonts, gradients, responsive) – Spec §18 instructions.

## Traceability Plan
| Requirement | Section | Artifact |
| --- | --- | --- |
| Dashboards & investigations | Spec §18 | React pages, screenshot evidence |
| CLI management | Spec §19 | `odctl` modules, smoke logs |
| RBAC enforcement | Spec §14, §18 | Auth middleware, tests |

## Pending Decisions
- Choose component library (EUI, custom) for charts/tables.
- Finalize CLI auth model (API token vs OIDC device flow).
