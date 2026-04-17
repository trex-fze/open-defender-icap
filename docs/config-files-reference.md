# Runtime Config Files Reference

This document is the canonical reference for runtime product configuration files in `config/`, including parameter semantics, precedence, validation behavior, and cross-service coupling.

For environment variables across runtime/frontend/test layers, see `docs/env-vars-reference.md`.

## 1) Configuration Loading Model

All Rust services/workers in this repo load JSON config through `config_core::load_config`.

### 1.1 Base precedence

1. If the configured file path exists (for example `config/icap.json`), the service parses that file.
2. If the file does not exist, the service reads `OD_CONFIG_JSON` and parses it as JSON.
3. If neither provides required fields, startup fails with deserialization or validation errors.

This means `OD_CONFIG_JSON` is a file-absence fallback, not a universal override layer.

### 1.2 Service-level env overlays

Several services apply additional env overlays after reading JSON. In those services, effective precedence is:

`explicit env override` > `JSON field` > `hardcoded default`.

Overlay-heavy services:
- `admin-api`
- `policy-engine`
- `llm-worker` (mainly routing/runtime behavior)
- `page-fetcher` (stream consumer-group controls)

File-centric services (minimal/no env overlay on core fields):
- `icap-adaptor`
- `reclass-worker`

## 2) `config/icap.json` (ICAP Adaptor)

Purpose: front-door ICAP service wiring for policy evaluation, cache behavior, pending tracking, and job publication.

### Parameters

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `host` | string | yes | none | Listen host for ICAP server. |
| `port` | u16 | yes | none | ICAP listen port (default sample `1344`). |
| `preview_size` | usize | no | `4096` | Socket read buffer/preview size; code enforces minimum `1024` on connection handler read buffer allocation (`max(1024)`). |
| `redis_url` | string\|null | no | null | Enables Redis-backed decision cache in addition to in-memory cache. If null, memory cache only. |
| `policy_endpoint` | string\|null | effectively yes | null | Policy API base URL consumed by `PolicyClient`; missing value causes startup error (`policy endpoint required`). |
| `metrics_host` | string | no | `0.0.0.0` | Prometheus exporter bind host. |
| `metrics_port` | u16 | no | `19005` | Prometheus exporter bind port. |
| `cache_channel` | string | no | `od:cache:invalidate` | Redis pub/sub channel for cache invalidation events. |
| `require_content` | bool | no | `true` | If true, eligible decisions are diverted into `ContentPending` workflow. |
| `pending_cache_ttl_seconds` | u64 | no | `60` | Local cache TTL for temporary pending decision path. |
| `job_queue.redis_url` | string | when `job_queue` present | none | Redis for classification job publishing. |
| `job_queue.stream` | string | no | `classification-jobs` | Redis Stream for LLM/reclass classification jobs. |
| `page_fetch_queue.redis_url` | string | when `page_fetch_queue` present | none | Redis for page fetch job publishing. |
| `page_fetch_queue.stream` | string | no | `page-fetch-jobs` | Redis Stream for page fetch jobs. |
| `page_fetch_queue.ttl_seconds` | i32 | no | `21600` | Page-fetch job TTL hint; publisher enforces minimum 60s at send time. |
| `admin_api.base_url` | string | when `admin_api` present | none | Admin API base URL used for pending upsert/clear calls. |
| `admin_api.admin_token` | string\|null | no | null | Optional `X-Admin-Token` for pending endpoints. |
| `canonicalization.tenant_domain_exceptions` | map<string, string[]> | no | `{}` | Tenant-aware exceptions for domain canonicalization. |

### Validation and startup behavior

- No explicit env override layer for core fields.
- Failure conditions:
  - malformed or missing required JSON keys.
  - missing `policy_endpoint` (runtime client init failure).
  - invalid Redis URL in queue/cache sections when enabled.

## 3) `config/policy-engine.json` (Policy Engine)

Purpose: policy evaluation runtime, policy storage mode, and admin-auth resolver integration.

