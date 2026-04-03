# Open Defender ICAP - Fast Testing Deployment Guide

This guide helps operators stand up Open Defender quickly for realistic local testing. It is optimized for fast feedback while still exercising the full proxy-to-classification flow.

## 1) What you need (server-side requirements)

- OS: macOS, Linux, or WSL2 with Docker support
- Tooling:
  - Docker Engine/Desktop + Docker Compose
  - `make`
  - `curl` and `jq`
- Recommended host resources:
  - CPU: 4+ cores
  - Memory: 8 GB minimum (12+ GB preferred)
  - Free disk: 20+ GB
- Default local ports:
  - `3128` (Squid proxy)
  - `1344` (ICAP adaptor)
  - `19000` (Admin API)
  - `19001` (Web Admin)
  - `19010` (Policy Engine)
  - `19100` (Event Ingester)
  - `5601` (Kibana)
  - `9090` (Prometheus)

## 2) Client-side configuration requirements

If you want browser/device traffic to pass through the stack:

1. Configure client proxy settings:
   - HTTP proxy: `localhost:3128`
   - HTTPS proxy: `localhost:3128`
2. Ensure Squid allows your client source IP:
   - The compose testing profile currently allows all client source IPs to avoid Docker Desktop source-IP translation issues during local tests.
   - If Squid does not contain explicit `http_access allow` rules, traffic will fail with `TCP_DENIED/403`.
   - Current local ACL policy in `deploy/docker/squid/squid.conf`:
     - allows all source clients in local test mode
     - allows `CONNECT` only to SSL port `443`
     - denies unsafe ports and then applies final deny rule
   - Security note: keep this profile for local/dev testing only. For shared networks or production, restrict source ACLs to your trusted subnets.
3. Generate and trust the Squid CA certificate:
   - Run `make gen-certs`
   - Import `deploy/docker/squid/certs/ca.pem` into the OS/browser trust store

Without the CA trust step, HTTPS interception tests will show certificate warnings (expected).

For API/script-based validation only (no browser), client proxy setup is optional.

## 3) Request/response flow overview

```mermaid
flowchart LR
    U[Client Browser/curl] -->|HTTP/HTTPS via proxy| SQ[Squid :3128]
    SQ -->|ICAP REQMOD| ICAP[ICAP Adaptor :1344]
    ICAP -->|Policy decision request| PE[Policy Engine :19010]
    PE -->|Action| ICAP

    ICAP -->|Cache read/write| R[(Redis)]
    ICAP -->|Enqueue classification| CS[Redis Stream: classification-jobs]
    ICAP -->|Enqueue page fetch| PS[Redis Stream: page-fetch-jobs]

    CS --> LLW[LLM Worker]
    PS --> PF[Page Fetcher]
    PF --> C4[Crawl4AI]
    PF --> PG1[(Postgres: page_contents)]

    LLW --> PG2[(Postgres: classifications + classification_requests)]
    LLW --> R

    PG1 --> API[Admin API :19000]
    PG2 --> API
    API --> UI[Web Admin :19001]
    API --> CLI[odctl]

    ICAP -->|Allow/Block/ContentPending response| SQ --> U
```

## 4) Server-side configuration checklist

Before first startup:

1. Create env file:
   ```bash
   cp .env.example .env
   ```
2. Generate Squid certs:
   ```bash
   make gen-certs
   ```
3. Verify config files exist and reflect your target test profile:
   - `config/icap.json`
   - `config/policy-engine.json`
   - `config/admin-api.json`
   - `config/llm-worker.json`

Compose defaults the LLM worker to a **real-first** profile: LM Studio at `http://192.168.1.170:1234` with OpenAI (`gpt-4o-mini`) as fallback.
- Ensure the host running docker can reach `192.168.1.170`. If not, set `OPENAI_API_KEY` before running smokes so fallback has credentials.
- The LLM provider smoke test will fail fast if neither the local LM Studio host nor OpenAI credentials are reachable.

## 5) Environment variables and usage details

Core variables used most often:

