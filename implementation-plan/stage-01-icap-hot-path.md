# Stage 1 Implementation Plan – ICAP Hot Path

**Status**: In Progress

## Objectives
- Deliver ICAP parsing, normalization, cache/policy integration, and ICAP response handling per RFC Stage 1 addendum.

## Work Breakdown
| Task ID | Description | Owner | Dependencies | Status | Evidence |
| --- | --- | --- | --- | --- | --- |
| S1-T1 | Implement ICAP parser + HTTP metadata extraction | Secure Gateway Eng | None | ✅ | `icap::IcapRequest` tests |
| S1-T2 | Build normalization pipeline (punycode, registered domains) | Secure Gateway Eng | S1-T1 | ✅ | `normalizer` tests |
| S1-T3 | Integrate policy client over REST JSON | Backend Eng | Policy API stub | ✅ | `policy_client` tests, `cargo test -p icap-adaptor` |
| S1-T4 | Multi-tier cache (memory + Redis) with TTL | Platform Eng | Redis available | ✅ | `cache::CacheClient` retries/backoff + Redis metrics |
| S1-T5 | ICAP response generation (allow/block) | Secure Gateway Eng | S1-T3 | ✅ | `icap_response` logic, manual verification |
| S1-T6 | Trace/log propagation & metrics | Platform Eng | S1-T4 | ✅ | `/metrics` endpoint + trace_id logging |
| S1-T7 | Squid integration smoke test | QA | S1-T5 | ✅ | `odctl smoke` ICAP request validation |

## Risks & Mitigations
- Redis unavailable → fallback memory cache but needs alerting (T4 follow-up).
- Policy client errors → need retries/backoff (future task).

## Next Checkpoint
- Complete T4/T6/T7, capture evidence, then move Stage 1 to “Complete”.
