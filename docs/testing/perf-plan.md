# Stage 7 – Performance Test Plan

We use k6 to validate that the Admin API (policies + reporting endpoints) meets the Stage 7 latency and error-rate targets (Spec §29).

## Scripts
- `tests/perf/k6-traffic.js`: exercises `/api/v1/reporting/traffic` and `/api/v1/policies` with a steady 20 VU load.
- `tests/perf/llm-failover.sh`: optionally stops a local LM Studio container (set `PRIMARY_SERVICE=lmstudio` if using compose); otherwise expect you to disable the primary manually before running.

## Running locally
```bash
BASE_URL=http://localhost:19000 \
ADMIN_TOKEN=changeme-admin \
k6 run tests/perf/k6-traffic.js

# failover smoke (set PRIMARY_SERVICE if compose manages your LM node)
PRIMARY_SERVICE=lmstudio tests/perf/llm-failover.sh
```

Expected thresholds:
- `http_req_failed < 1%`
- `http_req_duration p(95) < 500ms`

## Environment notes
- Requires the docker-compose stack to be running so the Admin API and Elasticsearch/reporting endpoints respond.
- For CI, set `BASE_URL` to the compose service (e.g., `http://admin-api:19000`) and mount the script in the k6 container.

## Evidence
- Capture the k6 summary output and store it under `docs/evidence/stage07/perf/*.txt` as part of S7 signoff.
- Save the failover smoke log to `docs/evidence/stage08/llm-failover.log`.
