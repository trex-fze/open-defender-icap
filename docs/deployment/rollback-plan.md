# Deployment & Rollback Plan (Stage 7)

This document outlines the steps for deploying the Open Defender stack (Docker compose / k8s) and rolling back safely.

## Docker Compose workflow
1. `make compose-up` (build + start all services).
2. Run `tests/integration.sh` to validate the stack.
3. For rollback:
   - `docker compose --env-file .env -f deploy/docker/docker-compose.yml down`
   - Revert to previous tagged image (`git checkout <tag>` and `docker compose --env-file .env -f deploy/docker/docker-compose.yml up -d --build`).
   - Run `tests/integration.sh` again to confirm health.

## Kubernetes (future)
- Deploy manifests via `kubectl apply -k deploy/k8s/overlays/<env>`.
- Use `kubectl rollout status deployment/<svc>` to monitor.
- Rollback via `kubectl rollout undo deployment/<svc>`.

## Runbooks
- `docs/runbooks/icap-adaptor.md` (ICAP restart),
- `docs/runbooks/admin-api.md` (DB migrations + health checks).

## Automation Hooks
- GitHub Actions pipeline (TBD) should execute:
  - `tests/unit.sh`
  - `tests/integration.sh`
  - Optionally `k6 run tests/perf/k6-traffic.js`

Evidence: capture `tests/integration.sh` logs (pre/post deploy) and attach to the Stage 7 bundle.