### Parameters

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `api_host` | string | yes | none | Bind host for REST API. |
| `api_port` | u16 | yes | none | Bind port (sample `19010`). |
| `policy_file` | string | yes | none | DSL file path used in file mode and DB bootstrap seeding. |
| `database_url` | string\|null | conditional | null | Enables DB-backed policy mode. If null, file mode is used. |
| `activation_database_url` | string\|null | no | null | Optional DB URL for taxonomy activation profile reads; falls back to policy DB URL when omitted. |
| `admin_token` | string\|null | no | null | Legacy/static admin token path (compatibility layer). |
| `auth.resolver_url` | string\|null | no | `http://localhost:19000/api/v1/iam/whoami` | IAM resolver endpoint used for admin route auth checks. |

Note: `policy-engine` auth settings currently honor `resolver_url`. Extra keys under `auth` in JSON samples are ignored unless the auth settings schema expands.

### Env overrides and aliases

- `OD_POLICY_DATABASE_URL` (preferred) or deprecated alias `DATABASE_URL` when `database_url` is null.
- `OD_TAXONOMY_DATABASE_URL` (preferred) or deprecated alias `OD_ADMIN_DATABASE_URL` when `activation_database_url` is null.
- `OD_IAM_RESOLVER_URL` overrides `auth.resolver_url`.

Deprecated alias usage emits startup warnings.

### Validation and mode behavior

- `api_host` and `policy_file` must be non-empty.
- If `database_url` is absent and `policy_file` does not exist, startup fails.
- DB mode:
  - loads policies from Postgres (or seeds from `policy_file` if empty).
  - applies migrations unless DB is detected shared with admin DB path where table bootstrap guard path is used.
- File mode:
  - evaluates directly from `policy_file`.

## 4) `config/admin-api.json` (Admin API)

Purpose: control-plane API configuration (DB, auth, cache invalidation, reporting/audit backends, taxonomy policy controls).

### Core parameters

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `host` | string | yes | none | Bind host for Admin API. |
| `port` | u16 | yes | none | Bind port (sample `19000`). |
| `database_url` | string\|null | conditional | null | Required effectively at runtime via file or env resolution. |
| `admin_token` | string\|null | no | null | Static token for `X-Admin-Token` compatibility/automation. |
| `redis_url` | string\|null | no | null | Enables cache invalidation publisher and classification stream publisher. |
| `cache_channel` | string\|null | no | null | Pub/sub channel for invalidation events; operational default from env path is `od:cache:invalidate`. |
| `policy_engine_url` | string\|null | no | `http://policy-engine:19010` | Admin->Policy Engine reload/runtime sync calls. |
| `policy_engine_admin_token` | string\|null | no | null | Token sent to Policy Engine admin calls; falls back to `admin_token` if null. |

### `auth` section

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `auth.mode` | `local`\|`hybrid`\|`oidc` | no | `local` | Authentication mode. |
| `auth.static_roles` | string[] | no | `policy-admin`,`policy-editor`,`policy-viewer`,`auditor` | Fallback roles list. |
| `auth.oidc_issuer` | string\|null | no | null | OIDC issuer for external JWT validation in hybrid/oidc mode. |
| `auth.oidc_audience` | string\|null | no | null | Expected OIDC audience. |
| `auth.oidc_hs256_secret` | string\|null | no | null | HS256 secret for JWT validation path. |
| `auth.allow_claim_fallback` | bool | no | `true` | Controls claim-derived fallback behavior. |
| `auth.local_jwt_secret` | string\|null | required in local/hybrid | null | Must be >=32 chars and not test/default pattern. |
| `auth.local_jwt_ttl_seconds` | i64 | no | `3600` | Min clamp is 300 seconds in env-merge path. |
| `auth.max_failed_attempts` | i32 | no | `5` | Min clamp 1. |
| `auth.lockout_seconds` | i64 | no | `900` | Min clamp 30. |
| `auth.refresh_ttl_seconds` | i64 | no | `604800` | Min clamp 600. |
| `auth.refresh_max_sessions` | i64 | no | `5` | Min clamp 1. |

