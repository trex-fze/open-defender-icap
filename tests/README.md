# Tests

## Stage 6 ingestion smoke (`tests/stage06_ingest.sh`)

This helper requires the Docker compose stack from `deploy/docker` to be running (Elasticsearch, Admin API, event-ingester). It performs the following checks:

1. Sends a synthetic event to `http://localhost:19100/ingest/filebeat` using the Filebeat secret.
2. Confirms the document exists in Elasticsearch by querying `traffic-events-*`.
3. Calls `http://localhost:19000/api/v1/reporting/traffic` to ensure the Admin API exposes the aggregated feed.

Usage:

```bash
cd /path/to/repo
make compose-up               # start stack
tests/stage06_ingest.sh       # requires curl + jq
```

Set `INGEST_URL`, `ELASTIC_URL`, `ELASTIC_USER`, `ELASTIC_PASS`, `ADMIN_URL`, or `INGEST_SECRET` to match your environment.
