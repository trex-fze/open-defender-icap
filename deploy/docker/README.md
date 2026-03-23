# Docker / Compose Workflows

This directory contains the compose stacks used for local development, CI-style integration tests, and lightweight smoke validation. Copy `.env.example` to `.env` at the repo root (never commit `.env`) and adjust secrets such as `OD_ADMIN_TOKEN` or `ELASTIC_PASSWORD` before starting the stack.

## Compose files
- `docker-compose.yml`: full developer stack (Redis, Postgres, ICAP adaptor, Policy Engine, Admin API, Squid, Kibana, Elasticsearch, Prometheus, workers, web-admin, odctl runner, **Filebeat + event-ingester** for Stage 6 telemetry).
- `docker-compose.test.yml`: extends the base stack and adds a `smoke-tests` service that runs `odctl smoke` plus basic override listing; also marks heavy services with the `dev` profile so they can be skipped in CI.
- `docker-compose.smoke.yml`: minimal stack (Redis, Postgres, Policy Engine, Admin API, ICAP adaptor, odctl runner, smoke tests) for quick validation.
- **Auth note**: Keep `OD_ADMIN_TOKEN` in `.env` for static token flows, or set `OD_OIDC_ISSUER` / `OD_OIDC_AUDIENCE` / `OD_OIDC_HS256_SECRET` to exercise the OIDC/RBAC guard (ensure issued tokens contain roles such as `policy-admin`, `policy-editor`).
- **Auditing/metrics**: populate `OD_AUDIT_ELASTIC_URL`, `OD_AUDIT_ELASTIC_INDEX`, `OD_AUDIT_ELASTIC_API_KEY` to stream audit events to Elasticsearch, and `OD_REVIEW_SLA_SECONDS` to change the review SLA threshold surfaced via `/metrics`.

## Common commands
```bash
# Start the full developer topology (requires .env)
cd deploy/docker
docker compose up --build

# Tail logs for the ICAP adaptor
docker compose logs -f icap-adaptor

# Run the minimal smoke stack
docker compose -f docker-compose.smoke.yml up --build --abort-on-container-exit

# Execute odctl commands inside the runner container
docker compose run --rm odctl-runner odctl override list
```

### Helper targets
For convenience, the repo root exposes a `Makefile` wrapper:

```bash
make gen-certs            # Generate Squid CA/server certs under deploy/docker/squid/certs/
make compose-up           # Equivalent to docker compose up --build
make compose-down         # Stops the stack
make compose-smoke        # Runs the minimal smoke stack (abort on completion)
make compose-test         # Runs the CI/test stack (docker-compose.yml + test overlay)
make compose-logs SERVICE=admin-api   # Tail logs for a specific service
```

Run `make gen-certs` once before the first `compose-up`; import `deploy/docker/squid/certs/ca.pem` into any client trust store hitting the Squid proxy.

## Startup sequence
1. Ensure Docker Desktop/Engine is running and ports 1344, 19000, 19001, 19005, 19010, 3128, 5432, 6379, 9200, 5601, 9090 are free.
2. Copy `.env.example` → `.env` (edit tokens/passwords as needed).
3. Run `make gen-certs` once to generate Squid certificates (stores them under `deploy/docker/squid/certs/`).
4. `docker compose up -d postgres redis` and wait for healthchecks, or just run `docker compose up --build` / `make compose-up` to start everything.
5. Run migrations/seeds as needed:
   - `docker compose run --rm odctl-runner odctl migrate run all`
   - `docker compose run --rm odctl-runner odctl seed policies config/policies.json default compose`
6. Once services are healthy, run `docker compose run --rm odctl-runner odctl smoke icap-adaptor:1344` (already performed automatically in the test/smoke stacks).
7. Access:
    - Admin API: http://localhost:19000/health/ready
    - Policy Engine: http://localhost:19010/health/ready
    - Event Ingester: http://localhost:19100/health/ready
    - Kibana: http://localhost:5601 (user `elastic`, password from `.env`)
    - Prometheus: http://localhost:9090
   - Squid proxy: http://localhost:3128 (ICAP wired to adaptor)

## Troubleshooting
- **Build failures**: ensure `cargo build --release` succeeds locally; the multi-service image relies on the workspace compiling cleanly.
- **Healthcheck retries**: Postgres/Elasticsearch may take >30s on first boot. Check `docker compose logs <service>` and confirm the expected passwords match `.env`.
- **Port conflicts**: adjust published ports by overriding the compose file (e.g., `docker compose -f docker-compose.yml -f overrides.yml up`).
- **odctl errors**: confirm `OD_ADMIN_TOKEN` inside `.env` matches the token configured in `config/admin-api.json` or environment variables.
