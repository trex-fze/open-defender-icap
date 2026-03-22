# Stage 2 RFC Addendum – Policy Engine Core

**Parent Sections**: `docs/engine-adaptor-spec.md` §§3, 4, 14, 20, 21, 23.

## Objectives
1. Implement policy DSL parser/compiler with precedence rules.
2. Persist policies, rules, overrides in Postgres with migrations.
3. Expose authenticated gRPC/REST APIs for decisions, policy CRUD, simulations.
4. Enforce RBAC, audit logging, and versioned policy deployment.

## Checklist
- [x] DSL grammar & parser aligned with Spec §14 evaluation order (see `crates/policy-dsl`, `config/policies.json`).
- [ ] Policy storage schema & migrations created (Spec §20 entities `policies`, `policy_rules`).
- [x] Decision API enriched with policy IDs/version metadata (Policy listing + reload endpoints).
- [ ] AuthN/Z via OIDC/mTLS with role-based scopes (Spec §14, §18 frontend).
- [ ] Policy simulation endpoint w/ trace outputs for audit (Spec §14 auditability).
- [ ] Unit/integration tests covering precedence, overrides, error paths (Spec §24–26).

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
