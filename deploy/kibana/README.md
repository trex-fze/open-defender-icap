# Kibana Dashboards

Stage 6 ships baseline Kibana saved objects covering the IP/SOC dashboards from Spec §16. Import them via Stack Management → Saved Objects (or run the curl commands below).

## Import via Kibana UI
1. Open http://localhost:5601 → Stack Management → Saved Objects → Import.
2. Select `deploy/kibana/dashboards/ip-analytics.ndjson`.
3. Choose "Automatically overwrite" when prompted so updates from git are applied.
4. The dashboard appears under **Dashboards → Traffic Operations**.

## Import via API
```bash
curl -u elastic:${ELASTIC_PASSWORD:-changeme-elastic} \
  -H 'kbn-xsrf: true' \
  -F file=@deploy/kibana/dashboards/ip-analytics.ndjson \
  http://localhost:5601/api/saved_objects/_import?overwrite=true
```

## Dashboard contents
- **Allow vs Block Trend**: Line chart splitting `recommended_action` over time.
- **Top Blocked Domains**: Horizontal bar chart of blocked URLs in the last 24h.
- Time picker defaults to `Last 24 hours`; adjust as needed.

All saved objects reference the `traffic-events-*` index pattern that is seeded along with the dashboard.
