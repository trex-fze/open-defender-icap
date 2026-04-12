# Open Defender ICAP – User & Operator Guide

This guide targets administrators, SOC analysts, DevOps/SRE, and support engineers who interact with the platform once deployed.

## 1. Personas & Access
- **Gateway Administrators**: Manage Squid config, ICAP adaptor deployment, SSL bump policies.
- **Policy Administrators**: Create/modify policies, maintain domain allow/deny overrides, manage taxonomy activation.
- **SOC Analysts**: Investigate IP/User/Device activity, monitor pending sites, respond to alerts.
- **DevOps/SRE**: Operate services, monitor health/metrics, perform deployments and rollbacks.
- **CLI Power Users**: Use `odctl` for automation (smoke tests, migrations, cache operations).

### 1.1 Authentication
- Admin API/UI use local username/password login by default (`POST /api/v1/auth/login`) and issue bearer tokens.
- If login returns `must_change_password=true`, web-admin forces `/auth/change-password` until password update succeeds.
- OIDC remains optional through `OD_AUTH_MODE=hybrid|oidc` for future enterprise integration.
- Service-to-service communication (ICAP adaptor → policy engine, workers → DB) uses mTLS/service tokens.

## 2. Getting Started
1. **Clone repo** and review `docs/engine-adaptor-spec.md` + `docs/architecture.md` for context.
2. **Install prerequisites**: Rust stable (>=1.80), Node LTS, Docker, docker-compose.
3. **Bootstrap workspace**: `cargo check`, `npm install` inside `web-admin`, then `make compose-up` (or `docker compose --env-file .env -f deploy/docker/docker-compose.yml up --build` from repo root).
4. **Run migrations**: `odctl migrate run all` (or `--target admin|policy`) to apply Postgres schema updates before starting services.
5. **Review canonical taxonomy**: `config/canonical-taxonomy.json` now defines the complete category/subcategory tree (including `advertisements/general-advertising`). Operators only toggle allow/deny state via the Admin UI/API; no CLI seeding is required post-Stage 12.

## 3. Operating the ICAP Adaptor
- Config file: `config/icap.json` (host/port, preview size, Redis URL, policy endpoint, metrics host/port, cache invalidation channel, optional `job_queue`). `cache_channel` defaults to `od:cache:invalidate` and controls the Redis pub/sub topic used for cache flush notifications. When `job_queue` is configured, the adaptor publishes classification jobs to the specified Redis stream for Stage 4 LLM workers.
- Key env vars: `OD_CONFIG_JSON` for containerized deployments; `RUST_LOG` for logging levels.
- Start service: `cargo run -p icap-adaptor` (dev) or via Docker image built with `deploy/docker/rust.Dockerfile`.
- Monitoring: tail `target/debug/icap-adaptor` logs and scrape Prometheus metrics from `http://<metrics_host>:<metrics_port>/metrics` (default `19005`).

