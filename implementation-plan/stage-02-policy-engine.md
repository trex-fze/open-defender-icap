# Stage 2 Implementation Plan – Policy Engine Core

**Status**: Planned

## Objectives
- Build policy DSL, storage, evaluator, and APIs per RFC addendum.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| S2-T1 | Finalize DSL syntax + grammar | Policy Architect | Stage 1 complete | ✅ | DSL defined in `crates/policy-dsl` + `config/policies.json` |
| S2-T2 | Implement DSL parser/compiler crate | Policy Eng | S2-T1 | ✅ | `policy-dsl` crate with unit tests |
| S2-T3 | Create Postgres schema + migrations for `policies`, `policy_rules` | Backend Eng | S2-T1 | ⬜ | Migration evidence required |
| S2-T4 | Extend policy-engine service with persistence + evaluator | Policy Eng | S2-T2/T3 | 🟡 | In-memory evaluator + reload/list APIs complete; DB persistence pending |
| S2-T5 | Implement policy CRUD + simulation APIs with auth | Backend Eng | S2-T4 | ⬜ | OpenAPI + auth tests |
| S2-T6 | Add audit logging for policy changes | Security Eng | S2-T5 | ⬜ | Event samples |

## Evidence Plan
- DSL spec doc, migration logs, OpenAPI schema, CI test reports.
