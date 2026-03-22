# Stage 2 Implementation Plan – Policy Engine Core

**Status**: Planned

## Objectives
- Build policy DSL, storage, evaluator, and APIs per RFC addendum.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| Task ID | Description | Owner | Dependencies | Status | Notes |
| S2-T1 | Finalize DSL syntax + grammar | Policy Architect | Stage 1 complete | ✅ | DSL defined in `crates/policy-dsl` + `config/policies.json`. |
| S2-T2 | Implement DSL parser/compiler crate | Policy Eng | S2-T1 | ✅ | `policy-dsl` crate, unit tests ensure parsing success. |
| S2-T3 | Create Postgres schema + migrations for `policies`, `policy_rules` | Backend Eng | S2-T1 | ✅ | `services/policy-engine/migrations/0001_init.sql` seeds schema. |
| S2-T4 | Extend policy-engine service with persistence + evaluator | Policy Eng | S2-T2/T3 | 🟡 | DB-backed evaluator, reload/list/create APIs implemented; pending production DB plumbing + CLI tooling. |
| S2-T5 | Implement policy CRUD + simulation APIs with auth | Backend Eng | S2-T4 | 🟡 | Policy engine now enforces role-based access (static tokens), stores versions/audit entries, and exposes `PUT /api/v1/policies/:id` plus CLI `policy import/update`; HS256/JWT support + deploy approvals still pending. |
| S2-T6 | Add audit logging for policy changes | Security Eng | S2-T5 | ⬜ | Write to Postgres + Elasticsearch. |
| S2-T7 | Document and expose `/api/v1/policies` for UI/CLI | Tech Writer | S2-T4 | ✅ | Updated architecture + user guide referencing endpoints. |
| S2-T8 | CLI/UX tooling for policy reload/list | DevTools Eng | S2-T4 | ⬜ | Add `odctl policy list/reload`. |

## Evidence Plan
- DSL spec doc, migration logs, OpenAPI schema, CI test reports.