## 4. Using the Policy Engine
- Config file: `config/policy-engine.json` (host/port, DSL path, optional `database_url`, optional `admin_token`, optional `auth.static_roles`). Leave `database_url` as `null` for file-backed mode; set to a Postgres connection string (or export `OD_POLICY_DATABASE_URL`/`DATABASE_URL`) to enable persistent storage. Policy admin APIs always require credentials (`Authorization: Bearer` or `X-Admin-Token`), and the IAM resolver enforces effective roles (`policy-admin`, `policy-editor`, `policy-viewer`).
- Policies reside in `config/policies.json`; `GET /api/v1/policies` lists active rules, `POST /api/v1/policies/reload` hot-reloads from the DSL/DB, `POST /api/v1/policies` (DB mode only) creates a new policy document, and `POST /api/v1/policies/simulate` evaluates a sample request without enforcing it (`mode=runtime` by default, `mode=policy_only` optional for pure rule debugging).
- Action semantics on the ICAP hot path: `Allow`/`Monitor` pass through, `Block`/`Warn`/`RequireApproval` block, `Review` blocks with review-specific messaging, and `ContentPending` serves the holding page while pending/job orchestration runs.
- Policy condition keys are strict: use only `domains`, `categories`, `users`, `groups`, `source_ips`, and `risk_levels` (unknown keys are rejected by validation).
- Policy-engine policy mutation/admin endpoints are compatibility-only and emit deprecation headers; use Admin API `/api/v1/policies*` for operator workflows.
- Policy updates: `PUT /api/v1/policies/<policy_id|current>` accepts a JSON body (`version`, `status=draft|archived`, optional `notes`, optional `rules`) and requires the `policy-editor` role. Promotion to active is only via `POST /api/v1/policies/:id/publish` (`policy-admin`). Hard delete is `DELETE /api/v1/policies/:id` (`policy-admin`). Every create/update stores a snapshot in `policy_versions` for audit/history.
- Active-policy continuity guard: disabling (`status=archived`) or deleting the currently active policy is rejected with `409 ACTIVE_POLICY_GUARD`; activate another policy first.
- `cargo run -p policy-engine` starts REST API with `/api/v1/decision` + health endpoints. On startup the service applies migrations in `services/policy-engine/migrations/` and seeds from the DSL file if the database is empty.
- Docker default topology now points both Admin API policy writes and policy-engine runtime reads at `defender_admin` to avoid control-plane/runtime drift unless operators explicitly override `OD_POLICY_DATABASE_URL`.
- When `OD_POLICY_DATABASE_URL` and `OD_ADMIN_DATABASE_URL` resolve to the same database, policy-engine startup uses a shared-DB bootstrap path that ensures required runtime tables (`policies`, `policy_rules`, `policy_versions`, `policy_audit_events`) exist before serving requests.
- Future operations: manage policies via Admin API/UI/CLI; run simulations for policy changes.

## 5. Admin API & Overrides
- Config file: `config/admin-api.json` controls host/port, optional `database_url`, optional `admin_token`, and cache invalidation wiring (`redis_url`, `cache_channel`). Leave `database_url` as `null` for check-ins, but set either `database_url` in the file or `OD_ADMIN_DATABASE_URL`/`DATABASE_URL` env vars in deployment shells; the service refuses to start without one of these values.
- Cache invalidation: when `redis_url` is configured (or `OD_CACHE_REDIS_URL` env var is set) the Admin API publishes override/policy updates to the `cache_channel` (defaults to `od:cache:invalidate`) and deletes matching Redis keys before returning to the client. Without Redis configured the API logs a warning and skips invalidation, which means cached policy decisions may take up to 5 minutes to expire.
- Policy propagation: Admin API policy `create`/`update`/`publish` persists in Postgres, invalidates policy cache, and immediately triggers policy-engine reload. If reload fails, the API returns `502 POLICY_RELOAD_FAILED` (the change remains persisted; operators can retry with `POST /api/v1/policies/reload`).
- Local authentication (default): set `OD_AUTH_MODE=local` and a strong `OD_LOCAL_AUTH_JWT_SECRET` (>=32 chars, non-default). `OD_DEFAULT_ADMIN_PASSWORD` is only required for first bootstrap when no active local `policy-admin` exists.
- Login endpoint: `POST /api/v1/auth/login` with `{ "username": "admin", "password": "..." }`; use returned bearer token for UI/API calls.
- Service-account/static tokens remain valid for automation through `X-Admin-Token`.
- Optional OIDC mode: set `OD_AUTH_MODE=hybrid|oidc` + `OD_OIDC_*` variables to validate external JWTs.
- Audit logging: every override create/update/delete writes to `audit_events` (Postgres) and, when `audit.elastic_url`/`audit.index` (or the `OD_AUDIT_ELASTIC_*` env vars) are set, also ships JSON documents to Elasticsearch for downstream dashboards.
- Override precedence: active domain Allow / Deny overrides are evaluated before classification/policy rules. An override on `mozilla.org` also affects `www.mozilla.org` and deeper subdomains unless a more-specific domain override exists.
- Service startup: `cargo run -p admin-api` applies migrations in `services/admin-api/migrations/` and exposes overrides, pending-classification, taxonomy, and reporting routes under `/api/v1`. Operators can also run inside Docker by adding the same env vars to the container spec.
- Metrics: `GET /metrics` exposes Prometheus counters including `taxonomy_activation_changes_total` that increments whenever operators save checkbox state.
- Health checks: `curl http://localhost:19000/health/ready` (readiness) and `/health/live` (liveness). Use `OD_ADMIN_URL` (default `http://localhost:19000`) to point `odctl override ...` commands at the service.