### `audit`, `metrics`, `reporting`, `canonicalization`

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `audit.elastic_url` | string\|null | no | null | Optional audit export target. |
| `audit.index` | string\|null | no | null (`audit-events` in sample) | Index name when audit export enabled. |
| `audit.api_key` | string\|null | no | null | API key for audit export auth. |
| `metrics.review_sla_seconds` | u64 | no | `14400` | SLA threshold surfaced in metrics. |
| `reporting.elastic_url` | string\|null | no | null | Enables reporting queries to Elasticsearch. |
| `reporting.index_pattern` | string | no | `traffic-events-*` | Reporting index pattern. |
| `reporting.api_key` | string\|null | no | null | API key auth option. |
| `reporting.username` | string\|null | no | null | Basic auth username option. |
| `reporting.password` | string\|null | no | null | Basic auth password option. |
| `reporting.default_range` | string | no | `24h` | Default dashboard/reporting range. |
| `reporting.timezone` | string | no | `Asia/Dubai` | Histogram timezone. |
| `canonicalization.tenant_domain_exceptions` | map<string, string[]> | no | `{}` | Tenant/domain canonicalization exception map. |

### Env overrides

High-impact overrides (non-exhaustive):

- DB/auth/cache/policy wiring:
  - `OD_ADMIN_DATABASE_URL` (preferred) or alias `DATABASE_URL`
  - `OD_ADMIN_TOKEN`
  - `OD_CACHE_REDIS_URL`
  - `OD_CACHE_CHANNEL`
  - `OD_POLICY_ENGINE_URL`
  - `OD_POLICY_ADMIN_TOKEN`

- Local/OIDC auth controls:
  - `OD_AUTH_MODE`, `OD_LOCAL_AUTH_JWT_SECRET`, `OD_LOCAL_AUTH_TTL_SECONDS`
  - `OD_LOCAL_AUTH_MAX_FAILED_ATTEMPTS`, `OD_LOCAL_AUTH_LOCKOUT_SECONDS`
  - `OD_LOCAL_AUTH_REFRESH_TTL_SECONDS`, `OD_LOCAL_AUTH_REFRESH_MAX_SESSIONS`
  - `OD_OIDC_ISSUER`, `OD_OIDC_AUDIENCE`, `OD_OIDC_HS256_SECRET`

- Bootstrap and optional integrations:
  - `OD_DEFAULT_ADMIN_PASSWORD` (required only for first local bootstrap with no active local policy-admin)
  - `OD_AUDIT_ELASTIC_URL`, `OD_AUDIT_ELASTIC_INDEX`, `OD_AUDIT_ELASTIC_API_KEY`
  - `OD_REVIEW_SLA_SECONDS`
  - `OD_REPORTING_ELASTIC_*`, `OD_REPORTING_DEFAULT_RANGE`, `OD_REPORTING_TIMEZONE`, `OD_TIMEZONE`
  - `OD_LLM_PROVIDERS_URL`, `OD_PROMETHEUS_URL`, `OD_CLASSIFICATION_STREAM`
  - `OD_ADMIN_CORS_ALLOW_ORIGIN`, `OD_TAXONOMY_MUTATION_ENABLED`

### Validation and startup behavior

- `host` non-empty.
- DB URL must resolve from file or env path, otherwise startup fails.
- Local/hybrid mode requires valid strong `OD_LOCAL_AUTH_JWT_SECRET`.

## 5) `config/llm-worker.json` (LLM Worker)

Purpose: async classification worker runtime, provider routing/failover, pending reconciliation, and metrics.

### Top-level parameters

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `queue_name` | string | yes | none | Consumer identity prefix. |
| `redis_url` | string | yes | none | Redis for stream consume/cache invalidation listener. |
| `cache_channel` | string | yes | none | Redis pub/sub invalidation channel. |
| `stream` | string | no | `classification-jobs` | Classification stream. |
| `page_fetch_stream` | string | no | `page-fetch-jobs` | Stream used for page-fetch enqueue from worker paths. |
| `database_url` | string | yes | none | Postgres connection for classifications/pending/page metadata. |
| `llm_endpoint` | string\|null | legacy | null | Legacy single-provider mode if `providers` absent. |
| `llm_api_key` | string\|null | legacy | null | Legacy key (or `LLM_API_KEY`). |
| `providers` | Provider[] | recommended | `[]` | Modern provider catalog/routing model. |
| `routing` | object | no | all optional | Runtime routing/failover/pending controls. |
| `metrics_host` | string | no | `0.0.0.0` | Metrics bind host. |
| `metrics_port` | u16 | no | `19015` | Metrics bind port. |

