# Stage 4 Implementation Plan – Async Classification & Reclassification

**Status**: Planned

## Objectives
- Build queue-driven classification pipeline, LLM worker, reclassification jobs.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status |
| --- | --- | --- | --- | --- |
| S4-T1 | Select queue tech + provision infra | Platform Eng | Stage 3 persistence | ✅ | Redis Streams (`classification-jobs`) + config wiring |
| S4-T2 | Implement job publisher in ICAP adaptor | Secure Gateway Eng | S4-T1 | ✅ | Adaptor publishes `ClassificationJob` events when verdict missing/review |
| S4-T3 | Build LLM worker (prompt builder, validator, persistence) | ML Eng | S4-T1 | ⏳ | Worker now consumes Redis stream/logs jobs; prompt/LLM integration pending |
| S4-T4 | Reclassification worker w/ triggers | ML Eng | S4-T2 | ⬜ |
| S4-T5 | Monitoring & metrics for workers | SRE | S4-T3 | ⬜ |
| S4-T6 | Integration tests w/ mock LLM | QA | S4-T3 | ⬜ |
