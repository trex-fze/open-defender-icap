# Golden Deployment Profiles

Stage 24 defines two deployment profiles for repeatable bootstrap and verification.

## Profiles

### `golden-local`

Purpose: deterministic local security-path verification with minimal external stack.

Included services:
- core control/data plane: `redis`, `postgres`, `admin-api`, `policy-engine`, `icap-adaptor`
- async workers: `llm-worker`, `page-fetcher`, `reclass-worker`, `crawl4ai`
- proxy path: `haproxy`, `squid`
- local operator extras: `web-admin`, `odctl-runner`, `mock-openai`, `smoke-origin`

Recommended env defaults:
- `OD_AUTH_MODE=local`
- `OD_LOCAL_AUTH_JWT_SECRET=<strong-secret>`
- `OD_LOCAL_AUTH_REFRESH_TTL_SECONDS=604800`
- `OD_IAM_SERVICE_TOKEN_TTL_DAYS=90`
- `OPENAI_API_KEY=` (can be blank with local/mock profile behavior)

### `golden-prodlike`

Purpose: production-like reliability and telemetry validation with full ingest/observability path.

Included services:
- all core and worker services from `golden-local` except UI/mock-only extras
- ingest + observability: `event-ingester`, `elasticsearch`, `kibana`, `prometheus`, `logstash`, `filebeat`

Recommended env defaults:
- `OD_AUTH_MODE=hybrid` or `oidc` (depending on auth integration testing)
- `OD_ELASTIC_URL=http://elasticsearch:9200`
- `OD_FILEBEAT_SECRET=<strong-secret>`
- `ELASTIC_PASSWORD=<strong-secret>`
- `OD_TRUST_PROXY_HEADERS=false` (enable only with strict `OD_TRUSTED_PROXY_CIDRS`)

## One-command bootstrap and verify

Use `tests/ops/golden-profile.sh`:

```bash
# Validate profile wiring only (no containers started)
PROFILE=golden-local DRY_RUN=1 bash tests/ops/golden-profile.sh verify
PROFILE=golden-prodlike DRY_RUN=1 bash tests/ops/golden-profile.sh verify

# Full bootstrap + health checks + smoke
PROFILE=golden-local bash tests/ops/golden-profile.sh verify
PROFILE=golden-prodlike bash tests/ops/golden-profile.sh verify

# Teardown
PROFILE=golden-local bash tests/ops/golden-profile.sh down
PROFILE=golden-prodlike bash tests/ops/golden-profile.sh down
```

Make targets:
- `make compose-golden-local`
- `make compose-golden-prodlike`
- `make compose-golden-down`

## Compose wiring

- Base stack: `deploy/docker/docker-compose.yml`
- Profile overlay: `deploy/docker/docker-compose.golden-profiles.yml`

The overlay assigns services to `golden-local` and `golden-prodlike` without changing existing non-profile compose workflows.
