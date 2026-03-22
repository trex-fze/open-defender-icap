# Stage 2 RFC Addendum – Policy Engine Core

**Parent Sections**: `docs/engine-adaptor-spec.md` §§3, 4, 14, 20, 21, 23.

## Objectives
1. Implement policy DSL parser/compiler with precedence rules (✅ initial DSL delivered via `crates/policy-dsl`).
2. Persist policies/rules in Postgres with migrations and hot-reload.
3. Expose authenticated REST APIs for policy CRUD, versioning, simulations.
4. Enforce RBAC, audit logging, and versioned policy deployment.

## Checklist
- [x] DSL grammar & parser aligned with Spec §14 evaluation order (see `crates/policy-dsl`, `config/policies.json`).
- [x] Policy storage schema & migrations created (Spec §20 entities `policies`, `policy_rules`).
- [x] Decision API enriched with policy metadata (policy listing + reload/create endpoints).
- [x] Auth scaffolding via admin token + middleware (Spec §14, §18) – HS256 JWT/RBAC guard now live on both Admin API and Policy Engine endpoints.
- [x] Policy simulation endpoint returning decision + matched rule (Spec §14 auditability) – trace/audit logging TBD.
- [ ] Unit/integration tests covering precedence, overrides, error paths (Spec §24–26).
- [x] Audit logging for policy changes (Spec §17). *(Policy versions recorded in `policy_versions` table via Admin API + CLI workflows.)*

## Data Model Snapshot (Planned)
- `policies`: `id (uuid)`, `name`, `version`, `status`, `created_by`, `created_at`, `updated_at`.
- `policy_rules`: `policy_id`, `rule_id`, `priority`, `action`, `conditions (jsonb)`, `description`.
- `policy_versions`: history table capturing deployments + approvals.
Migrations will reside in `services/policy-engine/migrations/` and run via `odctl migrate run`.

## API Additions (Planned)
- `GET /api/v1/policies` – returns version + summaries (implemented).
- `POST /api/v1/policies/reload` – reload DSL file (implemented).
- `POST /api/v1/policies` – create new policy (DB-backed) with draft status.
- `PUT /api/v1/policies/{id}` – update policy metadata or attach rule set.
- `POST /api/v1/policies/{id}/deploy` – promote version, optionally run simulation and capture audit record.
- `POST /api/v1/policies/simulate` – accept metadata sample, return rule trace.

## Security & RBAC
- Roles: `policy-admin`, `policy-editor`, `policy-viewer`, `auditor`.
- Enforce via OIDC scopes; CLI uses client credentials.
- Audit every policy change and reload event with actor, diff summary, reason.

## Open Questions
- Should DB be single-tenant or multi-tenant (per branch/organization)?
- How to merge file-based DSL with DB-managed policies (initial plan: DB authoritative, DSL used for bootstrap/tests).
- Simulation output format (JSON trace vs text) and retention.

This addendum will track completion of the remaining checklist items before Stage 2 is marked complete.

## Traceability Plan
| Requirement | Source Section | Implementation Artifact |
| --- | --- | --- |
| Policy precedence | Spec §14 | `policy-dsl` crate, evaluator tests |
| Policy CRUD APIs | Spec §20, §23 | Axum handlers, OpenAPI docs |
| Auditing | Spec §17, §20 | Audit event writer, DB schema |

## Open Questions
- Need final DSL syntax (JSON vs custom) confirmation.
- Decide on simulation interface format (JSON vs CLI output).

This document will be updated before coding Stage 2 tasks.