## 6. CLI (`odctl`) Usage
| Command | Purpose | Notes |
| --- | --- | --- |
| `odctl help` | Display available commands | Lists current subcommands. |
| `odctl health` | Run health checks (future) | Will query backend `/health` endpoints. |
| `odctl smoke [host:port]` | Send sample ICAP REQMOD to adaptor | Defaults to `127.0.0.1:1344`; prints ICAP status line. |
| `odctl policy list` | List draft/active policies via Admin API | Calls `GET /api/v1/policies?include_drafts=true`; requires admin auth token. |
| `odctl policy runtime-sync` | Show control-plane vs runtime policy drift status | Calls `GET /api/v1/policies/runtime-sync`; reports `in_sync` and drift reason when present. |
| `odctl policy create --file policy.json --name "Draft Name" [--version vX]` | Create a new policy draft from DSL document | Validates via `/api/v1/policies/validate`, then creates via `/api/v1/policies`. |
| `odctl policy show <policy-id>` | Show one policy with rules | Calls `GET /api/v1/policies/:id`. |
| `odctl policy history <policy-id>` | Show policy version history table | Calls `GET /api/v1/policies/:id/versions`. |
| `odctl policy pull <policy-id> --file policy.json` | Export current policy detail to DSL file | Writes rules + version to local JSON file. |
| `odctl policy push <policy-id> --file policy.json [--notes ...]` | Update policy draft from DSL file | Validates then `PUT /api/v1/policies/:id`; publish remains separate. |
| `odctl policy publish <policy-id> [--version release-...]` | Promote draft to active policy | Calls `POST /api/v1/policies/:id/publish`; requires `policy-admin` role. |
| `odctl policy delete <policy-id> --yes` | Delete policy (irreversible) | Calls `DELETE /api/v1/policies/:id`; requires explicit confirmation. |
| `odctl policy validate --file policy.json` | Validate policy DSL payload without persisting | Calls `POST /api/v1/policies/validate`. |
| `odctl cache get/delete <normalized_key>` | Inspect or evict cache entries | Calls `GET`/`DELETE /api/v1/cache-entries/:cache_key`. |
| `odctl migrate run [admin|policy|all]` | Apply Postgres migrations for admin/policy services | Reads `OD_ADMIN_DATABASE_URL` / `OD_POLICY_DATABASE_URL` unless `--admin-url/--policy-url` provided; runs both when target omitted. |
| `odctl override export --action allow|block --file <path>` | Export Allow/Deny exchange text bundle | Calls `GET /api/v1/overrides/export?action=...`. |
| `odctl override import --action allow|block --file <path> --mode merge|replace [--dry-run]` | Import Allow/Deny exchange entries | Calls `POST /api/v1/overrides/import`; `replace` apply requires `--yes`. |
| `odctl page show --key <normalized>` | Inspect Crawl4AI excerpts and metadata | Useful when debugging LLM prompts; add `--json` for raw output. |
| `odctl taxonomy show` | View taxonomy activation state | Calls `GET /api/v1/taxonomy`. |
| `odctl taxonomy activation export --file activation.json` | Export editable activation payload | Converts live taxonomy state into activation update format. |
| `odctl taxonomy activation apply --file activation.json --yes` | Apply taxonomy activation profile | Calls `PUT /api/v1/taxonomy/activation`; requires confirmation. |
| `odctl classification list --state all|classified|unclassified` | List classified/unclassified keys | Calls `GET /api/v1/classifications` with cursor pagination. |
| `odctl classification pending` | List sites blocked pending Crawl4AI + LLM verdict | Mirrors `/api/v1/classifications/pending`; shows latest status, base URL, timestamps. In domain-first mode, subdomain requests collapse into canonical `domain:<registered_domain>` pending rows. |
| `odctl classification pending-delete <normalized_key>` | Remove one pending row | Calls `DELETE /api/v1/classifications/:key/pending`. |
| `odctl classification pending-clear --yes` | Remove all pending rows | Calls `DELETE /api/v1/classifications/pending`; destructive. |
| `odctl classification unblock --key <normalized> --action Allow ...` | Manually set a verdict to unblock/deny traffic (legacy/manual endpoint) | Sends `POST /api/v1/classifications/:key/unblock`; requires `policy-editor` role and records reason in audit log. |
| `odctl classification manual-classify <key> --category ... --subcategory ...` | Manual taxonomy classify for pending workflows | Calls `POST /api/v1/classifications/:key/manual-classify`. |
| `odctl classification metadata-classify <key> [--provider ...]` | Queue metadata-only classification with preferred provider | Calls `POST /api/v1/classifications/:key/metadata-classify`; fallback policy still applies. |
| `odctl classification update <key> --category ... --subcategory ...` | Update persisted classification labels | Calls `PATCH /api/v1/classifications/:key`. |
| `odctl classification delete <key> --yes` | Delete classification/pending/page-content state for key | Calls `DELETE /api/v1/classifications/:key`; destructive. |
| `odctl classification export --file bundle.json` | Export domain classifications for backup/share | Calls `GET /api/v1/classifications/export` and writes bundle schema `od-classification-bundle.v1`. |
| `odctl classification import --file bundle.json --dry-run --recompute` | Import a bundle with optional policy recompute | Calls `POST /api/v1/classifications/import`; use `--no-recompute` to trust imported risk/action/confidence values. Invalid rows are rejected and saved to JSONL error file. |
| `odctl classification flush --all --dry-run` | Preview or apply bulk classification flush | Calls `POST /api/v1/classifications/flush`; supports `--all`, `--prefix`, or `--keys-file` scopes. |
| `odctl report dashboard --range 24h --top 10` | Pull dashboard analytics payload | Calls `GET /api/v1/reporting/dashboard` for CLI automation/reporting. |