### `providers[]` schema

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `name` | string | yes | none | Provider identifier used by routing. |
| `type` | enum | yes | none | `lm_studio`, `ollama`, `vllm`, `openai`, `anthropic`, `openai_compatible`, `custom_json`. |
| `endpoint` | string | yes | none | Completion endpoint URL. |
| `model` | string\|null | no | null | Provider model hint. |
| `timeout_ms` | u64\|null | no | null | Per-provider timeout. |
| `headers` | map<string,string> | no | `{}` | Custom request headers. |
| `api_key` | string\|null | no | null | Inline key (avoid in production files). |
| `api_key_env` | string\|null | no | null | Env var name holding key. |

API key resolution: `api_key` (if non-empty) > `api_key_env` lookup > empty string.

### `routing` schema

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `default` | string\|null | no | first provider | Primary provider name. |
| `fallback` | string\|null | no | null | Fallback provider name. |
| `policy` | `aggressive`\|`safe`\|`disabled` | no | `aggressive` | Can be overridden by env. |
| `primary_retry_max` | usize\|null | no | `3` | Primary retry count. |
| `primary_retry_backoff_ms` | u64\|null | no | `500` | Retry base backoff. |
| `primary_retry_max_backoff_ms` | u64\|null | no | `5000` | Retry max backoff. |
| `retryable_status_codes` | u16[] | no | `[408,429,500,502,503,504]` | Retry-class HTTP status set. |
| `fallback_cooldown_secs` | u64\|null | no | `30` | Cooldown after fallback trip. |
| `fallback_max_per_min` | usize\|null | no | `30` | Fallback budget per minute. |
| `stale_pending_minutes` | u64\|null | no | `0` | `0` disables stale pending diversion. |
| `stale_pending_online_provider` | string\|null | no | null | Required when stale diversion enabled unless `fallback` resolves. |
| `stale_pending_health_ttl_secs` | u64\|null | no | `30` | Health probe cache TTL for stale diversion. |
| `stale_pending_max_per_min` | usize\|null | no | `10` | Rate limit for stale diversion path. |
| `online_context_mode` | `required`\|`preferred`\|`metadata_only` | no | `required` | Controls excerpt dependency behavior for online providers. |
| `metadata_only_force_action` | PolicyAction string | no | `Monitor` | Forced action when metadata-only path is used. |
| `metadata_only_max_confidence` | f32 | no | `0.4` | Clamped to [0,1]. |
| `metadata_only_requeue_for_content` | bool | no | `true` (sample sets false) | Requeue policy when metadata-only classification occurs. |
| `content_required_mode` | `required`\|`auto` | no | `auto` | Content hard requirement strategy. |
| `metadata_only_allowed_for` | `online`\|`all` | no | `all` | Scope of metadata-only allowance. |
| `metadata_only_fetch_failure_threshold` | usize | no | `1` | Min clamp 1. |
| `metadata_only_no_content_statuses` | string[] | no | `failed`,`unsupported`,`blocked` | Lower-cased and normalized at runtime. |
| `pending_reconcile_enabled` | bool\|null | no | `true` | Background stale pending reconciliation. |
| `pending_reconcile_interval_secs` | u64\|null | no | `60` | Min clamp 10. |
| `pending_reconcile_stale_minutes` | u64\|null | no | `10` | Min clamp 1. |
| `pending_reconcile_batch` | usize\|null | no | `100` | Min clamp 1. |
| `requeue_max_attempts` | usize\|null | no | env-only fallback path | Used in job runtime requeue settings. |

### Env overrides

Routing/runtime envs supersede routing JSON:

