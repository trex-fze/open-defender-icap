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
4. **Run migrations**: `odctl migrate run all` (or `--target admin|policy`) to apply Postgres schema updates before starting services.
5. **Seed taxonomy**: `odctl taxonomy seed` to populate initial categories (Stage 3+).

## 3. Operating the ICAP Adaptor
- Config file: `config/icap.json` (host/port, preview size, Redis URL, policy endpoint, metrics host/port, cache invalidation channel, optional `job_queue`). `cache_channel` defaults to `od:cache:invalidate` and controls the Redis pub/sub topic used for cache flush notifications. When `job_queue` is configured, the adaptor publishes classification jobs to the specified Redis stream for Stage 4 LLM workers.
- Key env vars: `OD_CONFIG_JSON` for containerized deployments; `RUST_LOG` for logging levels.
- Start service: `cargo run -p icap-adaptor` (dev) or via Docker image built with `deploy/docker/rust.Dockerfile`.
- Monitoring: tail `target/debug/icap-adaptor` logs and scrape Prometheus metrics from `http://<metrics_host>:<metrics_port>/metrics` (default `19005`).

## 4. Using the Policy Engine
- Config file: `config/policy-engine.json` (host/port, DSL path, optional `database_url`, optional `admin_token`, optional `auth.static_roles`). Leave `database_url` as `null` for file-backed mode; set to a Postgres connection string (or export `OD_POLICY_DATABASE_URL`/`DATABASE_URL`) to enable persistent storage. When `admin_token` is set—either in the file or via `OD_POLICY_ADMIN_TOKEN`—policy admin APIs require header `X-Admin-Token` (CLI reads `OD_ADMIN_TOKEN`). The `auth` block controls which roles (`policy-admin`, `policy-editor`, `policy-viewer`) are granted to static tokens.
- Policies reside in `config/policies.json`; `GET /api/v1/policies` lists active rules, `POST /api/v1/policies/reload` hot-reloads from the DSL/DB, `POST /api/v1/policies` (DB mode only) creates a new policy document, and `POST /api/v1/policies/simulate` evaluates a sample request without enforcing it.
- Policy updates: `PUT /api/v1/policies/<policy_id|current>` accepts a JSON body (`version`, `status`, optional `notes`, optional `rules`) and requires the `policy-editor` role. Every create/update stores a snapshot in `policy_versions` for audit/history.
- `cargo run -p policy-engine` starts REST API with `/api/v1/decision` + health endpoints. On startup the service applies migrations in `services/policy-engine/migrations/` and seeds from the DSL file if the database is empty.
- Future operations: manage policies via Admin API/UI/CLI; run simulations for policy changes.

## 5. Admin API & Overrides
- Config file: `config/admin-api.json` controls host/port, optional `database_url`, optional `admin_token`, and cache invalidation wiring (`redis_url`, `cache_channel`). Leave `database_url` as `null` for check-ins, but set either `database_url` in the file or `OD_ADMIN_DATABASE_URL`/`DATABASE_URL` env vars in deployment shells; the service refuses to start without one of these values.
- Cache invalidation: when `redis_url` is configured (or `OD_CACHE_REDIS_URL` env var is set) the Admin API publishes override/review updates to the `cache_channel` (defaults to `od:cache:invalidate`) and deletes the matching Redis keys before returning to the client. Without Redis configured the API logs a warning and skips invalidation, which means cached policy decisions may take up to 5 minutes to expire.
- Admin authentication: set `admin_token` in the config or provide `OD_ADMIN_TOKEN` (the CLI already reads this variable). Requests must include header `X-Admin-Token` when any token is configured.
- OIDC/RBAC: set `OD_OIDC_ISSUER`, `OD_OIDC_AUDIENCE`, and `OD_OIDC_HS256_SECRET` (or configure the `auth` block in `config/admin-api.json`) to validate HS256 JWT bearer tokens. Roles extracted from the token (`policy-admin`, `policy-editor`, `policy-viewer`, `review-approver`, `auditor`) determine access; static tokens inherit the roles listed in `auth.static_roles`.
- Audit logging: every override create/update/delete and review resolution writes to `audit_events` (Postgres) and, when `audit.elastic_url`/`audit.index` (or the `OD_AUDIT_ELASTIC_*` env vars) are set, also ships JSON documents to Elasticsearch for downstream dashboards.
- Service startup: `cargo run -p admin-api` applies migrations in `services/admin-api/migrations/` and exposes overrides + review queue routes under `/api/v1`. Operators can also run inside Docker by adding the same env vars to the container spec.
- Metrics: `GET /metrics` exposes Prometheus gauges/counters for review queue depth and SLA compliance. Configure `metrics.review_sla_seconds` (or `OD_REVIEW_SLA_SECONDS`) to adjust the SLA threshold (default 4 hours).
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
| `odctl policy import <file> [name] [created_by]` | Create a DB-backed policy from a DSL file | Wraps `POST /api/v1/policies`; honors `OD_ADMIN_TOKEN`. |
| `odctl policy update <id|current> <file>` | Update policy metadata/rules with JSON payload | Sends `PUT /api/v1/policies/{id}`; `id` can be `current` to target the active policy. |
| `odctl policy import/export` | Manage policy packages (future) | Depends on Stage 2 completion. |
| `odctl cache lookup/invalidate` | Inspect redis entries (future) | Tied to Stage 3 cache enhancements. |
| `odctl migrate run [admin|policy|all]` | Apply Postgres migrations for admin/policy services | Reads `OD_ADMIN_DATABASE_URL` / `OD_POLICY_DATABASE_URL` unless `--admin-url/--policy-url` provided; runs both when target omitted. |
| `odctl seed policies [file] [name] [created_by]` | Load policy DSL file via Policy API | Defaults to `config/policies.json`, `name=default`; requires admin token. |
| `odctl override update <id> <file>` | PUT override definition | JSON matches Admin API payload; invalidates caches instantly. |
| `odctl review list` | List pending review queue items | Displays status, normalized key, submitter/assignee. |
| `odctl review resolve <id> <file>` | Resolve review item via JSON payload | Wraps `/api/v1/review-queue/{id}/resolve`; triggers cache invalidation. |

