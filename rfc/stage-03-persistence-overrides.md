# Stage 3 RFC Addendum – Persistence & Overrides

**Parent Sections**: `docs/engine-adaptor-spec.md` §§8, 9, 11, 14, 20, 21.

## Objectives
1. Design and migrate Postgres schema for classifications, overrides, audits.
2. Implement override/review workflows with API + UI hooks.
3. Ensure cache invalidation/versioning rules tied to taxonomy/model versions.
4. Introduce audit logging + evidence capture for policy/override actions.

## Checklist
- [x] Schema + migrations for entities listed in Spec §20 (`classifications`, `overrides`, etc.). *(Added taxonomy tables, cache entries, reclassification jobs, CLI/UI audit logs, reporting aggregates.)*
- [x] Override APIs (create/update/delete, scope validation) – Spec §23.4. *(Admin API validates scopes/actions, supports PUT + DELETE, emits cache invalidations, and Stage 10/11 UI+CLI flows are complete.)*
- [x] Review queue endpoints & SLA metrics – Spec §§14, 16. *(List + resolve endpoints wired; Prometheus `/metrics` exposes queue depth + SLA counters.)*
- [x] Cache invalidation hooks on DB changes – Spec §11. *(Admin API purges Redis keys + publishes events; ICAP adaptor and workers subscribe and react.)*
- [x] Audit event pipeline (DB + Elasticsearch) – Spec §17. *(Admin API writes audit_events and streams to Elasticsearch when configured.)*
- [x] Unit/integration tests for persistence logic – Spec §24–26. *(Completed via Stage 3 implementation/test evidence in `implementation-plan/stage-03-persistence-overrides.md`.)*

## Traceability Plan
| Requirement | Section | Artifact |
| --- | --- | --- |
| Manual overrides | Spec §14 | Override service, audit trail |
| Reclassification queue | Spec §11, §16 | Worker jobs, schema |
| Evidence retention | Spec §20, §29 | Migration logs, audit records |

## Resolved Decisions
- Migration tooling: SQLx migrations are the standard for services in this repo.
- Retention policy: hot retention and lifecycle controls are enforced through checked-in index/ILM configs and stage hardening runbooks.