- Failover/retry: `OD_LLM_FAILOVER_POLICY`, `OD_LLM_PRIMARY_RETRY_MAX`, `OD_LLM_PRIMARY_RETRY_BACKOFF_MS`, `OD_LLM_PRIMARY_RETRY_MAX_BACKOFF_MS`, `OD_LLM_RETRYABLE_STATUS_CODES`, `OD_LLM_FALLBACK_COOLDOWN_SECS`, `OD_LLM_FALLBACK_MAX_PER_MIN`
- Stale pending: `OD_LLM_STALE_PENDING_MINUTES`, `OD_LLM_STALE_PENDING_ONLINE_PROVIDER`, `OD_LLM_STALE_PENDING_HEALTH_TTL_SECS`, `OD_LLM_STALE_PENDING_MAX_PER_MIN`
- Metadata/content policy: `OD_LLM_ONLINE_CONTEXT_MODE`, `OD_LLM_CONTENT_REQUIRED_MODE`, `OD_LLM_METADATA_ONLY_ALLOWED_FOR`, `OD_LLM_METADATA_ONLY_FETCH_FAILURE_THRESHOLD`, `OD_LLM_METADATA_ONLY_NO_CONTENT_STATUSES`, `OD_LLM_METADATA_ONLY_FORCE_ACTION`, `OD_LLM_METADATA_ONLY_MAX_CONFIDENCE`, `OD_LLM_METADATA_ONLY_REQUEUE_FOR_CONTENT`
- Pending reconcile: `OD_PENDING_RECONCILE_ENABLED`, `OD_PENDING_RECONCILE_INTERVAL_SECS`, `OD_PENDING_RECONCILE_STALE_MINUTES`, `OD_PENDING_RECONCILE_BATCH`
- Provider probing/logging/runtime extras: `OD_LLM_PROVIDER_HEALTH_TTL_SECS`, `OD_LLM_PROVIDER_HEALTH_TIMEOUT_MS`, `OD_LOG_DIR`, `OPENAI_API_KEY`, `LLM_API_KEY`
- Stream consumer-group controls: `OD_LLM_STREAM_GROUP`, `OD_LLM_STREAM_CONSUMER`, `OD_LLM_STREAM_GROUP_START_ID`, `OD_LLM_STREAM_DEAD_LETTER`, `OD_LLM_STREAM_CLAIM_IDLE_MS`, `OD_LLM_STREAM_CLAIM_BATCH`

### Validation and startup behavior

- `queue_name`, `redis_url`, `database_url` required.
- Provider catalog must resolve valid default/fallback names.
- Stale pending mode fails startup if enabled without resolvable online provider.

## 6) `config/page-fetcher.json` (Page Fetcher)

Purpose: Crawl4AI fetch-worker behavior, payload sizing, persistence limits, stream handling.

### Parameters

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `queue_name` | string | no | `page-fetcher` | Worker queue identity. |
| `stream` | string | no | `page-fetch-jobs` | Input Redis Stream. |
| `stream_start_id` | string | no | `$` | Read start ID for consumer-group setup. |
| `redis_url` | string | yes | none | Redis address. |
| `crawl_service_url` | string | yes | none | Crawl4AI endpoint. |
| `database_url` | string | yes | none | Postgres URL. |
| `metrics_host` | string | no | `0.0.0.0` | Metrics bind host. |
| `metrics_port` | u16 | no | `19025` | Metrics port. |
| `max_excerpt_chars` | usize | no | `4000` | Stored excerpt cap. |
| `max_html_context_chars` | usize | no | `120000` | Max cleaned text/body context size. |
| `max_html_bytes` | usize | no | `524288` | Raw crawl byte cap. |
| `ttl_seconds` | i32 | no | `21600` | Page content TTL hint. |
| `page_content_max_versions_per_key` | i64 | no | `30` | Must be >=1. |
| `fetch_timeout_seconds` | u64 | no | `60` | Crawl HTTP timeout. |
| `idempotency_ttl_seconds` | u64 | no | `86400` | Job idempotency cache TTL. |
| `database_pool_size` | u32 | no | `5` | SQL pool max connections. |
| `terminal_retry_cooldown_seconds` | u64 | no | `1200` | Retry cooldown for terminal fetch statuses. |
| `blocked_retry_cooldown_seconds` | u64 | no | `14400` | Retry cooldown for blocked statuses. |
| `unsupported_retry_cooldown_seconds` | u64 | no | `21600` | Retry cooldown for unsupported statuses. |
| `unsupported_host_allowlist` | string[] | no | `[]` | Hosts exempted from unsupported-host prefilter. |

### Env-only stream controls

- `OD_PAGE_FETCH_STREAM_GROUP`
- `OD_PAGE_FETCH_STREAM_CONSUMER`
- `OD_PAGE_FETCH_STREAM_GROUP_START_ID`
- `OD_PAGE_FETCH_STREAM_DEAD_LETTER`
- `OD_PAGE_FETCH_STREAM_CLAIM_IDLE_MS`
- `OD_PAGE_FETCH_STREAM_CLAIM_BATCH`

