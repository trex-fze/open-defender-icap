# Stage 7 Evidence Checklist

| Task | Artifact | Location |
| --- | --- | --- |
| S7‑T1 Unit coverage | `tests/unit.sh` console log, `docs/testing/unit-coverage.md` | Attach latest run log under `docs/evidence/stage07/unit-tests.log` |
| S7‑T2 Integration/smoke | `tests/integration.sh` log (odctl smoke + Stage 6 ingest) | Save as `docs/evidence/stage07/integration.log` |
| S7‑T3 Performance | k6 summary from `k6 run tests/perf/k6-traffic.js` | `docs/evidence/stage07/perf-summary.txt` |
| S7‑T4 Security | `tests/security/authz-smoke.sh` output + manual prompt-injection notes | `docs/evidence/stage07/security.log` |
| S7‑T5 Deployment/rollback | Completed checklist per `docs/deployment/rollback-plan.md` plus `docker compose` outputs | `docs/evidence/stage07/rollback.txt` |

Instructions:
1. Run each script/plan and tee the output into the evidence folder (paths listed above).
2. Capture screenshots (Kibana dashboards, Prometheus alerts) referenced in Stage 6 docs and include them in `docs/evidence/stage07/`.
3. Once collected, share the folder with QA/SOC for signoff.
