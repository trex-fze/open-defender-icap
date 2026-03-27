# Stage-by-Stage Implementation Plan

This roadmap mirrors the RFC tracker and captures actionable work items, owners, dependencies, and evidence for each stage.

| Stage | Major Work Items | Owner Role | Dependencies | Evidence of Completion |
| --- | --- | --- | --- | --- |
| 0. Foundation (✅) | Workspace scaffold, config loader, placeholder binaries | Lead Engineer | None | `cargo check`, repo layout, config samples |
| 1. ICAP Hot Path (✅) | ICAP parser/normalizer, policy client, Redis cache, ICAP responses | Secure Web Gateway team | RFC Stage 1 | `cargo test -p icap-adaptor`, Redis metrics, ICAP response compliance fixes |
| 2. Policy Engine Core (✅) | DSL parser, persistence, RBAC, policy simulation, OpenAPI | Policy Engine team | Stage 1 partly complete | Policy unit/integration tests, readiness report, activation-aware decisions |
| 3. Persistence & Overrides (✅) | Postgres schema, migrations, overrides/review APIs, audit trail | Backend team | Stage 2 | Migration logs, override API tests, audit samples |
| 4. Async Classification (✅) | Redis Streams, LLM worker, reclass worker, queue monitoring | Classification team | Stage 3 | Worker integration tests, canonical prompt enforcement, strict content-gating evidence |
| 5. Admin UI/CLI (✅) | React dashboards, CLI commands, AuthN/Z, report builder | Frontend + DevTools | Stage 3 | UI e2e tests, CLI smoke logs, accessibility report |
| 6. Reporting/Observability (✅) | ES ingestion, Kibana dashboards, Prometheus metrics, alerts | SRE + SOC | Stages 1–5 | Dashboard screenshots, alert configs, log samples |
| 7. Testing & Ops (✅) | Full test suites, docker-compose QA env, deployment runbooks, evidence package | QA + DevOps + PMO | All prior stages | Test reports, smoke artifacts, runbook evidence |
| 10. Frontend Management Parity (✅) | Management UI parity for policy/override/review/taxonomy/reporting/diagnostics and RBAC UX hardening | Frontend + Platform | Stages 5, 6, 7, 9 | Stage 10 RFC/plan, web-admin e2e evidence, operator workflow docs |
| 11. RBAC and User/Group Management (✅) | IAM schema, effective-role auth resolver, users/groups/roles/service-accounts APIs, UI/CLI lifecycle flows | Platform Security + Backend + Frontend + DevTools | Stages 5, 10 | Stage 11 RFC/plan/checklist, authz matrix evidence, migration runbook |

## Current Focus

1. Keep strict content-aware behavior stable in production-like smoke runs (`tests/security/facebook-e2e-smoke.sh`).
2. Maintain canonical taxonomy enforcement paths (prompt contract, alias mapping, persistence validation).
3. Improve operator diagnostics and runbook automation around pending classifications and Crawl4AI health.