Config file location: `~/.odctl/config` (YAML/JSON) storing API endpoints & tokens. Example commands: `odctl smoke 10.0.0.5:1344`, `OD_POLICY_URL=http://localhost:19010 OD_ADMIN_TOKEN=secret odctl policy reload`, `OD_ADMIN_TOKEN=secret odctl policy simulate request.json`.

## 7. React Admin UI (Future)
- Start dev server: `npm install && npm run dev` in `web-admin/` (port 19001).
- Planned routes: Dashboard, IP/User/Device investigations, Policy mgmt, Overrides, Review queue, Reports, Audit, Health, Cache, Reclassification.
- Authentication: OIDC login; RBAC controlling navigation.
- Build: `npm run build`; deploy static assets behind reverse proxy.

## 8. Docker & Compose Workflows
- **Prep**: Copy `.env.example` → `.env`, edit tokens/passwords, then run `make gen-certs` (generates Squid CA/server certs under `deploy/docker/squid/certs/`; import `ca.pem` into any client trust store that should trust the proxy).
- **Local dev**: `make compose-up` (or `docker compose up --build` inside `deploy/docker/`) starts Redis, Postgres, ICAP adaptor, Policy engine, Admin API, Squid, workers, Kibana, Prometheus, React UI, and the `odctl` runner. Use `make compose-logs SERVICE=icap-adaptor` to tail logs quickly.
- **Smoke stack**: `docker compose -f docker-compose.smoke.yml up --build --abort-on-container-exit` spins up only Redis/Postgres/core services plus a smoke-tests container that runs `odctl smoke`.
- **CI/integration**: `docker compose -f docker-compose.yml -f docker-compose.test.yml up --abort-on-container-exit` runs the same smoke harness but can skip heavy services via profiles.
- **Health checks**: `curl http://localhost:19000/health/ready`, `curl http://localhost:19010/health/ready`, `redis-cli -h localhost ping`, `curl http://localhost:5601/status` (Kibana), `curl http://localhost:9090/-/ready` (Prometheus).
- **Shutdown**: `docker compose down` keeps volumes, `docker compose down -v` wipes Postgres/Redis/ES data.
- **Workers**: `llm-worker` consumes the `classification-jobs` Redis Stream defined by `stream`/`queue_name`, calls the configured LLM endpoint, persists verdicts, and exports Prometheus metrics (e.g., `llm_invocations_total`, `llm_jobs_failed_total`) via `metrics_host`/`metrics_port`. `reclass-worker` connects to Postgres using `database_url`, scans `classifications.next_refresh_at`, writes to `reclassification_jobs`, republishes jobs to the same stream, and serves backlog metrics (`reclassification_backlog`) from its metrics endpoint. Keep `redis_url`, `job_stream`, planner/dispatcher batch sizes, and metric ports aligned with your compose deployment.

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
