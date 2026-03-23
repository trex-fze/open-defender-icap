# Stage 6 Evidence – Reporting & Observability

This checklist documents how to capture and archive the artifacts required for the Stage 6 sign‑off (Spec §§16–17 & 33–34). Store redacted screenshots/JSON under `docs/evidence/` before sharing with auditors.

## 1. Kibana dashboard screenshot
1. Import the Stage 6 saved objects from `deploy/kibana/dashboards/ip-analytics.ndjson` (Stack Management → Saved Objects → Import → overwrite).
2. Open **Dashboards → Traffic Operations** and set the time picker to `Last 24 hours`.
3. Capture a screenshot showing the *Allow vs Block Trend* and *Top Blocked Domains* visualizations.
4. Save the image as `docs/evidence/stage06/kibana-traffic.png` (redact tenant/IP data if necessary).

## 2. CLI traffic report sample
Run `odctl report traffic --range 24h --top 5 --json > docs/evidence/stage06/traffic-report-sample.json`. The committed sample (`docs/evidence/traffic-report-sample.json`) demonstrates the expected structure; replace it with live data for formal evidence.

## 3. Prometheus / alert evidence
1. Ensure the compose stack is running (`make compose-up`).
2. Trigger an alert by lowering `cache_hit_ratio` (e.g., set `CACHE_TEST_MODE=miss` on ICAP) or by stopping Filebeat to raise the ingestion failure alert.
3. Verify alerts in the Prometheus UI (`http://localhost:9090/alerts`) and capture a screenshot/log snippet. Store as `docs/evidence/stage06/prom-alerts.txt` or `.png`.
4. Record the relevant log excerpt (e.g., `docker compose logs prometheus --tail=200 | tee docs/evidence/stage06/prometheus-alert.log`).

## 4. Elasticsearch document proof
Use the helper test (`tests/stage06_ingest.sh`) to ingest a synthetic event and confirm it reaches Elasticsearch and `/api/v1/reporting/traffic`. Archive the resulting `_count` response in `docs/evidence/stage06/es-count.json` if required.

> Tip: Keep raw evidence files out of version control if they include customer metadata. The provided sample JSON is anonymized and safe to commit.
