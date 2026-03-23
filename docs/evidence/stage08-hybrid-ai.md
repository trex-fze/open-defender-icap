# Stage 8 Evidence – Hybrid AI Support

| Artifact | Command / Path | Notes |
| --- | --- | --- |
| Provider catalog | `curl http://localhost:19015/providers | jq` (attach output) | Shows primary/fallback providers & endpoints |
| CLI inspector | `odctl llm providers --url http://localhost:19015/providers` | Include text output |
| Per-provider metrics | Prometheus screenshot (`llm_provider_*` series) | Capture Grafana/Prometheus UI or `curl` snippet |
| LM Studio overlay | `docker compose -f docker-compose.yml -f docker-compose.lmstudio.yml up -d lmstudio` logs | Demonstrate offline node running |
| Prompt-injection smoke | `tests/security/llm-prompt-smoke.sh` output | Store log under `docs/evidence/stage08/prompt-smoke.log` |
| LLM failover smoke | `tests/perf/llm-failover.sh` | Save console log under `docs/evidence/stage08/llm-failover.log` |
| Performance evidence | `k6 run tests/perf/k6-traffic.js` (hybrid config) | Attach summary showing latency/error rates |

Store raw logs/screenshots in `docs/evidence/stage08/` (git-ignored if sensitive) before final sign-off.
