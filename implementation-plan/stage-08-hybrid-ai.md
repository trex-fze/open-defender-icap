# Stage 8 Implementation Plan – AI Hybrid Model Support

**Status**: Complete

## Objectives
- Support multiple LLM providers (offline + online) with policy-based routing and fallback.
- Provide tooling/observability so operators can inspect provider health, metrics, and security posture.
- Document operational workflows (compose overlays, CLI commands, evidence) for AI-assisted investigations.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| S8-T1 | Config schema & router (`providers` + `routing`) | Backend Eng | Stages 1–7 | ✅ | WorkerConfig refactor + provider router committed |
| S8-T2 | Offline adapters (LM Studio, Ollama, vLLM) | Backend Eng | S8-T1 | ✅ | LM Studio example (`gpt-oss-120b`), Ollama/OpenAI-compatible JSON adapters |
| S8-T3 | Online adapters (OpenAI, Claude/Anthropic) | Backend Eng | S8-T1 | ✅ | OpenAI chat + Anthropic message integrations landed |
| S8-T4 | CLI/Admin tooling (`odctl llm providers`, metrics `/providers`) | CLI/Backend | S8-T1 | ✅ | New `odctl llm providers` command + metrics endpoint catalog |
| S8-T5 | Provider-level metrics & alerts | SRE | S8-T1–T3 | ✅ | Per-provider counters/latency histograms + Prometheus rules (`stage8-llm-alerts`) |
| S8-T6 | Document external LM Studio/Ollama integration | DevOps | S8-T2 | ✅ | README/integration plan describe connecting to remote LM Studio (192.168.1.170) or standalone Ollama nodes |
| S8-T7 | Security/perf validation (prompt injection, fallback load) | Security/Perf Eng | S8-T2–T4 | ✅ | `tests/security/llm-prompt-smoke.sh` + `tests/perf/llm-failover.sh` cover injection + failover |
| S8-T8 | Evidence & runbooks (Stage 8) | TPM | S8-T1–S8-T7 | ✅ | Evidence set + runbook steps linked from `docs/evidence/stage08-hybrid-ai.md` |

## Milestones
1. **M1 – Hybrid Config & Offline Providers** (S8-T1/T2) ✅
2. **M2 – Online Providers & Routing** (S8-T3/T4/T5) ✅
3. **M3 – Ops/Security Evidence** (S8-T6–T8) ✅

## Risks & Mitigations
| Risk | Mitigation |
| --- | --- |
| Provider secrets leaking in configs | Enforce `api_key_env` usage, document secret management. |
| Offline model drift vs SaaS outputs | Store provider metadata in classifications; build dashboards to compare actions. |
| Latency spikes on failover | Prometheus alert on `llm_provider_timeouts_total` and fallback counts. |
| Prompt injection across providers | Re-run Stage 7 security tests with each backend, sanitize prompts centrally. |

## Testing & Evidence
- `cargo test -p llm-worker` (unit/integration) – run before merging provider changes.
- `odctl llm providers` – record output and attach to Stage 8 evidence.
- Perf/security scripts (to be added under `tests/perf` / `tests/security`).
- Compose instructions & screenshots for LM Studio connectivity.

## Next Steps
1. Finish per-provider metrics + Prometheus alerts (S8-T5).
2. Add compose overlays + documentation for LM Studio/Ollama (S8-T6).
3. Extend security/perf suites for provider failover (S8-T7).
4. Capture evidence + runbooks (S8-T8).

## Stage 8 Addendum (Post-Complete)
- Added budgeted stale-pending online diversion in `llm-worker`:
  - Uses `classification_requests.requested_at` age to determine stale pending eligibility.
  - Attempts configured online provider when health checks pass.
  - Respects existing fallback controls and introduces a separate stale-divert per-minute cap.
- Added metrics family `llm_stale_pending_*` and updated architecture/runbook docs.
- Added online-context mode controls for online providers:
  - `required|preferred|metadata_only` excerpt handling (`OD_LLM_ONLINE_CONTEXT_MODE`).
  - metadata-only guardrails (`OD_LLM_METADATA_ONLY_FORCE_ACTION`, `OD_LLM_METADATA_ONLY_MAX_CONFIDENCE`).
  - optional pending follow-up mode (`OD_LLM_METADATA_ONLY_REQUEUE_FOR_CONTENT`).
- Added observability for context decisions and guardrails via `llm_context_mode_total`, `llm_metadata_only_guardrail_total`, and `llm_metadata_only_requeue_total`.
- Added no-content/API fallback controls:
  - `OD_LLM_CONTENT_REQUIRED_MODE=required|auto` for strict vs adaptive content gating.
  - `OD_LLM_METADATA_ONLY_FETCH_FAILURE_THRESHOLD` (default `2`) and `OD_LLM_METADATA_ONLY_NO_CONTENT_STATUSES` to trigger metadata fallback after repeated terminal fetch failures.
  - `OD_LLM_METADATA_ONLY_ALLOWED_FOR=online|all` to support offline-only provider deployments.
- Added observability for metadata fallback reasons via `llm_metadata_only_reason_total` and `llm_fetch_failure_fallback_total`.
