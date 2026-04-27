# Docker / Compose Workflows

This directory contains the compose stacks used for local development, CI-style integration tests, and lightweight smoke validation. Use `.env` for development mode and `.env-prod` for production-like mode (never commit either file).

Canonical env policy:
- Use root `/.env` for development mode and root `/.env-prod` for production-like mode.
- Do not rely on `deploy/docker/.env`; compose commands should pass `--env-file .env -f deploy/docker/docker-compose.yml` from repo root.
- For the complete variable catalog (runtime + frontend + test controls), see `docs/env-vars-reference.md`.
- Compose includes a `config-preflight` secret gate. Insecure/default secrets fail startup unless `OD_ALLOW_INSECURE_DEV_SECRETS=true` is explicitly set.

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
docker compose --env-file .env -f deploy/docker/docker-compose.yml up --build

# Start production-like mode (requires .env-prod)
docker compose --env-file .env-prod -f deploy/docker/docker-compose.yml up --build

# Tail logs for the ICAP adaptor
docker compose --env-file .env -f deploy/docker/docker-compose.yml logs -f icap-adaptor

# Run the minimal smoke stack
docker compose --env-file .env -f deploy/docker/docker-compose.smoke.yml up -d --build
docker compose --env-file .env -f deploy/docker/docker-compose.smoke.yml run --rm smoke-tests
docker compose --env-file .env -f deploy/docker/docker-compose.smoke.yml down

# Execute odctl commands inside the runner container
docker compose --env-file .env -f deploy/docker/docker-compose.yml run --rm odctl-runner odctl override list

# Stage 24 golden profile verify
PROFILE=golden-local bash tests/ops/golden-profile.sh verify
PROFILE=golden-prodlike bash tests/ops/golden-profile.sh verify
```

### Helper targets
For convenience, the repo root exposes a `Makefile` wrapper:

```bash
make gen-certs            # Generate Squid CA/server certs under deploy/docker/squid/certs/
make compose-up           # Equivalent to docker compose up --build
make compose-down         # Stops the stack
make compose-smoke        # Runs smoke-tests in the minimal smoke stack
COMPOSE_PROFILES=dev make compose-test  # Runs CI/test stack with dev-profile services available
make MODE=prod preflight # Validates strict secret gate with .env-prod
make MODE=prod start     # Starts stack with .env-prod
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
2. Copy `.env.example` -> `.env` for local development, or `.env-prod.example` -> `.env-prod` for production-like validation.
   - Local dev defaults in `.env.example` require explicit `OD_ALLOW_INSECURE_DEV_SECRETS=true`.
   - For production-like runs, unset `OD_ALLOW_INSECURE_DEV_SECRETS` and provide strong unique secrets.
   - Timezone defaults to `OD_TIMEZONE=Asia/Dubai`; keep `OD_REPORTING_TIMEZONE` aligned unless you intentionally want different dashboard bucket timezone.
   - Set `OD_SQUID_ALLOWED_CLIENT_CIDRS` to include both client LAN CIDR(s) and the Docker bridge CIDR used by HAProxy -> Squid. Example: `OD_SQUID_ALLOWED_CLIENT_CIDRS=192.168.1.0/24,172.18.0.0/16`.
   - Discover the active HAProxy docker-network subnet from repo root:
     ```bash
     HAPROXY_ID=$(docker compose --env-file .env -f deploy/docker/docker-compose.yml ps -q haproxy)
     NET_NAME=$(docker inspect "$HAPROXY_ID" --format '{{range $k, $v := .NetworkSettings.Networks}}{{$k}}{{end}}')
     docker network inspect "$NET_NAME" --format '{{range .IPAM.Config}}{{.Subnet}}{{end}}'
     ```
3. Run `make gen-certs` once to generate Squid and web-admin certificates (`deploy/docker/squid/certs/`, `deploy/docker/web-admin/certs/`).
4. `docker compose --env-file .env -f deploy/docker/docker-compose.yml up -d postgres redis` and wait for healthchecks, or run `docker compose --env-file .env -f deploy/docker/docker-compose.yml up --build` / `make compose-up` for dev mode. For production-like validation use `--env-file .env-prod` or `make MODE=prod start`.
   - `config-preflight` runs first and blocks startup on insecure/missing secrets when development override is not enabled.
   - If Kibana remains in "server is not ready yet", bootstrap a new service token and set it in root `.env`:
     ```bash
     curl -u elastic:${ELASTIC_PASSWORD:-changeme-elastic} -s -X POST \
       "http://localhost:9200/_security/service/elastic/kibana/credential/token/od-stack?pretty"
     ```
     - Set `ELASTICSEARCH_SERVICEACCOUNTTOKEN=<token.value>` in root `.env`.
     - Recreate Kibana: `docker compose --env-file .env -f deploy/docker/docker-compose.yml up -d --force-recreate kibana`.
5. Run migrations/seeds as needed:
    - Shared DB default (`.env.example`): `docker compose --env-file .env -f deploy/docker/docker-compose.yml run --rm odctl-runner odctl migrate run admin`
    - Use `odctl migrate run all` only when `OD_ADMIN_DATABASE_URL` and `OD_POLICY_DATABASE_URL` point to different databases.
6. Once services are healthy, run `docker compose --env-file .env -f deploy/docker/docker-compose.yml run --rm odctl-runner odctl smoke --profile compose` (already performed automatically in the test/smoke stacks).
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
- **Admin API exits with local-auth secret error**: if logs show `OD_LOCAL_AUTH_JWT_SECRET appears to use a default/test value`, generate a strong secret (`openssl rand -base64 48`), set `OD_LOCAL_AUTH_JWT_SECRET` in root `.env`, then restart `admin-api` and `web-admin`.
- **Proxy returns `403` for all requests (`TCP_DENIED/403`)**: verify `OD_SQUID_ALLOWED_CLIENT_CIDRS` includes the HAProxy->Squid Docker bridge subnet in addition to LAN client CIDRs, then recreate proxy services (`docker compose --env-file .env -f deploy/docker/docker-compose.yml up -d --force-recreate squid haproxy`) and confirm `/tmp/squid.generated.conf` contains expected `acl localnet src ...` entries.
- **Kibana not ready (`Unable to retrieve version information` / `.security` auth/index errors)**: create a new service token (`/_security/service/elastic/kibana/credential/token/od-stack`), set `ELASTICSEARCH_SERVICEACCOUNTTOKEN` in root `.env`, and recreate `kibana`.
- **Healthcheck retries**: Postgres/Elasticsearch may take >30s on first boot. Check `docker compose logs <service>` and confirm the expected passwords match `.env`.
- **Port conflicts**: adjust published ports by overriding the compose file (e.g., `docker compose -f docker-compose.yml -f overrides.yml up`).
- **Proxy `403` despite reachable `:3128` on Docker Desktop/macOS**: this is often source ACL mismatch caused by Desktop NAT rewrite before HAProxy/Squid evaluate `src`. For dev, set `OD_SQUID_ALLOWED_CLIENT_CIDRS=0.0.0.0/0`, recreate `haproxy` + `squid`, and enforce LAN-only access to `3128` at host/router firewall.
- **odctl errors**: confirm `OD_ADMIN_TOKEN` inside `.env` matches the token configured in `config/admin-api.json` or environment variables.
