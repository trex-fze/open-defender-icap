# Stage 3 Implementation Plan – Persistence & Overrides

**Status**: Planned

## Objectives
- Implement Postgres persistence, overrides, review workflows, audit logging.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status |
| --- | --- | --- | --- | --- |
| S3-T1 | Design DB schema per Spec §20 | Backend Architect | Stage 2 DB tooling | ⏳ (override/review/classification tables defined; remaining audit artifacts pending) |
| S3-T2 | Implement migrations + tooling | Backend Eng | S3-T1 | ⏳ (sqlx migrations + env wiring merged) |
| S3-T3 | Build override API/service + caching hooks | Backend Eng | S3-T2 | ✅ (override CRUD + validation + Redis cache invalidation live; adaptor + workers subscribe to cache events) |
| S3-T4 | Implement review queue + SLA metrics | SOC Eng | S3-T2 | ✅ (review endpoints emit Prometheus metrics + SLA counters) |
| S3-T5 | Audit trail writer (DB + ES) | Security Eng | S3-T2 | ✅ (audit_events table + optional Elasticsearch exporter) |
| S3-T6 | Evidence capture (audit samples, API tests) | QA | S3-T3/T5 | ✅ (override validation tests + `/metrics` coverage captured via `cargo test -p admin-api`) |
| S3-T7 | Unit/integration tests for overrides + review persistence | QA | S3-T3/T4 | ✅ (admin API validator tests + cache invalidation tests in icap adaptor) |
