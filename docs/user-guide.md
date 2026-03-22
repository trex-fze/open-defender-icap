# Open Defender ICAP – User & Operator Guide

This guide targets administrators, SOC analysts, DevOps/SRE, and support engineers who interact with the platform once deployed.

## 1. Personas & Access
- **Gateway Administrators**: Manage Squid config, ICAP adaptor deployment, SSL bump policies.
- **Policy Administrators**: Create/modify policies, overrides, review decisions.
- **SOC Analysts**: Investigate IP/User/Device activity, monitor review queue, respond to alerts.
- **DevOps/SRE**: Operate services, monitor health/metrics, perform deployments and rollbacks.
- **CLI Power Users**: Use `odctl` for automation (smoke tests, migrations, cache operations).

### 1.1 Authentication
- Admin API/UI/CLI authenticate via enterprise OIDC (client credentials or device flow).
- Service-to-service communication (ICAP adaptor → policy engine, workers → DB) uses mTLS/service tokens.

## 2. Getting Started
1. **Clone repo** and review `docs/engine-adaptor-spec.md` + `docs/architecture.md` for context.
2. **Install prerequisites**: Rust stable (>=1.80), Node LTS, Docker, docker-compose.
3. **Bootstrap workspace**: `cargo check`, `npm install` inside `web-admin`, `docker compose -f deploy/docker/docker-compose.yml up --build`.
4. **Run migrations** (future stage): `odctl migrate run` (once implemented) to set up Postgres schema.
5. **Seed taxonomy**: `odctl taxonomy seed` to populate initial categories (Stage 3+).

## 3. Operating the ICAP Adaptor
- Config file: `config/icap.json` (host/port, preview size, Redis URL, policy endpoint).
- Key env vars: `OD_CONFIG_JSON` for containerized deployments; `RUST_LOG` for logging levels.
- Start service: `cargo run -p icap-adaptor` (dev) or via Docker image built with `deploy/docker/rust.Dockerfile`.
- Monitoring: tail `target/debug/icap-adaptor` logs; watch metrics once Prometheus exporter is added (Stage 6).

## 4. Using the Policy Engine
- Config file: `config/policy-engine.json` (host/port).
- `cargo run -p policy-engine` starts REST API with `/api/v1/decision` + health endpoints.
- Future operations: manage policies via Admin API/UI/CLI; run simulations for policy changes.

## 5. CLI (`odctl`) Usage
| Command | Purpose | Notes |
| --- | --- | --- |
| `odctl help` | Display available commands | Implemented basic placeholder; expanded functionality forthcoming. |
| `odctl health` | Run health checks (future) | Will query backend `/health` endpoints. |
| `odctl smoke run` | Execute smoke tests (future) | Will orchestrate `ST-*` suite from spec. |
| `odctl policy import/export` | Manage policy packages (future) | Depends on Stage 2 completion. |
| `odctl cache lookup/invalidate` | Inspect redis entries (future) | Tied to Stage 3 cache enhancements. |

Config file location: `~/.odctl/config` (YAML/JSON) storing API endpoints & tokens.

## 6. React Admin UI (Future)
- Start dev server: `npm install && npm run dev` in `web-admin/` (port 19001).
- Planned routes: Dashboard, IP/User/Device investigations, Policy mgmt, Overrides, Review queue, Reports, Audit, Health, Cache, Reclassification.
- Authentication: OIDC login; RBAC controlling navigation.
- Build: `npm run build`; deploy static assets behind reverse proxy.

## 7. Docker & Compose Workflows
- **Local dev**: `docker compose -f deploy/docker/docker-compose.yml up --build` to launch Redis, Postgres, adaptor, policy engine, workers, etc.
- **Health checks**: `curl http://localhost:19000/health/ready` (Admin API), `curl http://localhost:19010/health/ready` (Policy), `redis-cli ping`.
- **Logs**: `docker compose logs icap-adaptor` etc.
- **Shutdown**: `docker compose down -v` (warning: removes volumes).

## 8. Troubleshooting
- **ICAP errors**: Check adaptor logs for parse errors; ensure Squid metadata headers present; verify `policy_endpoint` reachable.
- **Redis issues**: Confirm `redis_url` configured; check `redis-cli INFO` for latency; fallback memory cache will emit warnings if Redis unreachable.
- **Policy errors**: 400 from `/api/v1/decision` indicates validation failure; inspect request body for missing `normalized_key`.
- **CLI auth failures**: Ensure config token valid; inspect `~/.odctl/logs` (future) for stack traces.
- **Docker build failures**: Clear `target/` and rebuild; ensure Rust toolchain matches required version.

## 9. Evidence & Reporting
- Keep `rfc/` and `implementation-plan/` documents updated as work progresses.
- Capture test artifacts (`cargo test` logs, smoke results) for Stage 7 signoff.
- Use Kibana dashboards (Stage 6) for SOC/management reporting, export as PDF when requested.

## 10. Support & Escalation
- **First line**: DevOps/SRE on-call monitors health dashboards and alerts.
- **Policy issues**: escalate to Policy Engine team; use simulation endpoint to validate proposed changes.
- **Classification delays**: check Redis Streams queue depth; scale workers as needed.
- **Security incidents**: notify SOC; extract audit events via Admin API or CLI.

Refer to `docs/architecture.md` for system internals and `docs/engine-adaptor-spec.md` + `rfc/` addenda for full requirements. This guide will be updated as each stage delivers new capabilities (UI routes, CLI commands, deployments, etc.).
