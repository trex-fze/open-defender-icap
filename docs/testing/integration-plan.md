# Stage 7 – Integration & Smoke Test Plan

This document explains the docker-compose driven integration suites required for Stage 7 (Spec §27–§28).

## Compose stack
- Base file: `deploy/docker/docker-compose.yml`
- Requirements: Docker Engine/Compose, `.env` with credentials (`OD_ADMIN_TOKEN`, `ELASTIC_PASSWORD`, etc.).
- Services: Redis, Postgres, Policy Engine, Admin API, ICAP adaptor, workers, event-ingester, Filebeat, Elasticsearch, Kibana, Prometheus.

## Test automation
- **Script**: `tests/integration.sh`
  1. `docker compose up -d --build`
  2. Runs `odctl smoke --profile compose` inside the runner container (covers ICAP flows, overrides, review queue, CLI logs).
  3. Executes `tests/stage06_ingest.sh` to validate Filebeat → ingester → Elasticsearch → reporting API path.
  4. Hits service health endpoints to ensure readiness.
  5. Tears down the stack (`docker compose down`).

## Evidence capture
- Store the CI log from `tests/integration.sh` under `docs/evidence/stage07-integration.log` for signoff.
- Record Prometheus/Kibana snapshots if additional verification is performed during the run.

## Manual smoke variants
- Minimal stack: `docker compose -f docker-compose.smoke.yml up --abort-on-container-exit`
- CI stack: `docker compose -f docker-compose.yml -f docker-compose.test.yml up --build`

## Next steps
- Integrate `tests/integration.sh` into CI (GitHub Actions, Jenkins, etc.).
- Add failure triage instructions (collect `docker compose logs`, inspect `prometheus` alerts).
