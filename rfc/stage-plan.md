# Stage-by-Stage RFC Tracker

This tracker maps implementation stages to the authoritative specification in `docs/engine-adaptor-spec.md` so each code increment explicitly traces back to RFC/standards requirements.

| Stage | Scope | Key RFC / Section References | Status | Notes |
| --- | --- | --- | --- | --- |
| 0. Foundation | Repository layout, config loader, placeholder services | Spec §3 (RFC mapping), §4–§6 | ✅ Complete | Workspace, config crates, placeholder binaries created. |
| 1. ICAP Hot Path | ICAP parsing, normalization, cache/policy skeleton, response handling | RFC 3507, RFC 3986, Spec §§9–11, 21–23 | ✅ Complete | Parser, normalization, policy client, Redis cache & metrics, CLI smoke. |
| 2. Policy Engine Core | Decision API, evaluator, DSL scaffolding | Spec §§14, 20, 21 | ✅ Complete | Policy API, DSL loading, persistence path, and activation gating are active. |
| 3. Cache & Persistence | Redis cache, Postgres schema, override store | Spec §§9, 11, 20 | ✅ Complete | Durable cache + DB schemas (classifications, overrides, activation, page contents) are in use. |
| 4. Async Classification | LLM worker, reclass pipeline, queues | Spec §§15, 16, 24 | ✅ Complete | Queue topology, canonical prompt validation, requeue logic, and strict content gating are live. |
| 5. Admin API/UI/CLI | React UI, CLI commands, admin endpoints | Spec §§13, 14, 18, 19 | ✅ Complete | Admin API/UI/CLI support taxonomy activation, pending workflows, and diagnostics. |
| 6. Reporting & Observability | ES ingestion, dashboards, metrics, alerts | Spec §§17, 18, 33 | ✅ Complete | Event ingest, dashboards, and service metrics are wired for operations. |
| 7. Testing & Ops | Full test suites, Docker/K8s deployment, evidence | Spec §§8, 24–31, 34–35 | ✅ Complete | Unit/integration/security/perf suites plus smoke artifacts and runbooks are established. |
| 10. Frontend Management Parity | Full UI coverage for all existing management APIs and operational diagnostics | Spec §§13, 14, 18, 19, 23, 33 | ✅ Complete | Stage 10 parity scope delivered with operator runbook coverage. |
| 11. RBAC and User/Group Management | Persistent IAM model, effective-role resolution, user/group/service-account lifecycle across API/UI/CLI | Spec §§8, 10, 13, 14, 23, 27 | ✅ Complete | Stage 11 IAM schema/resolver/UI/CLI flows delivered with authz smoke coverage. |
| 13. Domain-First Classification Scope | Canonical domain-key dedupe for pending/classification/content paths while preserving subdomain override precision | Spec §§20.3, 20.3.1 | ✅ Complete | Canonical key helper, ICAP/Admin auto-promotion, migration 0015, and runtime smoke evidence delivered. |

Detailed RFC addenda for implemented stages live under `rfc/`; use this tracker as the high-level status map against the master specification.
