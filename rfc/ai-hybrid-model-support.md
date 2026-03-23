# RFC – AI Hybrid Model Support (Offline + Online)

**Status:** Draft  
**Authors:** Open Defender ICAP Team  
**Related Specs:** `docs/engine-adaptor-spec.md` §§16, 24, 29–34

## 1. Overview

Open Defender currently assumes a single remote LLM endpoint per `llm-worker`. To support air-gapped deployments and cloud AI services simultaneously, we will introduce a provider-agnostic architecture that can route classification jobs to offline models (LM Studio, Ollama, vLLM, etc.) or online SaaS providers (OpenAI, Anthropic Claude) based on policy, performance, or cost.

## 2. Goals

- Allow operators to configure multiple providers (offline/online) with unified configuration.
- Enable dynamic selection/fallback without code changes (e.g., route primary traffic to local vLLM, burst to OpenAI when offline node is saturated).
- Preserve existing job ingestion, metrics, and security guarantees.
- Provide CLI/Admin tooling to list, validate, and switch providers safely.

## 3. Non-goals

- Training or fine-tuning models inside this repo.
- Shipping vendor-specific SDKs beyond HTTP/REST integrations.
- Replacing the deterministic policy engine (LLMs remain advisory).

## 4. Architecture

### 4.1 Provider Abstraction

Define a `LlmProvider` trait with implementations for:

- **Offline providers:**
  - **LM Studio** – local HTTP endpoint, configurable model name.
  - **Ollama** – REST API (`/api/generate`) with model selection.
  - **vLLM** – exposes OpenAI-compatible API (treat as “OpenAI-compatible offline”).
- **Online providers:**
  - **OpenAI** – `https://api.openai.com/v1/chat/completions` with API key + model.
  - **Anthropic Claude** – HTTPS API via API key (or AWS Bedrock credentials).

### 4.2 Configuration

Extend `config/llm-worker.json` to support:

```jsonc
{
  "providers": [
    {
      "name": "local-ollama",
      "type": "ollama",
      "endpoint": "http://ollama:11434",
      "model": "llama3",
      "timeout_ms": 15000
    },
    {
      "name": "openai-prod",
      "type": "openai",
      "endpoint": "https://api.openai.com/v1",
      "model": "gpt-4o-mini",
      "api_key_env": "OPENAI_API_KEY"
    }
  ],
  "routing": {
    "default": "local-ollama",
    "fallback": "openai-prod",
    "policy": "failover" // future: weight-based, round-robin
  }
}
```

Environment variables take precedence for secrets (e.g., `OPENAI_API_KEY`).

### 4.3 Selection & Fallback

- Job consumer reads routing policy:
  - **primary** provider handles traffic.
  - On network error/timeout or rate limit, fallback is attempted.
- Future extensions: weight-based distribution, capability tags (e.g., “supports function calling”).

### 4.4 Security & Compliance

- Offline connectors run on localhost/compose network; enforce allowlists.
- Online connectors require API keys stored via env vars/secret managers.
- Prompt-injection hardening remains (sanitization + audit logging).
- Audit logs include provider name, latency, truncated prompts/responses.

### 4.5 Observability

- Metrics labeled with provider (`llm_requests_total{provider="openai-prod"}`) + success/failure.
- Prometheus alerts for provider saturation (timeouts, fallbacks).
- `odctl` command to run provider health checks (`odctl llm check --provider local-ollama`).

### 4.6 Deployment

- Compose overlays for LM Studio/Ollama containers.
- Docs for pointing at external SaaS (OpenAI/Claude) including rate-limit guidance.

## 5. API & Schema Changes

- `config/llm-worker.json` gains `providers` + `routing` sections.
- `llm-worker` CLI logging includes provider name.
- Optional new Admin API endpoint to list provider status.

## 6. Risks & Mitigations

| Risk | Mitigation |
| --- | --- |
| Credential sprawl | Use env refs (`api_key_env`) instead of raw strings; document secret storage. |
| Offline provider latency | Provide Prometheus metric + alert; allow fallback to online provider. |
| Incompatible JSON schemas | Transform responses to internal `LlmResponse` struct with adapters. |
| Model drift between providers | Store provider + model metadata in classification record for audit. |

## 7. Open Questions

- Do we need synchronous admin toggles (e.g., disable online providers at runtime)?
- Should we persist provider selection decisions for analytics (ES index)?

## 8. Acceptance Criteria

1. `llm-worker` can run with only offline providers (no internet).
2. Operator can add an online provider via config/env, and jobs succeed without code changes.
3. Metrics and logs expose provider name + error counts.
4. `tests/security/authz-smoke.sh` + new provider health checks pass.
5. Docs updated (README, Stage plans) highlighting hybrid AI support.