Smoke verification: run `tests/policy-runtime-smoke.sh` to verify publish -> reload propagation, runtime sync, and decision-path behavior end-to-end in Docker.
| `odctl iam users disable <id>` | Disable IAM user access | Calls `POST /api/v1/iam/users/:id/disable`; protected/last-admin guardrails may return `409`. |
| `odctl iam users enable <id>` | Re-enable a disabled IAM user | Calls `POST /api/v1/iam/users/:id/enable`. |
| `odctl iam users update <id> --username ...` | Edit IAM user identity fields/status | Calls `PUT /api/v1/iam/users/:id`; supports username/email/display/subject/status updates. |
| `odctl iam users delete <id> --yes` | Hard delete IAM user (irreversible) | Calls `DELETE /api/v1/iam/users/:id?hard=true`; blocked for protected/last-admin cases. |
| `odctl iam users set-password <id> --password ...` | Set or reset local user password | Calls `POST /api/v1/iam/users/:id/set-password`. |
| `odctl iam users tokens list <user-id>` | List personal API keys for a user | Calls `GET /api/v1/iam/users/:id/tokens`. |
| `odctl iam users tokens create <user-id> --name ...` | Create user API token (plaintext shown once) | Calls `POST /api/v1/iam/users/:id/tokens`. |
| `odctl iam users tokens revoke <user-id> <token-id> --yes` | Revoke a user API token | Calls `DELETE /api/v1/iam/users/:id/tokens/:token_id`. |
| `odctl iam audit list --limit 100` | Browse IAM audit trail | Calls `GET /api/v1/iam/audit` with cursor pagination. |
| `odctl iam recover-admin-password --username admin --new-password ... --reason ... --yes` | Break-glass local admin password recovery | Uses direct DB access (`OD_ADMIN_DATABASE_URL` or `--admin-db-url`), writes audit event, sets `must_change_password=true`, and invalidates existing local JWT sessions. |

Config file location: `~/.odctl/config` (YAML/JSON) storing API endpoints & tokens. Example commands: `odctl smoke 10.0.0.5:1344`, `OD_POLICY_URL=http://localhost:19010 OD_ADMIN_TOKEN=secret odctl policy reload`, `OD_ADMIN_TOKEN=secret odctl policy simulate request.json`.