### Validation

- Required non-empty: `queue_name`, `redis_url`, `crawl_service_url`, `database_url`.
- `page_content_max_versions_per_key` must be >=1.

## 7) `config/reclass-worker.json` (Reclass Worker)

Purpose: periodic reclassification planner/dispatcher and optional page-fetch republish behavior.

### Parameters

| Field | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `redis_url` | string | yes | none | Redis connection for stream publishing. |
| `job_stream` | string | yes | none | Classification stream target (typically `classification-jobs`). |
| `database_url` | string | yes | none | Postgres source for refresh planning and metadata. |
| `planner_interval_seconds` | u64 | no | `60` | Internal planner loop interval (runtime uses `max(5)` when building duration). |
| `planner_batch_size` | i64 | no | `200` | Planned rows per iteration. |
| `dispatcher_batch_size` | i64 | no | `200` | Dispatch rows per iteration. |
| `metrics_host` | string | no | `0.0.0.0` | Metrics bind host. |
| `metrics_port` | u16 | no | `19016` | Metrics port. |
| `db_pool_size` | u32 | no | `5` | SQL pool max connections. |
| `page_fetch_queue.redis_url` | string | when block present | none | Redis for optional page-fetch queue publication. |
| `page_fetch_queue.stream` | string | no | `page-fetch-jobs` | Page-fetch stream name. |
| `page_fetch_queue.ttl_seconds` | i32 | no | `21600` | TTL hint; publisher enforces minimum 60s. |

### Env overrides

- No dedicated field-by-field env overlay for core config (aside from generic `OD_CONFIG_JSON` fallback when file absent).

## 8) `config/policies.json` (Policy DSL)

Purpose: policy document schema used by Policy Engine file mode and DB bootstrap seeding.

### Schema

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `version` | string | yes | Policy version label. |
| `rules` | PolicyRule[] | yes | Ordered rule set (priority-driven by evaluator). |
| `rules[].id` | string | yes | Rule identifier. |
| `rules[].description` | string\|null | no | Human-readable description. |
| `rules[].priority` | u32 | yes | Lower priority value is evaluated earlier. |
| `rules[].action` | PolicyAction | yes | `Allow`, `Block`, `Warn`, `Monitor`, `Review`, `RequireApproval`, `ContentPending`. |
| `rules[].conditions` | object | no | Unknown keys rejected (`deny_unknown_fields`). |
| `conditions.domains` | string[]\|null | no | Domain targets. |
| `conditions.categories` | string[]\|null | no | Category labels. |
| `conditions.users` | string[]\|null | no | User IDs. |
| `conditions.groups` | string[]\|null | no | Group IDs. |
| `conditions.source_ips` | string[]\|null | no | Source IP matches. |
| `conditions.risk_levels` | string[]\|null | no | Risk-level matches. |

### How to write policy rules JSON

`config/policies.json` must be a JSON object with this shape:

```json
{
  "version": "2026-04-17-default",
  "rules": [
    {
      "id": "rule-id",
      "description": "Optional description",
      "priority": 10,
      "action": "Block",
      "conditions": {
        "domains": null,
        "categories": null,
        "users": null,
        "groups": null,
        "source_ips": null,
        "risk_levels": null
      }
    }
  ]
}
```

Rules are evaluated by ascending `priority` (smaller number first), and the first matching rule wins.

Default `rules` example:

```json
[
  {
    "id": "block-malware",
    "description": "Block domains tagged as malware",
    "priority": 5,
    "action": "Block",
    "conditions": {
      "domains": null,
      "categories": [
        "Malware / Phishing / Fraud"
      ],
      "users": null,
      "groups": null,
      "source_ips": null,
      "risk_levels": null
    }
  },
  {
    "id": "block-adult-sexual-content",
    "description": "Block Adult / Sexual Content",
    "priority": 50,
    "action": "Block",
    "conditions": {
      "domains": null,
      "categories": [
        "Adult / Sexual Content"
      ],
      "users": null,
      "groups": null,
      "source_ips": null,
      "risk_levels": null
    }
  },
  {
    "id": "allow-default",
    "description": "Allow everything else",
    "priority": 1000,
    "action": "Allow",
    "conditions": {
      "domains": null,
      "categories": null,
      "users": null,
      "groups": null,
      "source_ips": null,
      "risk_levels": null
    }
  }
]
```

