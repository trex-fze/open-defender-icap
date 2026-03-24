# Stage 8 Evidence – Hybrid AI Support

| Artifact | Command / Path | Notes |
| --- | --- | --- |
| Provider catalog (HTTP) | `docs/evidence/stage08/providers-http.json` | Captured via `docker compose exec odctl-runner curl -s http://llm-worker:19015/providers \\| jq '.'` |
| CLI inspector | `docs/evidence/stage08/providers-cli.txt` | Output from `docker compose exec odctl-runner odctl llm providers --url http://llm-worker:19015/providers` |
| Per-provider metrics | `docs/evidence/stage08/provider-metrics.log` | Snapshot from `docker compose exec odctl-runner bash -lc "curl -s http://llm-worker:19015/metrics"` after enqueueing a test job |
| LM Studio remote health | `curl http://192.168.1.170:1234/healthz` *(run outside repo)* | Document in runbook since primary provider is user-managed LM Studio host |
| Prompt-injection smoke | `docs/evidence/stage08/prompt-smoke.log` | `tests/security/llm-prompt-smoke.sh` against running stack |
| LLM failover smoke | `docs/evidence/stage08/llm-failover.log` | `tests/perf/llm-failover.sh` demonstrating OpenAI fallback |
| Performance evidence | `k6 run tests/perf/k6-traffic.js` (hybrid config) | Attach summary showing latency/error rates |

Store raw logs/screenshots in `docs/evidence/stage08/` (git-ignored if sensitive) before final sign-off.

## Runbook Snippet

1. `docker compose up -d redis postgres policy-engine admin-api mock-openai llm-worker odctl-runner` (inside `deploy/docker/`).
2. Install tooling once inside `odctl-runner`: `docker compose exec odctl-runner bash -lc "apt-get update && apt-get install -y curl jq redis-tools postgresql-client"`.
3. Provider catalog/metrics: use the commands referenced in the table above (they operate entirely inside the compose network).
4. Prompt-injection & failover smokes: run `docker run --rm --network docker_default -v $PWD:/work -w /work rust:1.88 bash -lc "apt-get update && apt-get install -y redis-tools postgresql-client && REDIS_URL=redis://redis:6379 DATABASE_URL=postgres://defender:defender@postgres:5432/defender_admin bash tests/security/llm-prompt-smoke.sh"` and similarly `PRIMARY_SERVICE=mock-openai REDIS_URL=... DATABASE_URL=... bash tests/perf/llm-failover.sh` to exercise fallback behavior.
5. Performance: `docker run --rm --network docker_default -v $PWD:/work -w /work grafana/k6 run tests/perf/k6-traffic.js` (captures throughput/latency in Prometheus).
