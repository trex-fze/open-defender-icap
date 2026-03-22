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
- Config file: `config/icap.json` (host/port, preview size, Redis URL, policy endpoint, metrics host/port).
- Key env vars: `OD_CONFIG_JSON` for containerized deployments; `RUST_LOG` for logging levels.
- Start service: `cargo run -p icap-adaptor` (dev) or via Docker image built with `deploy/docker/rust.Dockerfile`.
- Monitoring: tail `target/debug/icap-adaptor` logs and scrape Prometheus metrics from `http://<metrics_host>:<metrics_port>/metrics` (default `19005`).

## 4. Using the Policy Engine
- Config file: `config/policy-engine.json` (host/port, DSL path, optional `database_url`, optional `admin_token`). Leave `database_url` as `null` for file-backed mode; set to a Postgres connection string to enable persistent storage. When `admin_token` is set, admin APIs require header `X-Admin-Token` (CLI reads `OD_ADMIN_TOKEN`).
- Policies reside in `config/policies.json`; `GET /api/v1/policies` lists active rules, `POST /api/v1/policies/reload` hot-reloads from the DSL/DB, `POST /api/v1/policies` (DB mode only) creates a new policy document, and `POST /api/v1/policies/simulate` evaluates a sample request without enforcing it.
- `cargo run -p policy-engine` starts REST API with `/api/v1/decision` + health endpoints. On startup the service applies migrations in `services/policy-engine/migrations/` and seeds from the DSL file if the database is empty.
- Future operations: manage policies via Admin API/UI/CLI; run simulations for policy changes.

## 5. Admin API & Overrides
- Config file: `config/admin-api.json` controls host/port, optional `database_url`, and optional `admin_token`. Leave `database_url` as `null` for check-ins, but set either `database_url` in the file or `OD_ADMIN_DATABASE_URL`/`DATABASE_URL` env vars in deployment shells; the service refuses to start without one of these values.
- Admin authentication: set `admin_token` in the config or provide `OD_ADMIN_TOKEN` (the CLI already reads this variable). Requests must include header `X-Admin-Token` when any token is configured.
- Service startup: `cargo run -p admin-api` applies migrations in `services/admin-api/migrations/` and exposes overrides + review queue routes under `/api/v1`. Operators can also run inside Docker by adding the same env vars to the container spec.
- Health checks: `curl http://localhost:19000/health/ready` (readiness) and `/health/live` (liveness). Use `OD_ADMIN_URL` (default `http://localhost:19000`) to point `odctl override ...` commands at the service.

## 6. CLI (`odctl`) Usage
| Command | Purpose | Notes |
| --- | --- | --- |
| `odctl help` | Display available commands | Lists current subcommands. |
| `odctl health` | Run health checks (future) | Will query backend `/health` endpoints. |
| `odctl smoke [host:port]` | Send sample ICAP REQMOD to adaptor | Defaults to `127.0.0.1:1344`; prints ICAP status line. |
| `odctl policy list` | List active policy rules via policy engine | Respects `OD_POLICY_URL` (default `http://localhost:19010`); add `OD_ADMIN_TOKEN` for protected endpoints. |
| `odctl policy reload` | Trigger policy reload (file/DB backed) | Requires admin token when configured. |
| `odctl policy simulate <file>` | Hit `/api/v1/policies/simulate` with a JSON request | JSON must match `DecisionRequest`; requires admin token when configured. |
| `odctl policy import/export` | Manage policy packages (future) | Depends on Stage 2 completion. |
| `odctl cache lookup/invalidate` | Inspect redis entries (future) | Tied to Stage 3 cache enhancements. |

Config file location: `~/.odctl/config` (YAML/JSON) storing API endpoints & tokens. Example commands: `odctl smoke 10.0.0.5:1344`, `OD_POLICY_URL=http://localhost:19010 OD_ADMIN_TOKEN=secret odctl policy reload`, `OD_ADMIN_TOKEN=secret odctl policy simulate request.json`.

## 7. React Admin UI (Future)
- Start dev server: `npm install && npm run dev` in `web-admin/` (port 19001).
- Planned routes: Dashboard, IP/User/Device investigations, Policy mgmt, Overrides, Review queue, Reports, Audit, Health, Cache, Reclassification.
- Authentication: OIDC login; RBAC controlling navigation.
- Build: `npm run build`; deploy static assets behind reverse proxy.

## 8. Docker & Compose Workflows
- **Local dev**: `docker compose -f deploy/docker/docker-compose.yml up --build` to launch Redis, Postgres, adaptor, policy engine, workers, etc.
- **Health checks**: `curl http://localhost:19000/health/ready` (Admin API), `curl http://localhost:19010/health/ready` (Policy), `redis-cli ping`.
- **Logs**: `docker compose logs icap-adaptor` etc.
- **Shutdown**: `docker compose down -v` (warning: removes volumes).

## 9. Troubleshooting
- **ICAP errors**: Check adaptor logs for parse errors; ensure Squid metadata headers present; verify `policy_endpoint` reachable.
- **Redis issues**: Confirm `redis_url` configured; check `redis-cli INFO` for latency; fallback memory cache will emit warnings if Redis unreachable.
- **Policy errors**: 400 from `/api/v1/decision` indicates validation failure; inspect request body for missing `normalized_key`.
- **CLI auth failures**: Ensure config token valid; inspect `~/.odctl/logs` (future) for stack traces.
- **Docker build failures**: Clear `target/` and rebuild; ensure Rust toolchain matches required version.

## 10. Evidence & Reporting
- Keep `rfc/` and `implementation-plan/` documents updated as work progresses.
- Capture test artifacts (`cargo test` logs, smoke results) for Stage 7 signoff.
- Use Kibana dashboards (Stage 6) for SOC/management reporting, export as PDF when requested.

## 11. Support & Escalation
- **First line**: DevOps/SRE on-call monitors health dashboards and alerts.
- **Policy issues**: escalate to Policy Engine team; use simulation endpoint to validate proposed changes.
- **Classification delays**: check Redis Streams queue depth; scale workers as needed.
- **Security incidents**: notify SOC; extract audit events via Admin API or CLI.

Refer to `docs/architecture.md` for system internals and `docs/engine-adaptor-spec.md` + `rfc/` addenda for full requirements. This guide will be updated as each stage delivers new capabilities (UI routes, CLI commands, deployments, etc.).