Full default `config/policies.json` example:

```json
{
  "version": "2026-04-17-default",
  "rules": [
    {
      "id": "block-malware",
      "description": "Block domains tagged as malware",
      "priority": 5,
      "action": "Block",
      "conditions": {
        "domains": null,
        "categories": ["Malware / Phishing / Fraud"],
        "users": null,
        "groups": null,
        "source_ips": null,
        "risk_levels": null
      }
    },
    {
      "id": "block-adult-sexual-content",
      "description": "Block Adult / Sexual Content",
      "priority": 50,
      "action": "Block",
      "conditions": {
        "domains": null,
        "categories": ["Adult / Sexual Content"],
        "users": null,
        "groups": null,
        "source_ips": null,
        "risk_levels": null
      }
    },
    {
      "id": "allow-default",
      "description": "Allow everything else",
      "priority": 1000,
      "action": "Allow",
      "conditions": {
        "domains": null,
        "categories": null,
        "users": null,
        "groups": null,
        "source_ips": null,
        "risk_levels": null
      }
    }
  ]
}
```

Quick validation command:

```bash
odctl policy validate --file config/policies.json
```

### Operational cautions

- `conditions` uses strict key validation; unknown condition fields hard-fail parsing.
- Keep category values synchronized with canonical taxonomy IDs/names used across runtime flows.

## 9) `config/canonical-taxonomy.json` (Canonical Taxonomy)

Purpose: globally shared category/subcategory ontology loaded by Admin API, Policy Engine, LLM Worker, and Reclass Worker.

### Schema

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `version` | string | yes | Must be non-empty. |
| `source` | string\|null | no | Provenance marker. |
| `categories` | Category[] | yes | Canonical category list. |
| `categories[].id` | string | yes | Unique category ID. |
| `categories[].name` | string | yes | Display name. |
| `categories[].always_enabled` | bool\|null | no | Activation profile hinting. |
| `categories[].subcategories` | Subcategory[] | yes | Must be non-empty per category. |
| `subcategories[].id` | string | yes | Unique within category. |
| `subcategories[].name` | string | yes | Display name. |
| `subcategories[].always_enabled` | bool\|null | no | Activation profile hinting. |

### Hard validation constraints (enforced in code)

- Exactly 41 categories must exist.
- Category IDs must be globally unique.
- Each category must contain at least one subcategory.
- Subcategory IDs must be unique within their parent category.
- Category `unknown-unclassified` must exist.

### Env override

- `OD_CANONICAL_TAXONOMY_PATH` overrides the default file path (`config/canonical-taxonomy.json`) for services that call taxonomy loader from env.

## 10) Cross-File Coupling and Drift Risks

### Must-stay-aligned fields

- `config/icap.json.job_queue.stream`
- `config/llm-worker.json.stream`
- `config/reclass-worker.json.job_stream`

All should refer to the same classification stream for end-to-end queue continuity.

- `config/icap.json.page_fetch_queue.stream`
- `config/page-fetcher.json.stream`
- `config/reclass-worker.json.page_fetch_queue.stream`

All should refer to the same page-fetch stream.

- `cache_channel` between ICAP/Admin/LLM paths should stay aligned to keep invalidation coherent.

- DB URLs across Admin API, Policy Engine, LLM Worker, Page Fetcher, Reclass Worker should reflect intended topology (shared DB vs explicit separation). Mismatches can create policy-control/runtime drift.

### Security-sensitive fields in sample configs

Checked-in sample values include placeholders such as `changeme-*`. Treat these as bootstrap examples only; replace in any non-local deployment.

## 11) Practical Baselines

### Local developer baseline

- Keep JSON files under `config/` mounted read-only in compose.
- Use default stream names unless testing queue isolation.
- Prefer env overrides for secrets/tokens/API keys.

### Production baseline

- Store secrets in secret manager/environment injection; avoid plaintext in JSON files.
- Pin DB and Redis endpoints via deployment env, not repo defaults.
- Keep taxonomy/policy docs under change control and verify schema with `--check-config` startup checks where supported.
- Monitor queue lag and DLQ alerts when adjusting retry/failover knobs.
