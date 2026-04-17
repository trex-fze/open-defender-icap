# Environment Variables Reference

This file tracks environment variables consumed by runtime services, frontend, and test scripts.

- Canonical runtime source: `/.env` (copy from `/.env.example`).
- Standalone frontend source: `web-admin/.env` (copy from `web-admin/.env.example`).
- Do not use `deploy/docker/.env` for normal runtime configuration.

## Runtime core

| Variable | Purpose |
| --- | --- |
| `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` | Postgres bootstrap values for compose. |
| `ELASTIC_PASSWORD`, `ELASTICSEARCH_SERVICEACCOUNTTOKEN` | Elasticsearch/Kibana bootstrap credentials. |
| `OD_ADMIN_TOKEN`, `OD_POLICY_ADMIN_TOKEN` | Admin and policy control-plane shared tokens. |
| `OD_ADMIN_DATABASE_URL`, `OD_POLICY_DATABASE_URL`, `OD_TAXONOMY_DATABASE_URL`, `DATABASE_URL` | Database URLs used by admin/policy services. |
| `OD_ADMIN_URL`, `OD_POLICY_URL`, `OD_POLICY_ENGINE_URL` | Internal service URLs used by tooling and service-to-service calls. |
| `OD_AUTH_MODE` | Admin auth mode (`local`, `hybrid`, `oidc`). |
| `OD_LOCAL_AUTH_JWT_SECRET`, `OD_DEFAULT_ADMIN_PASSWORD` | Local auth bootstrap secrets (`OD_LOCAL_AUTH_JWT_SECRET` must be strong/non-default in local/hybrid mode). |
| `OD_LOCAL_AUTH_TTL_SECONDS`, `OD_LOCAL_AUTH_MAX_FAILED_ATTEMPTS`, `OD_LOCAL_AUTH_LOCKOUT_SECONDS`, `OD_LOCAL_AUTH_REFRESH_TTL_SECONDS`, `OD_LOCAL_AUTH_REFRESH_MAX_SESSIONS` | Local auth access-token/refresh-token and lockout controls. |
| `OD_IAM_SERVICE_TOKEN_TTL_DAYS` | Default service-account token expiry window in days (used when `expires_at` is omitted on create/rotate). |
| `OD_OIDC_ISSUER`, `OD_OIDC_AUDIENCE`, `OD_OIDC_HS256_SECRET` | OIDC/JWT validation parameters. |
| `OD_ADMIN_CORS_ALLOW_ORIGIN` | Browser origin allowed by Admin API CORS middleware (compose default `https://localhost:19001`). |
| `OD_TIMEZONE` | Platform timezone baseline propagated to compose services (`TZ`) and used as default reporting timezone (`Asia/Dubai`). |

## Reporting, ingest, and telemetry

## Local auth secret requirements

- `OD_LOCAL_AUTH_JWT_SECRET` is required when `OD_AUTH_MODE=local` or `OD_AUTH_MODE=hybrid`.
- Use a strong random value (recommended: 32+ characters).
- Do not use placeholder-like values such as `changeme`, `default`, or `test`.
- Generate a strong secret with OpenSSL and set it in root `/.env`:

```bash
openssl rand -base64 48
```

```env
OD_LOCAL_AUTH_JWT_SECRET=<paste-generated-secret>
```

| Variable | Purpose |
| --- | --- |
| `OD_AUDIT_ELASTIC_URL`, `OD_AUDIT_ELASTIC_INDEX`, `OD_AUDIT_ELASTIC_API_KEY` | Admin audit export destination. |
| `OD_REPORTING_ELASTIC_URL`, `OD_REPORTING_INDEX_PATTERN`, `OD_REPORTING_ELASTIC_USERNAME`, `OD_REPORTING_ELASTIC_PASSWORD`, `OD_REPORTING_ELASTIC_API_KEY`, `OD_REPORTING_DEFAULT_RANGE`, `OD_REPORTING_TIMEZONE` | Admin reporting query backend and defaults (`OD_REPORTING_TIMEZONE` controls histogram bucket timezone). |
| `OD_PROMETHEUS_URL` | Prometheus base URL used by Admin API operations telemetry endpoints (`/api/v1/reporting/ops-summary`, `/api/v1/reporting/ops-llm-series`). |
| `OD_REVIEW_SLA_SECONDS` | SLA threshold used by review metrics. |
| `OD_ELASTIC_URL`, `OD_ELASTIC_INDEX_PREFIX`, `OD_ELASTIC_INDEX_PATTERN` | Event-ingester index destination/pattern. |
| `OD_ELASTIC_TEMPLATE_NAME`, `OD_ELASTIC_ILM_NAME`, `OD_ELASTIC_APPLY_TEMPLATES` | Event-ingester template/ILM behavior. |
| `OD_ELASTIC_USERNAME`, `OD_ELASTIC_PASSWORD`, `OD_ELASTIC_API_KEY` | Event-ingester auth to Elasticsearch. |
| `OD_INGEST_ENDPOINT`, `OD_INGEST_BIND`, `OD_INGEST_RETRY_ATTEMPTS` | Ingest endpoint wiring and retry controls. |
| `OD_FILEBEAT_SECRET` | Shared ingest secret between Filebeat and event-ingester. |
| `OD_ENVIRONMENT` | Filebeat environment label (`dev` by default). |
| `OD_LOG`, `RUST_LOG` | Service log level controls. |