- `OD_ADMIN_TOKEN`: Admin API and CLI authentication token
- `ELASTIC_PASSWORD`: Elasticsearch credential used by compose services
- `OD_FILEBEAT_SECRET`: shared secret for ingest path validation
- `OD_ADMIN_DATABASE_URL` (or `DATABASE_URL`): Admin API Postgres connection
- `OD_POLICY_DATABASE_URL`: Policy Engine Postgres connection
- `OD_CACHE_REDIS_URL`: Redis URL for cache invalidation
- `OD_CACHE_CHANNEL`: Redis invalidation channel (default `od:cache:invalidate`)
- `OPENAI_API_KEY`: credential used by `openai` fallback provider
- `OD_LOG_DIR`: local worker log root (default compose value `/app/logs`, mounted from host `logs/`)
- `CRAWL4AI_LOG_SUBDIR`: crawl service log subdirectory under `OD_LOG_DIR` (default `crawl4ai`)
- `CRAWL4AI_AUDIT_LOG_FILE`: structured crawl audit file name (default `crawl-audit.jsonl`)
- `CRAWL4AI_APP_LOG_FILE`: crawl service application log file name (default `crawl4ai-service.log`)
- `CRAWL4AI_LOG_MAX_BYTES`: max bytes per crawl log file before rotation (default `20971520`)
- `CRAWL4AI_LOG_BACKUP_COUNT`: rotated log file count to retain (default `10`)

LLM failover safety controls (env overrides for `config/llm-worker.json` routing):

- `OD_LLM_FAILOVER_POLICY`: `safe|aggressive|disabled`
- `OD_LLM_PRIMARY_RETRY_MAX`: retries on primary provider before fallback (default `3`)
- `OD_LLM_PRIMARY_RETRY_BACKOFF_MS`: base retry backoff in ms (default `500`)
- `OD_LLM_PRIMARY_RETRY_MAX_BACKOFF_MS`: max retry backoff in ms (default `5000`)
- `OD_LLM_RETRYABLE_STATUS_CODES`: comma-separated retryable statuses (default `408,429,500,502,503,504`)
- `OD_LLM_FALLBACK_COOLDOWN_SECS`: cooldown after fallback failures (default `30`)
- `OD_LLM_FALLBACK_MAX_PER_MIN`: fallback attempt budget per minute (default `30`)
- `OD_LLM_STALE_PENDING_MINUTES`: enable stale pending online diversion after this many minutes (`0` disables)
- `OD_LLM_STALE_PENDING_ONLINE_PROVIDER`: provider name to use for stale pending diversion (default routing fallback provider)
- `OD_LLM_STALE_PENDING_HEALTH_TTL_SECS`: cache ttl for online provider health checks (default `30`)
- `OD_LLM_STALE_PENDING_MAX_PER_MIN`: separate stale diversion cap per minute (default `10`)
- `OD_LLM_ONLINE_CONTEXT_MODE`: online-provider content mode `required|preferred|metadata_only` (default `required`)
- `OD_LLM_CONTENT_REQUIRED_MODE`: content gating mode `required|auto` (default `auto`)
- `OD_LLM_METADATA_ONLY_ALLOWED_FOR`: metadata-only scope `online|all` (default `all`)
- `OD_LLM_METADATA_ONLY_FETCH_FAILURE_THRESHOLD`: fallback threshold for fetch failures before metadata-only classification (default `2`)
- `OD_LLM_METADATA_ONLY_NO_CONTENT_STATUSES`: terminal fetch statuses treated as no-content targets (default `failed,unsupported,blocked`)
- `OD_LLM_METADATA_ONLY_FORCE_ACTION`: force action when online call runs metadata-only (default `Monitor`)
- `OD_LLM_METADATA_ONLY_MAX_CONFIDENCE`: cap confidence for metadata-only outputs (default `0.4`)
- `OD_LLM_METADATA_ONLY_REQUEUE_FOR_CONTENT`: keep pending row and wait for excerpt after metadata-only persistence (default `true`)
- `OD_PENDING_RECONCILE_ENABLED`: enable background stale pending reconciliation (default `true`)
- `OD_PENDING_RECONCILE_INTERVAL_SECS`: reconcile loop interval in seconds (default `60`)
- `OD_PENDING_RECONCILE_STALE_MINUTES`: age threshold for reconciling stale pending rows (default `10`)
- `OD_PENDING_RECONCILE_BATCH`: max pending rows reconciled per cycle (default `100`)

