# Implementation Plan – AI Hybrid Model Support

**Status:** Planned  
**Epic:** Enable offline & online LLM providers for `llm-worker`.

## Objectives

1. Refactor LLM worker to support multiple provider types (offline + online).  
2. Provide tooling/docs for configuring, monitoring, and testing providers.  
3. Preserve security/compliance requirements (prompt hygiene, secrets management).

## Work Breakdown

| Task ID | Description | Owner | Dependencies | Status |
| --- | --- | --- | --- | --- |
| AI-H1 | Config schema refactor (`providers` + `routing`, env override support) | Backend Eng | Current `llm-worker` config | ⬜ |
| AI-H2 | Provider abstraction layer (`LlmProvider` trait + adapters for LM Studio, Ollama, vLLM, OpenAI, Claude) | Backend Eng | AI-H1 | ⬜ |
| AI-H3 | Routing/fallback logic + metrics labels (provider, outcome) | Backend Eng | AI-H2 | ⬜ |
| AI-H4 | Compose overlays + docs for offline providers (LM Studio/Ollama containers) | DevOps | AI-H2 | ⬜ |
| AI-H5 | CLI/Admin tooling (`odctl llm check`, optional Admin API endpoint) | CLI/Backend | AI-H2 | ⬜ |
| AI-H6 | Security review (prompt sanitization audit, credential guidance) | Security Eng | AI-H2 | ⬜ |
| AI-H7 | Testing suite (unit, integration against mock providers, perf & failover) | QA/Perf Eng | AI-H2 | ⬜ |
| AI-H8 | Documentation updates (README, Stage plans, runbooks) | Tech Writer | AI-H1–H7 | ⬜ |

## Task Details

### AI-H1 Config Refactor
- Extend `config/llm-worker.json` schema; add `providers` array & `routing` block.
- Support env references for secrets (`api_key_env`).
- Provide migration helper (default current endpoint becomes a single provider entry).

### AI-H2 Provider Abstraction
- Introduce `providers/mod.rs` with modules per backend:
  - `lmstudio`, `ollama`, `vllm` (OpenAI-compatible), `openai`, `anthropic`.
- Each adapter responsible for request formatting, auth headers, response normalization.

### AI-H3 Routing & Metrics
- Implement `ProviderRouter` that selects provider per job, tracks fallbacks, and emits metrics:
  - `llm_requests_total{provider="openai"}`
  - `llm_failovers_total` when fallback triggered.
- Update `tests/stage06_ingest.sh` or new smoke to validate metrics.

### AI-H4 Deployment Assets
- Add compose services/examples for LM Studio & Ollama (optional profiles).
- Document GPU requirements (if any) and offline image caching.

### AI-H5 Tooling
- **CLI:** new `odctl llm check --provider <name>` calling Admin API or direct worker health endpoint.
- **Admin API (optional):** `/api/v1/llm/providers` listing status/latency.

### AI-H6 Security
- Update `docs/testing/security-plan.md` with AI provider coverage.
- Document secret handling + network isolation (air-gapped guidance).

### AI-H7 Testing
- Unit tests: provider selection, config parsing, response normalization.
- Integration: run worker against mock HTTP servers for offline/online providers.
- Performance: adapt `tests/perf/k6-traffic.js` or add provider-specific soak tests.
- Failover simulation: intentionally break primary provider and ensure fallback is used.

### AI-H8 Documentation
- README: mention hybrid AI support in Quick Start/FAQ.
- Stage 6/7 plans: note future tasks or update status when implemented.
- Runbooks: add provider troubleshooting commands.

## Milestones

1. **Milestone 1 – Config + Offline Provider (Ollama)**: Complete AI-H1/H2 for local provider; ensure tests pass offline.  
2. **Milestone 2 – Online Providers + Routing**: Add OpenAI/Claude adapters, fallback metrics (AI-H3).  
3. **Milestone 3 – Tooling & Docs**: CLI/Admin APIs, compose overlays, security docs (AI-H4–H8).

## Dependencies & Risks

| Dependency | Notes |
| --- | --- |
| LM Studio/Ollama container images | Need instructions for GPU/CPU environments. |
| OpenAI/Claude API keys | Provide sample `.env` references; avoid hardcoding secrets. |

| Risk | Mitigation |
| --- | --- |
| Provider drift/feature mismatch | Normalize to common `LlmResponse`; document unsupported features. |
| Increased attack surface (external APIs) | Enforce TLS, store keys in env/secret manager, expand authZ tests. |
| Latency variability | Add Prometheus alerts for provider latency & failover counts. |

## Evidence & Signoff

- Config diff showing `providers` block.
- Logs/metrics from offline + online runs.
- `odctl llm check` output for each provider.
- Updated documentation references (README, security plan, runbooks).
- Test results (unit, integration, perf, security).

Once all tasks reach ✅, link evidence in `docs/evidence/stage08-hybrid-ai.md` (to be created during execution).
