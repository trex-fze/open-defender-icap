# Stage 1 RFC Addendum – ICAP Hot Path

**Parent Spec**: `docs/engine-adaptor-spec.md` §§3, 4, 9–12, 21–24.

## Objectives
1. Parse ICAP REQMOD requests and encapsulated HTTP messages per RFC 3507.
2. Normalize domains/URLs (RFC 3986/5890) to generate cache keys and policy context.
3. Integrate policy decision service via REST (JSON API).
4. Provide multi-tier caching with Redis + in-process fallback.
5. Return ICAP responses within latency budget (<40 ms p95) with placeholder enforcement.

## Requirements Checklist
- [x] ICAP parser handling start line, headers, HTTP block (§11).
- [x] URL normalizer with punycode + registered-domain detection (§10, §21).
- [x] Policy decision request schema aligned with Spec §23.1.
- [x] In-process cache honors TTLs and prevents stale reuse (§11).
- [x] Redis integration for shared verdict cache with retry/backoff (§11, §33 `cache_hit_ratio`).
- [x] ICAP 204/200 responses for allow/block actions (§11 fail-open guidance).
- [x] Metrics/trace propagation with `/metrics` endpoint (§17, §33).
- [x] CLI smoke test exercising ICAP REQMOD flow (§27).
- [ ] Override lookup, manual placeholder actions (§14) – pending Stage 2.
- [ ] Event emission/audit logging (§17, §20) – pending Stage 3.
- [ ] TLS/mTLS between Squid and adaptor (§3 TLS RFCs) – planned Stage 3.
- [x] Unit/integration tests covering parser, cache fallback, and ICAP error paths (§24–26).

## Traceability
- **RFC 3507**: `icap::IcapRequest` parser ensures compliance with REQMOD structure.
- **RFC 3986 / 5890**: `normalizer::normalize_target` enforces URI + IDNA normalization.
- **Spec §9**: Cache design implemented via `cache::CacheClient` (Redis + memory).
- **Spec §20**: Policy decision API consumed via `policy_client::PolicyClient`.
- **Spec §11**: ICAP responses generated in `icap_response` helper with block vs allow behavior.

## Next Actions
1. Add override/policy context enrichment before policy calls (tie-in with Spec §14 order of evaluation).
2. Enforce trace IDs + structured logging for each ICAP verdict (Spec §17).
3. Harden Redis connectivity (retry/backoff, health metrics) and expose cache metrics endpoints.
4. Implement RESP MOD support and preview handling once content filtering pipeline attaches (Stage 4).
