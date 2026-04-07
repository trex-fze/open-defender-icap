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
| 13. Domain-First Classification Scope (✅) | Canonical domain-key dedupe across pending, page fetch, and classification persistence while preserving subdomain override matching | Platform + Backend | Stages 2, 4, 9 | Stage 13 RFC/plan, migration 0015, ICAP smoke evidence (`www` + `api` -> single domain key) |
| 14. Pending Hardening & Output-Invalid Fallback (✅) | Persist terminal crawl failures, avoid metadata requeue loops, online verification for local output-invalid errors, terminal insufficient-evidence fallback | Classification + Backend + SRE | Stages 4, 9, 13 | Stage 14 RFC/plan, llm-worker/page-fetcher test runs, runtime pending-loop reduction evidence |
| 15. Cursor Pagination Hard Cutover (✅) | Replace mixed list responses with cursor pagination (`limit` + `cursor`, `{data, meta}`), migrate web-admin + odctl, add keyset indexes | Backend + Frontend + DevTools | Stages 5, 10, 11, 14 | Stage 15 RFC/plan, migration 0016, cargo/web test evidence, docker runtime cursor-chain smoke |
| 16. Policy Action Outcome Hardening (✅) | Action semantics hardening (`Review`/`ContentPending`), strict condition validation, activation parity, runtime-parity simulation | Platform Security + Policy + Backend + SWG + QA | Stages 2, 4, 10, 15 | Stage 16 plan/checklist + `implementation-plan/stage-16-verification.md` evidence log |
| 17. ContentPending Reliability and Terminalization (✅) | Timeout diagnostics, pending->terminal reliability hardening, multi-run smoke stabilization | Classification + SWG + SRE + QA | Stages 4, 14, 16 | Stage 17 plan/checklist + verification log with reliability matrix |

## Current Focus

1. Keep strict content-aware behavior stable in production-like smoke runs (`tests/security/facebook-e2e-smoke.sh`).
2. Maintain canonical taxonomy enforcement paths (prompt contract, alias mapping, persistence validation).
3. Improve operator diagnostics and runbook automation around pending classifications and Crawl4AI health.
4. Execute stream consumer-group migration for restart-safe queue processing semantics.
5. Convert remaining policy/reporting page-based APIs to cursor parity where heavy-read patterns warrant keyset traversal.
