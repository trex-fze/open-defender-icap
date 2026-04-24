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
| 18. Facebook E2E Reliability Hardening (✅) | Multi-run reliability gate + deterministic failure diagnostics for facebook end-to-end smoke | SWG + Classification + SRE + QA | Stages 9, 17 | Stage 18 plan/checklist + `tests/security/facebook-e2e-reliability.sh` harness + verification log |
| 19. Taxonomy Enforcement Parity (✅) | Cross-service canonicalization/activation/persistence parity matrix and tests | Classification + Policy + Backend + QA | Stages 12, 16, 17 | Stage 19 plan/checklist + `tests/taxonomy-parity.sh` matrix + verification log |
| 20. Ops Diagnostics and Runbook Automation (✅) | One-command pending diagnostics collector + runbook triage automation | SRE + SWG + Docs | Stages 17, 18 | Stage 20 plan/checklist + `tests/ops/content-pending-diagnostics.sh` + verification log |
| 21. Stream Consumer-Group Migration (✅) | Restart-safe Redis stream processing (`XREADGROUP`, ACK/claim, poison handling) | Backend + Classification + SRE | Stages 4, 14, 17 | Stage 21 plan/checklist + stream audit/restart smoke artifacts + verification log |
| 22. Cursor Parity for Policy/Reporting APIs (✅) | Convert remaining policy/reporting page-offset list APIs to cursor/keyset parity | Backend + Frontend + DevTools + QA | Stages 15, 16 | Stage 22 plan/checklist + cursor audit + `tests/policy-cursor-smoke.sh` evidence |
| 23. Dashboard Traffic Intelligence (✅) | Deliver rich dashboard graphs and client-IP traffic intelligence with backend reporting endpoint and ingest field enrichment | Backend + Frontend + SRE + QA | Stages 6, 10, 22 | Stage 23 RFC/plan/checklist + dashboard analytics verification artifacts |
| 24. Reliability and Operability Hardening (✅) | Config fail-fast contract, queue idempotency/replay tooling, unified diagnostics bundle, auth/session hardening, golden deployment profile | Platform + Security + SRE + Backend + Frontend + DevTools + QA | Stages 20, 21, 23 | Stage 24 RFC/plan/checklist + reliability/auth/ops evidence bundle |
| 25. Prompt Injection Hardening (Planned) | Strict visible-only crawl extraction, llm-worker injection detection and forced-review guardrails, runtime action-authority hardening | Security + Classification + SWG + Policy + SRE + QA | Stages 14, 16, 17, 21, 24 | Stage 25 plan/checklist/decisions + security smoke and verification evidence |

## Current Focus

Operational gate ownership, cadence, thresholds, and evidence locations are tracked in `docs/runbooks/operational-completion-matrix.md`.

1. Keep Stage 18 reliability gate green in routine regression runs (`RUNS=10`).
2. Keep Stage 20 diagnostics collector/runbook path validated in on-call drills.
3. Periodically run `tests/taxonomy-parity.sh` to guard cross-service taxonomy canonicalization parity.
4. Periodically run `tests/stream-consumer-restart-smoke.sh` and `tests/policy-cursor-smoke.sh` in release-candidate validation.
5. Keep Stage 24 reliability/auth/golden-profile verification gates in release-candidate runs.
6. Execute Stage 25 prompt-injection hardening gates, including strict extraction and excerpt-based security smoke coverage.