## 7. React Admin UI
- Compose default web UI: `https://localhost:19001` served by nginx with TLS (self-signed cert from `make gen-certs`).
- Standalone frontend dev: `npm install && npm run dev` in `web-admin/` (port 19001; Vite proxy forwards `/api/*` to `VITE_ADMIN_API_PROXY_TARGET`).
- In compose HTTPS mode, keep `VITE_ADMIN_API_URL`/`VITE_ADMIN_API_FALLBACK` empty so auth/login and API calls stay on same-origin `/api/*`.
- Routes: Dashboard (Elastic analytics + Prometheus operations telemetry overlay), Investigations, Policies (+ draft create/publish), **Pending Sites** (manual classification with category/subcategory and policy-computed action; per-row metadata-only classify with preferred provider selection and fallback-aware queueing; per-row pending delete plus guarded delete-all queue cleanup; subdomain inputs auto-promote to canonical domain key), **Classifications** (classified/unclassified CRUD management with both `Effective Action` and `Recorded Action` columns), Allow / Deny list (domain + subdomain overrides), Taxonomy (canonical listing with activation toggles and save/reset), Diagnostics (Page Content + Cache tools), Settings (RBAC + CLI audit logs + Classification Exchange import/export/flush + Allow/Deny Exchange line-by-line import/export).
- Dashboard analytics: live graphs for unique clients (`client.ip`), total bandwidth, hourly request/blocked/bandwidth trend, frequently accessed domains, blocked domains, and top blocked requesters by `client.ip`.
- Authentication: local username/password login screen; RBAC controls navigation after token issuance.
- Authentication gate: when `must_change_password=true`, UI redirects to password-change checkpoint before route access.
- Build: `npm run build`; deploy static assets behind reverse proxy.
- Operator runbook and screenshot checklist: `docs/runbooks/stage10-web-admin-operator-runbook.md`.
- Frontend expansion roadmap: see `rfc/stage-10-frontend-management-parity.md` and `implementation-plan/stage-10-frontend-management-parity.md` for full management-feature parity scope.

## 8. Docker & Compose Workflows
- **Prep**: Copy `.env.example` → `.env`, edit tokens/passwords, then run `make gen-certs` (generates Squid CA/server certs under `deploy/docker/squid/certs/` and web-admin TLS cert/key under `deploy/docker/web-admin/certs/`; import `ca.pem` for proxy trust and `web-admin.pem` for browser trust on `https://localhost:19001`). Use root `/.env` as the only compose runtime env file.
- **Local dev**: `make compose-up` (preferred) starts Redis, Postgres, ICAP adaptor, Policy engine, Admin API, Squid, workers, Kibana, Prometheus, React UI, and the `odctl` runner. Direct compose equivalent from repo root: `docker compose --env-file .env -f deploy/docker/docker-compose.yml up --build`. Use `make compose-logs SERVICE=icap-adaptor` to tail logs quickly.
- **Smoke stack**: `docker compose --env-file .env -f deploy/docker/docker-compose.smoke.yml up --build --abort-on-container-exit` spins up only Redis/Postgres/core services plus a smoke-tests container that runs `odctl smoke`.
- **CI/integration**: `docker compose --env-file .env -f deploy/docker/docker-compose.yml -f deploy/docker/docker-compose.test.yml up --abort-on-container-exit` runs the same smoke harness but can skip heavy services via profiles.
- **Health checks**: `curl http://localhost:19000/health/ready`, `curl http://localhost:19010/health/ready`, `redis-cli -h localhost ping`, `curl http://localhost:5601/status` (Kibana), `curl http://localhost:9090/-/ready` (Prometheus).
- **Shutdown**: `make compose-down` (preferred), or `docker compose --env-file .env -f deploy/docker/docker-compose.yml down`; use `... down -v` to wipe Postgres/Redis/ES data.
- **Workers**: `llm-worker` consumes the `classification-jobs` Redis Stream defined by `stream`/`queue_name`, calls the configured LLM endpoint, persists verdicts, and exports Prometheus metrics (e.g., `llm_invocations_total`, `llm_jobs_failed_total`) via `metrics_host`/`metrics_port`. For long-lived `waiting_content` keys, stale-divert controls can prefer an online provider after `requested_at` exceeds `OD_LLM_STALE_PENDING_MINUTES`, while still honoring normal fallback budgets plus a separate stale-divert cap (`OD_LLM_STALE_PENDING_MAX_PER_MIN`). Online-provider context behavior is controlled via `OD_LLM_ONLINE_CONTEXT_MODE=required|preferred|metadata_only`; metadata-only path is guarded by `OD_LLM_METADATA_ONLY_FORCE_ACTION`, `OD_LLM_METADATA_ONLY_MAX_CONFIDENCE`, and `OD_LLM_METADATA_ONLY_REQUEUE_FOR_CONTENT` (recommended default is `false`). For API/non-renderable sites, use `OD_LLM_CONTENT_REQUIRED_MODE=auto` (recommended default) and threshold/status controls (`OD_LLM_METADATA_ONLY_FETCH_FAILURE_THRESHOLD`, `OD_LLM_METADATA_ONLY_NO_CONTENT_STATUSES`) to allow metadata fallback after repeated fetch failures; use `OD_LLM_METADATA_ONLY_ALLOWED_FOR=all` (recommended default) so local/offline primary providers do not stall behind strict content gating. When local output fails JSON/schema checks, the worker attempts online metadata-only verification and, if unavailable/failing, terminalizes classification as `unknown-unclassified / insufficient-evidence` to prevent infinite pending loops. A background pending reconciler (`OD_PENDING_RECONCILE_*`) auto-heals stale `waiting_content` rows by re-enqueuing missing jobs or clearing rows once classified. `reclass-worker` connects to Postgres using `database_url`, scans `classifications.next_refresh_at`, writes to `reclassification_jobs`, republishes jobs to the same stream, and serves backlog metrics (`reclassification_backlog`) from its metrics endpoint. Keep `redis_url`, `job_stream`, planner/dispatcher batch sizes, and metric ports aligned with your compose deployment.
- **Local-first recommendation**: keep `OD_LLM_FAILOVER_POLICY=safe` and set `OD_LLM_STALE_PENDING_MINUTES=0` so online providers are fallback-only, not stale-first. This ensures local LLM handles normal traffic whenever available.
- **Crawl diagnostics logs**: `crawl4ai` writes structured audit lines to `logs/crawl4ai/crawl-audit.jsonl` (via compose bind `../../logs:/app/logs`). Each entry contains UTC timestamp, normalized key, URL, report (`success|failed|blocked`), reason, status code, duration, and truncated error details so operators can explain why keys remain in pending.