## Proxy and traffic identity

| Variable | Purpose |
| --- | --- |
| `OD_HAPROXY_BIND_HOST`, `OD_HAPROXY_BIND_PORT` | Host-published HAProxy listener. |
| `OD_HAPROXY_BACKEND_HOST`, `OD_HAPROXY_BACKEND_PORT`, `OD_HAPROXY_LISTEN_PORT` | HAProxy render template internals. |
| `OD_SQUID_ALLOWED_CLIENT_CIDRS` | Source CIDR allow-list at HAProxy/Squid edge. |
| `OD_TRUST_PROXY_HEADERS` | Enable/disable forwarded-header trust for ingress identity. |
| `OD_TRUSTED_PROXY_CIDRS` | CIDRs allowed to supply trusted forwarded headers. |

## LLM and pending-queue controls

| Variable | Purpose |
| --- | --- |
| `OPENAI_API_KEY`, `LLM_API_KEY` | API key sources for online/legacy provider modes. |
| `OD_LOG_DIR` | Shared log root for worker and crawl logs. |
| `OD_LLM_FAILOVER_POLICY`, `OD_LLM_PRIMARY_RETRY_MAX`, `OD_LLM_PRIMARY_RETRY_BACKOFF_MS`, `OD_LLM_PRIMARY_RETRY_MAX_BACKOFF_MS` | Primary retry/failover behavior. |
| `OD_LLM_RETRYABLE_STATUS_CODES`, `OD_LLM_FALLBACK_COOLDOWN_SECS`, `OD_LLM_FALLBACK_MAX_PER_MIN` | Retryable classes and fallback budgeting. |
| `OD_LLM_PROVIDERS_URL` | Admin API upstream URL for LLM provider catalog proxy (`/api/v1/ops/llm/providers`), defaults to `http://llm-worker:19015/providers`. |
| `OD_LLM_PROVIDER_HEALTH_TTL_SECS`, `OD_LLM_PROVIDER_HEALTH_TIMEOUT_MS` | LLM worker `/providers` health-probe cache TTL and probe timeout used for dashboard provider status. |
| `OD_LLM_STALE_PENDING_MINUTES`, `OD_LLM_STALE_PENDING_ONLINE_PROVIDER`, `OD_LLM_STALE_PENDING_HEALTH_TTL_SECS`, `OD_LLM_STALE_PENDING_MAX_PER_MIN` | Stale-pending online diversion controls. |
| `OD_LLM_ONLINE_CONTEXT_MODE`, `OD_LLM_CONTENT_REQUIRED_MODE`, `OD_LLM_METADATA_ONLY_ALLOWED_FOR` | Context and content-gating modes. |
| `OD_LLM_METADATA_ONLY_FETCH_FAILURE_THRESHOLD`, `OD_LLM_METADATA_ONLY_NO_CONTENT_STATUSES` | Metadata-only fallback trigger tuning. |
| `OD_LLM_METADATA_ONLY_FORCE_ACTION`, `OD_LLM_METADATA_ONLY_MAX_CONFIDENCE`, `OD_LLM_METADATA_ONLY_REQUEUE_FOR_CONTENT` | Metadata-only guardrails and queue behavior. |
| `OD_PENDING_RECONCILE_ENABLED`, `OD_PENDING_RECONCILE_INTERVAL_SECS`, `OD_PENDING_RECONCILE_STALE_MINUTES`, `OD_PENDING_RECONCILE_BATCH` | Background pending reconciliation loop controls. |
| `OD_LLM_JOB_REQUEUE_MAX` | Per-job requeue attempt cap. |
| `OD_LLM_STREAM_GROUP`, `OD_LLM_STREAM_CONSUMER`, `OD_LLM_STREAM_GROUP_START_ID`, `OD_LLM_STREAM_DEAD_LETTER`, `OD_LLM_STREAM_CLAIM_IDLE_MS`, `OD_LLM_STREAM_CLAIM_BATCH` | LLM worker Redis Stream group/claim tuning. |

## Page fetch and Crawl4AI

