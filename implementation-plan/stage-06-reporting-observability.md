# Stage 6 Implementation Plan – Reporting & Observability

**Status**: Planned

## Objectives
- Implement Elasticsearch/Kibana dashboards, Prometheus metrics, reporting APIs.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status |
| --- | --- | --- | --- | --- |
| S6-T1 | Build event ingestion pipeline (Filebeat/ingester) | SRE | Stage 3 audits | ✅ | Event ingester service + Filebeat shipper wired to Elasticsearch via compose |
| S6-T2 | Define ES index templates + ILM policies | Data Eng | S6-T1 | ✅ | `deploy/elastic/*.json` + event-ingester bootstrap auto-apply templates/ILM |
| S6-T3 | Create Kibana dashboards per Spec §16 | SOC | S6-T2 | ✅ | `deploy/kibana/dashboards/ip-analytics.ndjson` seeds Traffic Operations dashboard (allow/block trend + top blocked domains) |
| S6-T4 | Expose Prometheus metrics + alerts | SRE | S1–S4 metrics | ✅ | New cache hit ratio & latency metrics, event-ingester counters, Prometheus scrape targets + Stage 6 alert rules |
| S6-T5 | Implement report APIs + CLI helpers | Backend Eng | S5 API | ⬜ |
| S6-T6 | Evidence capture (screenshots, logs) | QA/SOC | S6-T3 | ⬜ |
| S6-T7 | Integration tests for ingestion/dashboards/alerts | QA/SRE | S6-T1–T5 | ⬜ |