## 9. Troubleshooting
- **ICAP errors**: Check adaptor logs for parse errors; ensure Squid metadata headers present; verify `policy_endpoint` reachable.
- **Redis issues**: Confirm `redis_url` configured; check `redis-cli INFO` for latency; fallback memory cache will emit warnings if Redis unreachable.
- **Policy errors**: 400 from `/api/v1/decision` indicates validation failure; inspect request body for missing `normalized_key`.
- **Policies -> Version History shows `Failed to fetch`**: verify Admin API readiness (`curl http://localhost:19000/health/ready`), ensure `OD_ADMIN_CORS_ALLOW_ORIGIN` matches the UI origin (default `https://localhost:19001`), and confirm the browser is using same-origin `/api/*` through nginx. If `VITE_ADMIN_API_URL` is set in HTTPS mode, do not point it to `http://localhost:19000`.
- **CLI auth failures**: Ensure config token valid; inspect `~/.odctl/logs` (future) for stack traces.
- **Forgot local admin password**: run `odctl iam recover-admin-password` from a trusted host with DB access; include incident reason and rotate credentials immediately after regaining access.
- **IAM disable/delete blocked with 409**: `PROTECTED_USER` means target is protected (default local admin). `LAST_ACTIVE_ADMIN` means operation would remove final active `policy-admin`; promote another admin first.
- **Docker build failures**: Clear `target/` and rebuild; ensure Rust toolchain matches required version.
- **Crawl pending unclear**: inspect `logs/crawl4ai/crawl-audit.jsonl` and correlate failing URLs by `normalized_key`; repeated `blocked`/`failed` reasons or terminal `unsupported:dns_unresolvable` in `page_contents` indicate no-content fallback path should be used.
- **Local model produced invalid JSON and key failed**: worker now attempts online metadata-only verification automatically. If online verification is unavailable/fails, classification is terminalized to `unknown-unclassified / insufficient-evidence` (pending is cleared).
- **Need override examples (domain + subdomain behavior)?** See FAQ entries in `README.md` and `docs/fast-testing-deployment.md` for UI + `odctl` examples, including full-domain block (`domain:example.com`) and most-specific subdomain precedence.
- **Why does Pending show `domain:example.com` after browsing `www.example.com`?** Domain-first classification scope is enabled: ICAP deduplicates subdomain traffic into canonical domain keys for pending/classification/content artifacts. Use Allow / Deny subdomain overrides for host-specific exceptions.

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
