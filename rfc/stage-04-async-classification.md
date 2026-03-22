# Stage 4 RFC Addendum – Async Classification & Reclassification

**Parent Sections**: `docs/engine-adaptor-spec.md` §§15, 16, 24, 33.

## Objectives
1. Build Redis Stream (or Kafka) queue pipeline for classification jobs.
2. Implement LLM worker with prompt enforcement, JSON validation, retry/backoff.
3. Create reclassification worker for TTL/model/taxonomy refresh.
4. Enforce first-seen placeholder logic + review queue escalation.

## Checklist
- [x] Queue schema, producer/consumer contracts (Spec §9B, §15). *(Redis stream `classification-jobs` with adaptor publisher + LLM worker consumer.)*
- [ ] LLM prompt + response validator per Spec §24 JSON schema.
- [ ] Classification persistence + cache update workflow (Spec §11).
- [ ] Reclassification triggers (low confidence, TTL expiry, taxonomy change) – Spec §16.
- [ ] Metrics/alerts (`llm_invocation_count`, `llm_timeout_rate`, `reclassification_backlog`) – Spec §33.
- [ ] Test suites (unit, integration with mock LLM, perf bursts) – Spec §25–29.

## Traceability Plan
| Requirement | Section | Artifact |
| --- | --- | --- |
| First-seen behavior | Spec §15 | Worker logic, placeholder cache entries |
| LLM guardrails | Spec §24 | Prompt builder, schema validator |
| Metrics | Spec §33 | Prometheus exporters, Kibana dashboards |

## TBD
- Decide on queue technology (Redis Streams vs Kafka) for production.
- Determine LLM provider integration (OpenAI vs internal) and secret handling.
