# Stage-by-Stage Implementation Plan

This roadmap mirrors the RFC tracker and captures actionable work items, owners, dependencies, and evidence for each stage.

| Stage | Major Work Items | Owner Role | Dependencies | Evidence of Completion |
| --- | --- | --- | --- | --- |
| 0. Foundation (✅) | Workspace scaffold, config loader, placeholder binaries | Lead Engineer | None | `cargo check`, repo layout, config samples |
| 1. ICAP Hot Path (🟡) | ICAP parser/normalizer, policy client, Redis cache, ICAP responses | Secure Web Gateway team | RFC Stage 1 | `cargo test -p icap-adaptor`, Redis metrics, Stage 1 RFC addendum |
| 2. Policy Engine Core (🟡) | DSL parser, persistence, RBAC, policy simulation, OpenAPI | Policy Engine team | Stage 1 partly complete | Policy unit/integration tests, OpenAPI doc, `svc-policy` readiness report |
| 3. Persistence & Overrides (⬜) | Postgres schema, migrations, overrides/review APIs, audit trail | Backend team | Stage 2 | Migration logs, override API tests, audit samples |
| 4. Async Classification (⬜) | Redis Streams, LLM worker, reclass worker, queue monitoring | Classification team | Stage 3 | Worker integration tests, prompt validation evidence, LLM cost metrics |
| 5. Admin UI/CLI (⬜) | React dashboards, CLI commands, AuthN/Z, report builder | Frontend + DevTools | Stage 3 | UI e2e tests, CLI smoke logs, accessibility report |
| 6. Reporting/Observability (⬜) | ES ingestion, Kibana dashboards, Prometheus metrics, alerts | SRE + SOC | Stages 1–5 | Dashboard screenshots, alert configs, log samples |
| 7. Testing & Ops (⬜) | Full test suites, docker-compose QA env, deployment runbooks, evidence package | QA + DevOps + PMO | All prior stages | Test reports, runbook PDFs, signoff forms |
| 10. Frontend Management Parity (⬜) | Management UI parity for policy/override/review/taxonomy/reporting/diagnostics and RBAC UX hardening | Frontend + Platform | Stages 5, 6, 7, 9 | Stage 10 RFC/plan, web-admin e2e evidence, operator workflow docs |

## Stage 1 Task Breakdown (Current)
1. **Redis cache resilience** – add retry/backoff, health metrics, and configuration validation.
2. **Policy request enrichment** – include user/group/IP metadata from Squid headers per Spec §§14 & 20.
3. **ICAP response enhancements** – add coaching page templates and preview support.
4. **Tracing & logging** – propagate `trace_id` from Squid headers into policy calls and logs.

Progress on these tasks will be logged alongside code commits and referenced against the RFC addendum.