Recommended local-first profile (current compose defaults):

- `OD_LLM_FAILOVER_POLICY=safe`
- `OD_LLM_STALE_PENDING_MINUTES=0`
- `OD_LLM_CONTENT_REQUIRED_MODE=auto`
- `OD_LLM_METADATA_ONLY_ALLOWED_FOR=all`
- `OD_LLM_METADATA_ONLY_FETCH_FAILURE_THRESHOLD=2`
- `OD_LLM_METADATA_ONLY_NO_CONTENT_STATUSES=failed,unsupported,blocked`
- `OD_LLM_METADATA_ONLY_FORCE_ACTION=Monitor`
- `OD_LLM_METADATA_ONLY_MAX_CONFIDENCE=0.4`

Integration-script performance and reliability controls:

- `INTEGRATION_BUILD=1|0`
  - `1` (default): build images first
  - `0`: reuse existing images for faster reruns
- `INTEGRATION_BUILD_RETRIES` (default `3`): retry attempts for transient build failures
- `INTEGRATION_PRUNE_ON_RETRY=1|0` (default `1`): runs `docker builder prune -f` before retry
- `INTEGRATION_RETRY_DELAY_SECONDS` (default `5`): delay between build retries

Examples:

```bash
INTEGRATION_BUILD=0 tests/integration.sh
```

```bash
INTEGRATION_BUILD=1 INTEGRATION_BUILD_RETRIES=3 tests/integration.sh
```

## 6) Start the project safely

From repo root:

```bash
make compose-up
```

Equivalent direct compose command:

```bash
docker compose -f deploy/docker/docker-compose.yml up --build -d
```

Readiness checks:

```bash
curl -sf http://localhost:19000/health/ready
curl -sf http://localhost:19010/health/ready
curl -sf http://localhost:19100/health/ready
```

Recommended quick validation pass:

```bash
tests/unit.sh
INTEGRATION_BUILD=0 tests/integration.sh
```

## 7) Stop the project safely

Normal shutdown (preserves persistent volumes):

```bash
make compose-down
```

Equivalent:

```bash
docker compose -f deploy/docker/docker-compose.yml down
```

Full reset (destructive; wipes local Postgres/Redis/Elasticsearch data):

```bash
docker compose -f deploy/docker/docker-compose.yml down -v
```

Use `down -v` only when you explicitly need a clean local data state.

## 8) FAQ

### What this project does

- Provides an ICAP decision platform integrated with Squid
- Combines deterministic policy evaluation with asynchronous classification workers
- Supports content-first blocking (`ContentPending`) until page content is fetched and classified
- Exposes management and visibility via API, UI, CLI, logs, and metrics

### What this project does not do

- It is not a generic firewall or endpoint security agent
- It does not replace enterprise IAM/IdP; it integrates with your auth model
- It does not automatically allow first-seen destinations; security-first posture may hold traffic pending classification
- It is not production hardening by default when run with local Compose

### Other common questions

- Why do I see "Site Under Classification"?
  - The destination is in content-first pending state; allow/block verdict is still being produced.
- Why do HTTPS sites show certificate warnings in browser tests?
  - The Squid CA cert is not trusted on the client yet.
- I set macOS proxy to `localhost:3128` but I cannot browse internet. Why?
  - Check Squid ACLs first. Missing `http_access allow` rules cause blanket `TCP_DENIED/403` responses.
  - Check logs with `docker compose -f deploy/docker/docker-compose.yml logs --tail=100 squid`.
  - If you changed `squid.conf`, restart the stack so ACL changes apply.
- How do I run fast repeat tests?
  - Use `INTEGRATION_BUILD=0 tests/integration.sh`.
- How do I force a full clean validation?
  - Use `INTEGRATION_BUILD=1 tests/integration.sh`.
- Build fails with transient Docker metadata errors. What now?
  - Retry with default integration settings (retries + prune-on-retry are enabled by default).
- Where do I inspect pending sites?
  - Use Admin API/UI pending view or `odctl classification pending`.
