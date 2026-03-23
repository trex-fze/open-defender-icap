# Stage 8 Evidence – Hybrid AI Support

| Artifact | Command / Path | Notes |
| --- | --- | --- |
| Provider catalog (HTTP) | `docs/evidence/stage08/providers-http.json` | Captured via `curl http://localhost:19015/providers | jq` |
| CLI inspector | `docs/evidence/stage08/providers-cli.txt` | Output from `odctl llm providers --url http://llm-worker:19015/providers` |
| Per-provider metrics | `docs/evidence/stage08/provider-metrics.log` | Snapshot from `curl http://localhost:19015/metrics` filtered to `llm_*` series |
| LM Studio remote health | `curl http://192.168.1.170:1234/healthz` *(run outside repo)* | Document in runbook since primary provider is user-managed LM Studio host |
| Prompt-injection smoke | `docs/evidence/stage08/prompt-smoke.log` | `tests/security/llm-prompt-smoke.sh` against running stack |
| LLM failover smoke | `docs/evidence/stage08/llm-failover.log` | `tests/perf/llm-failover.sh` demonstrating OpenAI fallback |
| Performance evidence | `k6 run tests/perf/k6-traffic.js` (hybrid config) | Attach summary showing latency/error rates |

Store raw logs/screenshots in `docs/evidence/stage08/` (git-ignored if sensitive) before final sign-off.
