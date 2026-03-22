# Stage 3 RFC Addendum – Persistence & Overrides

**Parent Sections**: `docs/engine-adaptor-spec.md` §§8, 9, 11, 14, 20, 21.

## Objectives
1. Design and migrate Postgres schema for classifications, overrides, review queue, audits.
2. Implement override/review workflows with API + UI hooks.
3. Ensure cache invalidation/versioning rules tied to taxonomy/model versions.
4. Introduce audit logging + evidence capture for policy/override actions.

## Checklist
- [ ] Schema + migrations for entities listed in Spec §20 (`classifications`, `overrides`, `review_queue`, etc.). *(Overrides + review_queue tables landed; classifications/audit artifacts still open.)*
- [x] Override APIs (create/update/delete, scope validation) – Spec §23.4. *(Admin API now validates scopes/actions, supports PUT + DELETE, and emits cache invalidations; CLI/UI follow-ups pending.)*
- [ ] Review queue endpoints & SLA metrics – Spec §§14, 16. *(List + resolve endpoints wired; SLA metrics + worker hooks TBD.)*
- [x] Cache invalidation hooks on DB changes – Spec §11. *(Admin API purges Redis keys + publishes events; ICAP adaptor subscribes and clears local caches.)*
- [x] Audit event pipeline (DB + Elasticsearch) – Spec §17. *(Admin API now writes audit_events table; ES hookup pending.)*
- [ ] Unit/integration tests for persistence logic – Spec §24–26.

## Traceability Plan
| Requirement | Section | Artifact |
| --- | --- | --- |
| Manual overrides | Spec §14 | Override service, audit trail |
| Reclassification queue | Spec §11, §16 | Worker jobs, schema |
| Evidence retention | Spec §20, §29 | Migration logs, audit records |

## Pending Decisions
- Choose migration tooling (sqlx vs refinery).
- Finalize retention policies (90d hot vs cold storage).
