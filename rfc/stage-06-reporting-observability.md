# Stage 6 RFC Addendum – Reporting & Observability

**Parent Sections**: `docs/engine-adaptor-spec.md` §§16, 17, 18, 33.

## Objectives
1. Ingest decision/audit events into Elasticsearch with ECS-compliant schemas.
2. Build Kibana dashboards for IP-based analytics, SOC, management, and ops views.
3. Expose Prometheus metrics per Spec §33 with alert rules for key KPIs.
4. Provide report APIs/export tools feeding UI/CLI.

## Checklist
- [x] Event ingestion pipeline (Filebeat/ingester) with trace correlation – Spec §17 (Filebeat container ships Squid logs to the new Rust event-ingester service which enriches events and bulk indexes them into Elasticsearch).
- [x] Elasticsearch index templates, ILM policies, retention – Spec §17 & §20 (JSON templates + ILM policy checked in under `deploy/elastic/` and auto-applied by event-ingester on startup).
- [x] Kibana dashboards (IP, user/device, management, security) – Spec §16 + §17 (initial Traffic Operations dashboard + saved objects under `deploy/kibana/dashboards` covering allow/block trends and top blocked domains; additional panels follow the same pattern).
- [x] Metrics export (`squid_to_icap_latency`, `cache_hit_ratio`, etc.) – Spec §33 (ICAP adaptor now exposes cache hit ratio + end-to-end latency; event-ingester publishes batch counters and durations; Prometheus scrapes all services).
- [x] Alert definitions + runbooks – Spec §33, §34 (Prometheus loads `prometheus-rules.yml` with cache ratio, latency, ingestion failure, and review SLA breach alerts).
- [ ] Report APIs & CLI helpers – Spec §16, §19.
- [ ] Unit/integration tests for ingestion, dashboards, and alerting workflows – Spec §25–26.

## Traceability Plan
| Requirement | Section | Artifact |
| --- | --- | --- |
| IP analytics | Spec §16 | Kibana dashboards, report APIs |
| Observability metrics | Spec §33 | Prometheus exporters, alert configs |
| Evidence retention | Spec §29 | Dashboard screenshots, log samples |

## Pending Workflows
- Define data masking for PII in logs/dashboards.
- Align alert routing with SOC on-call structure.
