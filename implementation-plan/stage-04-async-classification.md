# Stage 4 Implementation Plan – Async Classification & Reclassification

**Status**: Planned

## Objectives
- Build queue-driven classification pipeline, LLM worker, reclassification jobs.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status |
| --- | --- | --- | --- | --- |
| S4-T1 | Select queue tech + provision infra | Platform Eng | Stage 3 persistence | ✅ | Redis Streams (`classification-jobs`) + config wiring |
| S4-T2 | Implement job publisher in ICAP adaptor | Secure Gateway Eng | S4-T1 | ✅ | Adaptor publishes `ClassificationJob` events when verdict missing/review |
| S4-T3 | Build LLM worker (prompt builder, validator, persistence) | ML Eng | S4-T1 | ✅ | Worker sends prompts to configured LLM endpoint, validates responses, and persists classifications in Postgres |
| S4-T4 | Reclassification worker w/ triggers | ML Eng | S4-T2 | ✅ | Inserts `reclassification_jobs` for stale TTLs and republishes jobs via Redis stream |
| S4-T5 | Monitoring & metrics for workers | SRE | S4-T3 | ✅ | LLM + reclass workers expose Prometheus metrics/HTTP endpoints |
| S4-T6 | Integration tests w/ mock LLM | QA | S4-T3 | ✅ | Dockerized Redis→LLM worker→Postgres test plus reclass planner/dispatcher coverage |
