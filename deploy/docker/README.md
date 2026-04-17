# Docker / Compose Workflows

This directory contains the compose stacks used for local development, CI-style integration tests, and lightweight smoke validation. Copy `.env.example` to `.env` at the repo root (never commit `.env`) and adjust secrets such as `OD_ADMIN_TOKEN` or `ELASTIC_PASSWORD` before starting the stack.

Canonical env policy:
- Use only `/.env` (repo root) for compose/service runtime values.
- Do not rely on `deploy/docker/.env`; compose commands should pass `--env-file ../../.env` explicitly.
- For the complete variable catalog (runtime + frontend + test controls), see `docs/env-vars-reference.md`.

## Compose files
- `docker-compose.yml`: full developer stack (Redis, Postgres, ICAP adaptor, Policy Engine, Admin API, Squid, Kibana, Elasticsearch, Prometheus, workers, web-admin, odctl runner, **Filebeat + event-ingester** for Stage 6 telemetry). Prometheus loads `prometheus-rules.yml`, including Stage 6 service health alerts plus Stage 24 queue reliability and auth-hardening alerts (pending age, processing stall, DLQ growth, login/refresh failure spikes, lockouts).
- `docker-compose.golden-profiles.yml`: Stage 24 profile overlay defining `golden-local` and `golden-prodlike` service sets.
- `docker-compose.test.yml`: extends the base stack and adds a `smoke-tests` service that runs `odctl smoke` plus basic override listing; also marks heavy services with the `dev` profile so they can be skipped in CI.
- `docker-compose.smoke.yml`: minimal stack (Redis, Postgres, Policy Engine, Admin API, ICAP adaptor, odctl runner, smoke tests) for quick validation.
- **Auth note**: Keep `OD_ADMIN_TOKEN` in `.env` for static token flows, or set `OD_OIDC_ISSUER` / `OD_OIDC_AUDIENCE` / `OD_OIDC_HS256_SECRET` to exercise the OIDC/RBAC guard (ensure issued tokens contain roles such as `policy-admin`, `policy-editor`).
- **Auditing/metrics**: populate `OD_AUDIT_ELASTIC_URL`, `OD_AUDIT_ELASTIC_INDEX`, `OD_AUDIT_ELASTIC_API_KEY` to stream audit events to Elasticsearch, and `OD_REVIEW_SLA_SECONDS` to change the review SLA threshold surfaced via `/metrics`.

## Common commands
```bash
# Start the full developer topology (requires .env)
cd deploy/docker
docker compose --env-file ../../.env up --build

# Tail logs for the ICAP adaptor
docker compose --env-file ../../.env logs -f icap-adaptor

# Run the minimal smoke stack
docker compose --env-file ../../.env -f docker-compose.smoke.yml up --build --abort-on-container-exit

# Execute odctl commands inside the runner container
docker compose --env-file ../../.env run --rm odctl-runner odctl override list

# Stage 24 golden profile verify
PROFILE=golden-local bash ../../tests/ops/golden-profile.sh verify
PROFILE=golden-prodlike bash ../../tests/ops/golden-profile.sh verify
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
make compose-golden-local # Stage 24 golden-local bootstrap + verify
make compose-golden-prodlike # Stage 24 golden-prodlike bootstrap + verify
make compose-golden-down  # Teardown golden profile stacks
```

Run `make gen-certs` once before the first `compose-up`; this generates:
- Squid CA/server certs under `deploy/docker/squid/certs/` (import `ca.pem` in clients using the proxy)
- Web admin TLS cert/key under `deploy/docker/web-admin/certs/` (import `web-admin.pem` for warning-free `https://localhost:19001` access)

## Startup sequence
1. Ensure Docker Desktop/Engine is running and ports 1344, 19000, 19001, 19005, 19010, 3128, 5432, 6379, 9200, 5601, 9090 are free.
2. Copy `.env.example` → `.env` (edit tokens/passwords as needed).
   - Timezone defaults to `OD_TIMEZONE=Asia/Dubai`; keep `OD_REPORTING_TIMEZONE` aligned unless you intentionally want different dashboard bucket timezone.
3. Run `make gen-certs` once to generate Squid and web-admin certificates (`deploy/docker/squid/certs/`, `deploy/docker/web-admin/certs/`).
4. `docker compose --env-file ../../.env up -d postgres redis` and wait for healthchecks, or just run `docker compose --env-file ../../.env up --build` / `make compose-up` to start everything.
5. Run migrations/seeds as needed:
   - Shared DB default (`.env.example`): `docker compose --env-file ../../.env run --rm odctl-runner odctl migrate run admin`
   - Use `odctl migrate run all` only when `OD_ADMIN_DATABASE_URL` and `OD_POLICY_DATABASE_URL` point to different databases.
   - `docker compose --env-file ../../.env run --rm odctl-runner odctl seed policies config/policies.json default compose`
6. Once services are healthy, run `docker compose --env-file ../../.env run --rm odctl-runner odctl smoke icap-adaptor:1344` (already performed automatically in the test/smoke stacks).
7. Access:
    - Admin API: http://localhost:19000/health/ready
    - Policy Engine: http://localhost:19010/health/ready
    - Event Ingester: http://localhost:19100/health/ready
    - Web Admin: https://localhost:19001
    - Kibana: http://localhost:5601 (user `elastic`, password from `.env`)
    - Prometheus: http://localhost:9090
   - Squid proxy: http://localhost:3128 (ICAP wired to adaptor)

## Troubleshooting
- **Build failures (`failed to solve ... open .../data/postgres: permission denied`)**: this is typically bind-mount ownership/permission mismatch on host runtime paths (`data/`, `logs/`) during build context send. Set appropriate ownership/permissions for your environment, then retry `docker compose ... up --build`.
- **Build failures (workspace compile)**: ensure `cargo build --release` succeeds locally; the multi-service image relies on the workspace compiling cleanly.
- **Migration mismatch (`failed to execute policy-engine migrations` with `migration ... missing in the resolved migrations`)**: this happens when `migrate run all` is used against a shared admin/policy database. In shared-DB mode, run `odctl migrate run admin`.
- **Healthcheck retries**: Postgres/Elasticsearch may take >30s on first boot. Check `docker compose logs <service>` and confirm the expected passwords match `.env`.
- **Port conflicts**: adjust published ports by overriding the compose file (e.g., `docker compose -f docker-compose.yml -f overrides.yml up`).
- **Proxy `403` despite reachable `:3128` on Docker Desktop/macOS**: this is often source ACL mismatch caused by Desktop NAT rewrite before HAProxy/Squid evaluate `src`. For dev, set `OD_SQUID_ALLOWED_CLIENT_CIDRS=0.0.0.0/0`, recreate `haproxy` + `squid`, and enforce LAN-only access to `3128` at host/router firewall.
- **odctl errors**: confirm `OD_ADMIN_TOKEN` inside `.env` matches the token configured in `config/admin-api.json` or environment variables.