| Variable | Purpose |
| --- | --- |
| `OD_PAGE_FETCH_REDIS_URL`, `OD_PAGE_FETCH_STREAM`, `OD_PAGE_FETCH_TTL_SECONDS` | Page fetch queue and content TTL controls. |
| `OD_PAGE_FETCH_STREAM_GROUP`, `OD_PAGE_FETCH_STREAM_CONSUMER`, `OD_PAGE_FETCH_STREAM_GROUP_START_ID`, `OD_PAGE_FETCH_STREAM_DEAD_LETTER`, `OD_PAGE_FETCH_STREAM_CLAIM_IDLE_MS`, `OD_PAGE_FETCH_STREAM_CLAIM_BATCH` | Page fetcher Redis Stream group/claim tuning. |
| `OD_PAGE_FETCH_METRICS_URL`, `OD_RECLASS_METRICS_URL`, `OD_ICAP_METRICS_URL`, `OD_CRAWL4AI_HEALTH_URL`, `OD_EVENT_INGESTER_URL`, `OD_KIBANA_STATUS_URL` | Optional admin-api platform-health probe endpoint overrides for non-default deployments. |
| `CRAWL4AI_HEADLESS`, `CRAWL4AI_BROWSER`, `CRAWL4AI_USER_AGENT` | Crawl browser runtime mode and UA. |
| `CRAWL4AI_VIEWPORT_WIDTH`, `CRAWL4AI_VIEWPORT_HEIGHT`, `CRAWL4AI_LOCALE`, `CRAWL4AI_TIMEZONE`, `CRAWL4AI_ACCEPT_LANGUAGE` | Crawl rendering locale and viewport settings. |
| `CRAWL4AI_ENABLE_STEALTH`, `CRAWL4AI_SIMULATE_USER`, `CRAWL4AI_OVERRIDE_NAVIGATOR`, `CRAWL4AI_VERBOSE` | Anti-bot and runtime behavior toggles. |
| `CRAWL4AI_WAIT_UNTIL`, `CRAWL4AI_DELAY_BEFORE_RETURN_HTML` | Crawl wait strategy and extraction delay. |
| `PORT` | Crawl4AI service port when running the Python app directly (default `8085`). |
| `CRAWL4AI_LOG_LEVEL`, `CRAWL4AI_LOG_SUBDIR`, `CRAWL4AI_APP_LOG_FILE`, `CRAWL4AI_AUDIT_LOG_FILE`, `CRAWL4AI_LOG_MAX_BYTES`, `CRAWL4AI_LOG_BACKUP_COUNT` | Crawl4AI logging and rotation controls. |

## Frontend (`web-admin`) variables

| Variable | Purpose |
| --- | --- |
| `VITE_ADMIN_API_URL` | Primary Admin API base URL for browser calls. In compose HTTPS mode, leave empty to use same-origin `/api/*` through nginx. |
| `VITE_ADMIN_API_FALLBACK` | Optional fallback Admin API URL when primary is empty; avoid `http://...` values when UI origin is HTTPS. |
| `VITE_ADMIN_TOKEN_MODE` | Auth header mode (`auto`, `bearer`, `token`) for browser calls. |
| `VITE_LLM_PROVIDERS_URL` | Optional frontend override for direct provider fetch; when unset, dashboard uses Admin API proxy endpoint (`/api/v1/ops/llm/providers`). |
| `VITE_ADMIN_API_PROXY_TARGET` | Vite dev-server proxy target for `/api/*` requests (recommended `http://localhost:19000` outside compose). |
| `VITE_HTTPS_ENABLED`, `VITE_HTTPS_CERT_FILE`, `VITE_HTTPS_KEY_FILE` | Optional Vite dev-server HTTPS enablement with local cert/key paths. |

## Advanced config overrides

| Variable | Purpose |
| --- | --- |
| `OD_CANONICAL_TAXONOMY_PATH` | Override canonical taxonomy JSON file path. |
| `OD_CONFIG_JSON` | JSON payload used by `config-core` when config files are absent. |
| `OD_TAXONOMY_MUTATION_ENABLED` | Temporarily allow taxonomy mutation endpoint (off by default). |
| `OD_OPS_HEALTH_ENABLED`, `OD_OPS_HEALTH_TTL_SECS`, `OD_OPS_HEALTH_TIMEOUT_MS` | Admin-api platform-health aggregation toggle, cache TTL, and per-probe timeout controls. Elasticsearch probe uses `OD_REPORTING_ELASTIC_URL` and `OD_REPORTING_ELASTIC_API_KEY` or (`OD_REPORTING_ELASTIC_USERNAME` + `OD_REPORTING_ELASTIC_PASSWORD`). |

## Test and smoke-script controls (selected)

| Variable | Purpose |
| --- | --- |
| `COMPOSE_FILE`, `COMPOSE_ENV_FILE` | Override compose file/env path in scripts. |
| `ADMIN_API_URL`, `ADMIN_TOKEN`, `ADMIN_BEARER` | API endpoint and auth for smoke scripts. |
| `INTEGRATION_BUILD`, `INTEGRATION_BUILD_RETRIES`, `INTEGRATION_PRUNE_ON_RETRY`, `INTEGRATION_RETRY_DELAY_SECONDS` | Integration script build strategy/retry controls. |
| `PROFILE`, `RUN_ID`, `ARTIFACT_ROOT`, `AUTO_TEARDOWN`, `RELIABILITY_*`, `RUNBOOK_EVIDENCE_FILE` | `tests/release-gate.sh` controls for profile selection, artifact locations, reliability tuning, teardown behavior, and optional manual evidence check. |
| `EXPECTED_CLIENT_IP`, `VERIFY_TRUSTED_XFF_PROMOTION` | Proxy identity validation script controls. |
| `ADMIN_TEST_DATABASE_URL`, `TEST_DOCKER_HOST` | Test-only service integration controls. |
