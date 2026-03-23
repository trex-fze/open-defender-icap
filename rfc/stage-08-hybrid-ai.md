# Stage 8 RFC Addendum – Hybrid AI Model Support

**Parent Sections:** `docs/engine-adaptor-spec.md` §§16, 24, 29–34

## Objectives
1. Allow operators to mix offline (LM Studio, Ollama, vLLM) and online (OpenAI, Claude/Anthropic) LLM providers.
2. Provide routing/failover and observability to manage AI-assisted classification safely.
3. Document security, deployment, and evidence requirements for hybrid AI operations.

## Checklist
- [x] Config schema supports multiple providers with routing/fallback (`config/llm-worker.json`).
- [x] Provider-specific adapters (OpenAI, Anthropic, Ollama, custom HTTP) integrated in `llm-worker`.
- [x] CLI & metrics endpoints expose provider catalogs (`odctl llm providers`, `/providers`).
- [x] Per-provider Prometheus metrics/alerts (latency, failure, timeout labels).
- [x] Compose overlays/instructions for LM Studio/Ollama deployment.
- [x] Security/perf suites updated for provider failover + prompt-injection hardening (`tests/security/llm-prompt-smoke.sh`, `tests/perf/llm-failover.sh`).
- [ ] Stage 8 evidence bundle (`docs/evidence/stage08-hybrid-ai.md`).

## Design Summary

### Provider Abstraction
- `ProviderKind` enumerates backend types (`lmstudio`, `ollama`, `vllm`, `openai`, `anthropic`, `custom_json`).
- `ProviderRouter` selects the primary provider and optional fallback per routing policy.
- Fallback occurs on HTTP/network errors; provider name logged/recorded for audit.

### Request/Response Normalization
- Prompts standardized via `SYSTEM_PROMPT` + `build_prompt` to ensure consistent JSON outputs.
- Parsers strip markdown fences/backticks before deserializing to `LlmResponse`.
- Classification storage includes provider metadata for future analytics.

### Observability
- Metrics server now exposes `/providers` for quick catalog inspection.
- Prometheus metrics/alerts include provider labels for invocations, failures, timeouts, and latency (see `stage8-llm-alerts`).
- `odctl llm providers` surfaces the same data from operator terminals.

### Security & Compliance
- Encourage env var secrets (`api_key_env`) to avoid plaintext keys.
- Document prompt-injection tests for each provider (Stage 7 security plan extension).
- Logs include provider info but omit raw prompts/responses unless required for debugging (respect PII guidelines).

### Deployment
- Default example: LM Studio at `http://192.168.1.170:1234` running `gpt-oss-120b`, with OpenAI fallback.
- Operators run LM Studio/Ollama on separate hosts or docker instances; the core compose stack remains unchanged.
- Online providers require API keys stored in `.env` or secret manager.

## Open Questions
- Should provider selection be dynamic (per taxonomy) via Admin API?
- Do we need real-time health checks (keepalive) to pre-emptively switch providers?
- Where should evidence/log storage live for hybrid runs (Elasticsearch index?).

## Acceptance Criteria
1. Operator can configure at least one offline and one online provider, switch via config, and run `llm-worker` without redeploying code.
2. CLI/metrics endpoints reflect active providers.
3. Per-provider metrics hit defined thresholds and feed alerts.
4. Security/perf suites cover fallback + prompt-injection scenarios.
5. Evidence package produced for Stage 8 sign-off (config, logs, screenshots).