- How does stale pending diversion behave with OFFLINE + ONLINE models?
  - Primary routing still starts with your configured default provider (usually offline/local).
  - If a key remains `waiting_content` longer than `OD_LLM_STALE_PENDING_MINUTES`, worker can try `OD_LLM_STALE_PENDING_ONLINE_PROVIDER` first, but only when health checks pass.
  - Existing fallback retry/cooldown rules still apply, and stale diversion is separately capped by `OD_LLM_STALE_PENDING_MAX_PER_MIN`.
- How does stale pending diversion behave with ONLINE-only models?
  - Diversion is effectively skipped because the online diversion target is already the primary provider (`provider_is_primary`).
  - The worker continues normal primary processing with existing retry/backoff/failover behavior; no duplicate routing loop is introduced.
- Can I choose whether online providers receive scraped excerpts?
  - Yes. Set `OD_LLM_ONLINE_CONTEXT_MODE` to `required` (always send excerpt), `preferred` (send when available), or `metadata_only` (never send excerpt).
  - In metadata-only mode, guardrails force conservative action/confidence via `OD_LLM_METADATA_ONLY_FORCE_ACTION` and `OD_LLM_METADATA_ONLY_MAX_CONFIDENCE`.
- What if a site is API-like and page content never renders?
  - Set `OD_LLM_CONTENT_REQUIRED_MODE=auto` and keep `OD_LLM_METADATA_ONLY_FETCH_FAILURE_THRESHOLD=2` so repeated terminal fetch failures can fall back to metadata-only classification.
  - Use `OD_LLM_METADATA_ONLY_NO_CONTENT_STATUSES` to tune what counts as terminal content failure.
- What if I run offline-only models?
  - Set `OD_LLM_METADATA_ONLY_ALLOWED_FOR=all` so metadata-only fallback is available to offline providers too.
  - Conservative guardrails still apply (`OD_LLM_METADATA_ONLY_FORCE_ACTION`, `OD_LLM_METADATA_ONLY_MAX_CONFIDENCE`).
- Why does a domain stay in Pending Sites even when it looks inactive?
  - If a prior queue event was missed/restarted, the pending row can become orphaned. Keep `OD_PENDING_RECONCILE_ENABLED=true` so stale rows are auto-healed (re-enqueued or cleared).
- Local LLM is healthy but I still see no local requests — why?
  - Most often the queue is blocked in `waiting_content` (content excerpt not ready), so no provider call happens yet.
  - Recommended baseline for local-first/hybrid deployments:
    - `OD_LLM_CONTENT_REQUIRED_MODE=auto`
    - `OD_LLM_METADATA_ONLY_ALLOWED_FOR=all`
    - `OD_LLM_METADATA_ONLY_FETCH_FAILURE_THRESHOLD=2`
  - This keeps content-aware classification when excerpt exists, but avoids infinite pending loops for API/non-renderable sites.
- How do I understand Crawl4AI failures clearly?
  - Read `logs/crawl4ai/crawl-audit.jsonl` on the host. Each entry contains timestamp, URL, report (`success|failed|blocked`), reason, status code, duration, and error details.
  - `blocked` is used for anti-bot/access-denied style failures (e.g., HTTP 403/captcha/access denied); other crawl failures are labeled `failed`.

## 9) Additional relevant information

- Keep failure artifacts from `tests/artifacts/content-pending/` when debugging classification timing or queue behavior.
- When changing `deploy/docker/squid/squid.conf`, apply safely with:
  1. `docker compose -f deploy/docker/docker-compose.yml down`
  2. `docker compose -f deploy/docker/docker-compose.yml up -d --build`
  3. `docker compose -f deploy/docker/docker-compose.yml run --rm odctl-runner odctl migrate run all`
- If integration fails, isolate by stage:
  1. `odctl smoke --profile compose`
  2. `tests/stage06_ingest.sh`
  3. `tests/page-fetch-flow.sh`
  4. `tests/content-pending-smoke.sh`
- For deployment internals and architecture depth, see:
  - `docs/architecture.md`
  - `docs/user-guide.md`
  - `docs/testing/integration-plan.md`
