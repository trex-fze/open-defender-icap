# Docker Compose Expansion Plan

This document records the design for the next iteration of the containerized dev/test environment. No code has been changed yet; the intent is to align on scope before touching the compose files.

## Goals
- Provide a reproducible local environment that mirrors the architecture in `docs/engine-adaptor-spec.md` §19 (Squid proxy, Redis, Postgres, Elasticsearch/Kibana, Prometheus, Admin API, ICAP adaptor, Policy engine, workers, React UI, CLI runner).
- Support three compose entrypoints: `docker-compose.yml` (developer stack), `docker-compose.test.yml` (CI integration test stack), `docker-compose.smoke.yml` (fast validation used by odctl smoke + health probes).
- Document startup flows, required environment variables/secrets, and validation steps so DevOps/SRE and contributors can run the stack consistently.
- Lay groundwork for automated tests that run inside the compose network (smoke/integration) without yet binding us to a specific CI implementation.

## Proposed Service Matrix
| Service | Compose File(s) | Notes |
| --- | --- | --- |
| `redis` | all | Single instance for cache + queues; persistent volume `redis-data`. |
| `postgres` | all | Stores policies, overrides, review queue; volume `pg-data`. |
| `icap-adaptor` | dev/test | Built from `deploy/docker/rust.Dockerfile`; mounts `config/`. Depends on `redis`, `policy-engine`. |
| `policy-engine` | dev/test | Same image; waits on `postgres`. |
| `admin-api` | dev/test | Waits on `postgres`, `redis`. Publishes cache invalidations. |
| `llm-worker`, `reclass-worker` | dev/test | Placeholders now; will consume Redis streams later. |
| `squid` | dev/test | Alpine Squid image with custom config (binds port 3128, talks ICAP to adaptor). |
| `react-ui` | dev | Node 20-based image running Vite dev server on 19001 (future). |
| `frontend-prod` | test/smoke | Built static assets served by nginx for end-to-end tests. |
| `elasticsearch`, `kibana` | dev/test | Optional for now but stubbed; port 9200/5601, volumes for data/config. |
| `prometheus` | dev/test | Scrapes adaptor/policy/admin metrics; mounts config with scrape targets. |
| `odctl-runner` | test/smoke | Lightweight container (Rust or Debian + odctl binary) used to run migrations, policy imports, smoke tests within the compose network. |

## File Layout & Tooling
1. **`deploy/docker/docker-compose.yml` (dev stack)**
   - Includes full service list above except smoke-only helpers.
   - Defines named volumes for Postgres, Redis, ES, Squid logs.
   - Binds host ports:
     - ICAP adaptor: 1344
     - Admin API: 19000
     - Policy engine: 19010
     - React UI: 19001
     - Kibana: 5601
     - Elasticsearch: 9200
     - Prometheus: 9090
   - Uses `.env` to inject defaults (DB URL, admin token placeholder, cache channel, etc.).
   - `depends_on` with `condition: service_healthy` once healthchecks exist.

2. **`deploy/docker/docker-compose.test.yml` (CI integration)**
   - Extends base service definitions via YAML anchors or Compose `extends`.
   - Replaces React dev server with production build container, disables Kibana/Prometheus for speed (optional, but keep ability to toggle via profiles).
   - Adds `smoke-tests` service that runs `odctl smoke localhost:1344` plus API health checks, failing the compose run if any command exits non-zero.
   - Provides target for `docker compose -f ... up --abort-on-container-exit` in CI.

3. **`deploy/docker/docker-compose.smoke.yml` (local quick validation)**
   - Minimal: Redis, Postgres, ICAP adaptor, Policy engine, Admin API, odctl runner.
   - Designed for `odctl smoke` + override tests; excludes Kibana, Prometheus, UI, workers.
   - Starts quickly and uses in-memory or tmpfs volumes.

4. **`.env` + `.env.example`**
   - `.env.example` checked in with safe defaults (DB URLs pointing to compose services, `OD_ADMIN_TOKEN=changeme`, etc.).
   - Actual `.env` ignored via `.gitignore`.
   - Document additional optional env vars (`OD_CACHE_CHANNEL`, `OD_POLICY_URL`, etc.).

5. **`deploy/docker/README.md`** (to be added after implementation)
   - Explains each compose file, startup commands, cleanup, volume locations, and troubleshooting tips.

## Healthchecks & Tests
- **Redis**: `CMD redis-cli ping` via `healthcheck`.
- **Postgres**: `pg_isready -U defender -d defender`.
- **ICAP adaptor**: custom script hitting `/health/ready` (once HTTP management port added) or simple TCP check for now.
- **Policy engine/Admin API**: curl their `/health/ready` endpoints.
- **Kibana/Elasticsearch**: built-in health endpoints.
- Compose docs describe running:
  1. `docker compose up -d postgres redis` → wait for health.
  2. `docker compose run --rm odctl-runner odctl migrate run all` and `... odctl seed policies config/policies.json default compose`.
  3. `docker compose up icap-adaptor policy-engine admin-api`.
  4. `docker compose run --rm odctl-runner odctl smoke icap-adaptor:1344`.

## Open Questions / Future Work
- Do we need Squid in the initial stack or can ICAP adaptor be called directly? (Assumption: include Squid now to align with spec.)
- When to introduce Elasticsearch/Kibana for real vs. placeholder containers? (Plan: stub in dev/test, allow disabling via `profiles`.)
- Certificate handling now automated via `make gen-certs`; revisit when production CAs/cert rotation are required.

## Next Steps
1. (Done) Create `.env.example`, update `.gitignore` for `.env`.
2. (Done) Add new compose files and `deploy/docker/README.md` matching this plan.
3. (Done) Add helper scripts/make targets (`make compose-up`, `make compose-smoke`) and Squid cert generation script.
4. (Done) Update docs (`docs/user-guide.md`, `docs/architecture.md`) to reference the new flows.
5. (Done) Implement odctl migration/seed commands to automate DB prep in the compose runner.
