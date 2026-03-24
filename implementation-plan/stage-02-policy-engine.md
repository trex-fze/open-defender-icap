# Stage 2 Implementation Plan – Policy Engine Core

**Status**: Complete

## Objectives
- Build policy DSL, storage, evaluator, and APIs per RFC addendum.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| Task ID | Description | Owner | Dependencies | Status | Notes |
| S2-T1 | Finalize DSL syntax + grammar | Policy Architect | Stage 1 complete | ✅ | DSL defined in `crates/policy-dsl` + `config/policies.json`. |
| S2-T2 | Implement DSL parser/compiler crate | Policy Eng | S2-T1 | ✅ | `policy-dsl` crate, unit tests ensure parsing success. |
| S2-T3 | Create Postgres schema + migrations for `policies`, `policy_rules` | Backend Eng | S2-T1 | ✅ | `services/policy-engine/migrations/0001_init.sql` seeds schema. |
| S2-T4 | Extend policy-engine service with persistence + evaluator | Policy Eng | S2-T2/T3 | ✅ | DB-backed evaluator fully wired; reload/list/create APIs persist to Postgres and drive the CLI. |
| S2-T5 | Implement policy CRUD + simulation APIs with auth | Backend Eng | S2-T4 | ✅ | Policy engine now enforces HS256/static RBAC, records versions, and exposes `PUT /api/v1/policies/:id` plus CLI `policy import/update`. Deploy approvals will be delivered in a later stage. |
| S2-T6 | Add audit logging for policy changes | Security Eng | S2-T5 | ✅ | `policy_audit_events` table + service logger capture create/update/reload events (Stage 4 will add ES export/approvals). |
| S2-T7 | Document and expose `/api/v1/policies` for UI/CLI | Tech Writer | S2-T4 | ✅ | Updated architecture + user guide referencing endpoints. |
| S2-T8 | CLI/UX tooling for policy reload/list | DevTools Eng | S2-T4 | ✅ | `odctl policy list/reload/import/update` all available. |
| S2-T9 | Unit/integration tests (precedence, overrides, error paths) | QA | S2-T4/S2-T5 | ✅ | `cargo test -p policy-engine` coverage added (decision + protection tests). |

## Evidence Plan
- DSL spec doc, migration logs, OpenAPI schema, CI test reports.
