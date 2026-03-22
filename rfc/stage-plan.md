# Stage-by-Stage RFC Tracker

This tracker maps implementation stages to the authoritative specification in `docs/engine-adaptor-spec.md` so each code increment explicitly traces back to RFC/standards requirements.

| Stage | Scope | Key RFC / Section References | Status | Notes |
| --- | --- | --- | --- | --- |
| 0. Foundation | Repository layout, config loader, placeholder services | Spec §3 (RFC mapping), §4–§6 | ✅ Complete | Workspace, config crates, placeholder binaries created. |
| 1. ICAP Hot Path | ICAP parsing, normalization, cache/policy skeleton, response handling | RFC 3507, RFC 3986, Spec §§9–11, 21–23 | 🟡 In Progress | Parser + normalization + policy call implemented; pending Redis hardening + Squid integration tests. |
| 2. Policy Engine Core | Decision API, evaluator, DSL scaffolding | Spec §§14, 20, 21 | 🟡 In Progress | Basic evaluator live; needs DSL persistence, precedence tree, auth. |
| 3. Cache & Persistence | Redis cache, Postgres schema, override store | Spec §§9, 11, 20 | ⬜ Planned | Implement durable cache, TTL/version management, DB migrations. |
| 4. Async Classification | LLM worker, reclass pipeline, queues | Spec §§15, 16, 24 | ⬜ Planned | Build queue topology, prompt validation, fallback paths. |
| 5. Admin API/UI/CLI | React UI, CLI commands, admin endpoints | Spec §§13, 14, 18, 19 | ⬜ Planned | After backend maturity, expose policies, overrides, reports. |
| 6. Reporting & Observability | ES ingestion, dashboards, metrics, alerts | Spec §§17, 18, 33 | ⬜ Planned | Implement event sinks, dashboards, alert rules, evidence capture. |
| 7. Testing & Ops | Full test suites, Docker/K8s deployment, evidence | Spec §§8, 24–31, 34–35 | ⬜ Planned | Execute smoke/integration/perf/security suites, runbooks, signoffs. |

Each stage will gain a dedicated RFC addendum document under `rfc/` as we elaborate details beyond the master spec. Stage 1 addendum will be added when Redis integration + Squid response handling complete.|
