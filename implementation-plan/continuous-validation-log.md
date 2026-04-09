# Continuous Validation Log

## 2026-04-07 regression drill

- `RUNS=10 bash tests/security/facebook-e2e-reliability.sh` -> pass rate 100% (10/10).
- `NORMALIZED_KEY=domain:facebook.com HOST_TAG=drill bash tests/ops/content-pending-diagnostics.sh` -> diagnostics bundle generated successfully.
- `bash tests/taxonomy-parity.sh` -> all parity matrix checks passed.
- `BUILD_IMAGES=0 bash tests/stream-consumer-restart-smoke.sh` -> restart + DLQ smoke passed.
- `bash tests/policy-cursor-smoke.sh` -> cursor chain + index presence smoke passed.

Notes:
- Artifacts were generated under `tests/artifacts/*` during the drill and then cleaned from the working tree to keep the repository tidy.
- This drill satisfies the current Stage 18/19/20/21/22 operational follow-through checks listed in `implementation-plan/stage-plan.md`.

## 2026-04-08 original client IP rollout

- Added single-tenant trusted proxy CIDR handling (`OD_TRUSTED_PROXY_CIDRS=192.168.1.0/24`) in event-ingester config and compose defaults.
- Extended Squid logging to include quoted `X-Forwarded-For` tail and enabled trusted forwarded-chain processing.
- Added ingestion enrichment that keeps `source.ip` as immediate peer and conditionally sets `client.ip` only for trusted peers.
- Added mapping updates for `client.ip` and `od.forwarded_for_raw` in the traffic index template.
- Validation: `cargo test -p event-ingester` passed with trusted/untrusted XFF unit coverage; `npm run build` passed.

## 2026-04-08 proxy ACL configurability hardening

- Replaced hardcoded Squid source/trust CIDRs with runtime-rendered ACL blocks sourced from env vars.
- Added `OD_SQUID_ALLOWED_CLIENT_CIDRS` for access ACL control and kept `OD_TRUSTED_PROXY_CIDRS` for trusted XFF promotion control.
- Added `OD_SQUID_BIND_HOST`/`OD_SQUID_BIND_PORT` to make client proxy endpoint deployment-specific without code edits.
- Updated fast deployment guide, README, and operator runbook with LAN client proxy guidance and troubleshooting for source-IP mismatch.
- Added `tests/proxy-production-linux-e2e.sh` to validate real-client source identity and trusted-XFF promotion on Linux-hosted deployments.

## 2026-04-08 HAProxy fronting Squid design

- Added HAProxy edge proxy plan implementation scaffolding (compose + generated HAProxy config) to preserve client identity via PROXY protocol before Squid.
- Updated Squid listener to `require-proxy-header` with explicit `proxy_protocol_access` trust gate and internal-only exposure in compose.
- Shifted ingestion trust default so `OD_TRUSTED_PROXY_CIDRS` is empty unless an operator explicitly enables trusted XFF promotion.

## 2026-04-08 strict header-trust identity model

- Switched HAProxy edge to overwrite `Forwarded` and `X-Forwarded-For` from socket peer (no untrusted append).
- Removed Squid PROXY-protocol requirement for Desktop/dev path and kept `follow_x_forwarded_for` trust-gated by `OD_TRUSTED_PROXY_CIDRS`.
- Added `OD_TRUST_PROXY_HEADERS` gate in event-ingester and implemented strict client resolver order: `Forwarded` -> `X-Forwarded-For` -> peer fallback.
- Added provenance field `od.client_ip_source` and extended parser coverage for quoted/ported IPv4/IPv6 tokens.
