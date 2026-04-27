# Infrastructure Config Reference

This document covers deployment and operations configuration under `deploy/` for Docker Compose, proxy/ingest pipeline, observability, and Elastic/Kibana assets.

For service runtime JSON configuration, see `docs/config-files-reference.md`.

## 1) Compose Topology and Layering

## 1.1 Base stack: `deploy/docker/docker-compose.yml`

Primary local/full stack definition. It includes:

- Data/control plane: `redis`, `postgres`, `policy-engine`, `admin-api`, `icap-adaptor`
- Async/AI plane: `llm-worker`, `page-fetcher`, `reclass-worker`, `crawl4ai`
- Proxy path: `haproxy`, `squid`
- Observability: `event-ingester`, `logstash`, `filebeat`, `elasticsearch`, `kibana`, `prometheus`
- Web and CLI tooling: `web-admin`, `odctl-runner`, plus `mock-openai` and `smoke-origin` test helpers

### Key operational traits

- Most services mount `../../config:/app/config:ro`, so runtime config edits are immediate after container restart.
- Persisted state uses bind mounts under `../../data/*` (Postgres, Redis, Elasticsearch, Squid logs, Filebeat state).
- Many defaults are intentionally dev-oriented (`changeme-*`, no TLS inside service mesh, local ports exposed).

## 1.2 Compose overlays and profile selectors

### `deploy/docker/docker-compose.smoke.yml`

- Minimal smoke topology using `extends` from base compose.
- Runs core services plus `smoke-tests` wiring from the test compose.

### `deploy/docker/docker-compose.test.yml`

- Adds `smoke-tests` service (`odctl migrate run admin` + `odctl smoke --profile compose`).
- Moves heavy observability/UI services behind `dev` profile for CI-like lean runs.

### `deploy/docker/docker-compose.integration.yml`

- Adds targeted integration jobs:
  - `odctl-smoke`
  - `ingest-smoke`
- Intended for scripted verification pipelines rather than long-lived service runtime.

### `deploy/docker/docker-compose.golden-profiles.yml`

- Defines service grouping profiles:
  - `golden-local`: core + UI/dev helpers
  - `golden-prodlike`: core + telemetry/observability pipeline

Use with base compose to switch footprint without editing service definitions.

## 2) Compose Runtime Variables to Treat as Control Knobs

The base compose file surfaces many env variables; these are the highest-impact infrastructure knobs:

- Platform identity/time:
  - `OD_TIMEZONE` (propagates to service `TZ` and related defaults)

- Datastores and control-plane:
  - `OD_ADMIN_DATABASE_URL`, `OD_POLICY_DATABASE_URL`, `OD_TAXONOMY_DATABASE_URL`
  - `OD_CACHE_REDIS_URL`, `OD_CACHE_CHANNEL`

- Admin auth and bootstrap:
  - `OD_AUTH_MODE`, `OD_LOCAL_AUTH_JWT_SECRET`, `OD_DEFAULT_ADMIN_PASSWORD`, `OD_ADMIN_TOKEN`

- LLM runtime behavior (injected to `llm-worker` container):
  - Failover/retry/stale-pending/metadata-only controls (`OD_LLM_*`, `OD_PENDING_RECONCILE_*`)

- Ingest pipeline and Elasticsearch auth:
  - `OD_FILEBEAT_SECRET`, `OD_ELASTIC_*`, `ELASTIC_PASSWORD`

- Proxy edge ACL and exposure:
  - `OD_HAPROXY_BIND_HOST`, `OD_HAPROXY_BIND_PORT`
  - `OD_SQUID_ALLOWED_CLIENT_CIDRS`, `OD_TRUSTED_PROXY_CIDRS`

Recommendation: keep these in root `.env` and avoid drift between compose invocations.

## 3) Proxy and Ingress Configuration

## 3.1 HAProxy generator: `deploy/docker/haproxy/render-haproxy-cfg.sh`

Generates runtime HAProxy config from environment:

| Variable | Default | Effect |
| --- | --- | --- |
| `OD_SQUID_ALLOWED_CLIENT_CIDRS` | `192.168.1.0/24` | Comma-separated source ACL list. Empty list causes script failure. |
| `OD_HAPROXY_BACKEND_HOST` | `squid` | Backend host target. |
| `OD_HAPROXY_BACKEND_PORT` | `3128` | Backend port target. |
| `OD_HAPROXY_LISTEN_PORT` | `3128` | Frontend bind port inside container. |

Behavioral implications:

- Requests are denied unless source matches generated `allowed_client` ACL.
- Forwarded headers (`X-Real-IP`, `X-Forwarded-For`, `Forwarded`) are always injected; `CONNECT` adjusts proto handling.

## 3.2 Squid template and renderer

- Template: `deploy/docker/squid/squid.conf`
- Renderer: `deploy/docker/squid/render-squid-conf.sh`

Renderer placeholders:

- `__OD_SQUID_ALLOWED_CLIENT_ACLS__`
- `__OD_SQUID_FOLLOW_XFF_RULES__`

Renderer envs:

| Variable | Default | Effect |
| --- | --- | --- |
| `OD_SQUID_ALLOWED_CLIENT_CIDRS` | `192.168.1.0/24` | Builds `acl localnet src ...`; empty result is fatal. |
| `OD_TRUSTED_PROXY_CIDRS` | empty | If set, emits trusted proxy ACL and `follow_x_forwarded_for allow trusted_proxies`; otherwise denies XFF trust globally. |

Template semantics:

- ICAP reqmod wired to `icap://icap-adaptor:1344/reqmod` with `bypass=1`.
- Standard safe-port and CONNECT guards.
- Access logs written to `/var/log/squid/access.log` in custom format including `Forwarded` and `X-Forwarded-For` headers.

Operational caution: permissive CIDRs (`0.0.0.0/0`) are acceptable only for constrained dev environments with external firewall controls.

## 3.3 Web admin reverse proxy: `deploy/docker/web-admin/nginx.conf`

Key settings:

- TLS listener on `19001` with cert/key from `/etc/nginx/certs/`.
- Security headers enabled (HSTS, X-Content-Type-Options, X-Frame-Options, Referrer-Policy).
- `/api/` is reverse proxied to `http://admin-api:19000/api/`.
- SPA fallback via `try_files ... /index.html`.

Operational caveat: API same-origin assumptions rely on this proxy; custom frontend deployments must preserve equivalent `/api/*` behavior or explicitly set frontend API envs.

## 4) Logging and Ingest Pipeline

## 4.1 Filebeat input: `deploy/docker/filebeat/filebeat.yml`

Captures Squid logs and forwards to Logstash.

Key fields:

- `filebeat.inputs[].paths`: `/var/log/squid/access.log*`
- `ignore_older: 2h`
- Adds metadata:
  - `od.source: squid`
  - `od.environment: ${OD_ENVIRONMENT:dev}`
  - `od.service: squid`
- Output: `logstash:5044`

## 4.2 Logstash forwarder: `deploy/docker/logstash/pipeline/logstash.conf`

Pipeline behavior:

- Input: Beats on `5044`
- Filter: stamps `[od][source]=squid`
- Output: HTTP POST to `${OD_INGEST_ENDPOINT:http://event-ingester:19100}/ingest/filebeat`
- Adds header `X-Filebeat-Secret` from `${OD_FILEBEAT_SECRET:changeme-ingest}`
- Emits debug copy to stdout (`rubydebug` codec)

Failure mode: secret mismatch between Filebeat/Logstash/event-ingester causes ingest rejection.

## 5) Prometheus and Alerting

## 5.1 Scrape config: `deploy/docker/prometheus.yml`

Scrape interval and static targets:

- `icap-adaptor:19005`
- `admin-api:19000/metrics`
- `event-ingester:19100/metrics`
- `llm-worker:19015/metrics`
- `reclass-worker:19016/metrics`
- `page-fetcher:19025/metrics`

Rule file include: `/etc/prometheus/prometheus-rules.yml`.

## 5.2 Alert rules: `deploy/docker/prometheus-rules.yml`

Rule groups and intent:

- `stage6-alerts`
  - Cache hit ratio degradation
  - Squid->ICAP latency p95 breach
  - Event ingest failures
  - Review SLA breach spikes

- `stage8-llm-alerts`
  - Provider failures/timeouts
  - Provider latency p95 > threshold

- `stage24-queue-alerts`
  - Pending age and queue stall conditions
  - DLQ growth for LLM/page-fetch workers

- `stage24-auth-alerts`
  - Elevated login failures
  - Lockout detection
  - Refresh-token failure spikes

Tune `for:` windows and thresholds to your environment before production alert routing.

## 6) Elasticsearch and Kibana Assets

## 6.1 Index template: `deploy/elastic/index-template.json`

Defines index pattern `traffic-events-*` and mapping/settings baseline.

Notable settings:

- `number_of_shards: 1`
- `number_of_replicas: 0` (dev default)
- `refresh_interval: 30s`

Notable mapped fields include `@timestamp`, `trace_id`, `source.ip`, `client.ip`, `destination.domain`, `url.full`, and `recommended_action`.

Production guidance: increase replicas and review shard strategy for retention/query volume.

## 6.2 ILM policy: `deploy/elastic/ilm-policy.json`

Lifecycle phases:

- `hot`: immediate with priority 100
- `warm`: after 30d, force-merge to one segment, priority 50
- `cold`: after 90d, priority 0
- `delete`: after 180d

Adjust retention to legal and storage requirements.

## 6.3 Kibana saved objects

- `deploy/kibana/dashboards/ip-analytics.ndjson`

This file is an import payload, not a service runtime config. It seeds baseline traffic/SOC dashboard objects against `traffic-events-*`.

## 7) Linux Host Kernel and Sysctl Tuning (Linux)

These settings matter most on Linux hosts running Elasticsearch + proxy/log ingest containers under sustained traffic.

## 7.1 Baseline sysctl targets

| Setting | Recommended baseline | Why it matters here |
| --- | --- | --- |
| `vm.max_map_count` | `262144` (minimum for Elasticsearch) | Prevents Elasticsearch bootstrap/runtime mmap failures. |
| `fs.file-max` | `200000`+ | Raises host-wide file descriptor ceiling for proxy/log-heavy workloads. |
| `net.core.somaxconn` | `1024`+ | Improves accept queue headroom during connection bursts. |
| `vm.swappiness` | `1` to `10` | Reduces swap pressure and latency spikes under memory load. |

Apply immediately (until reboot):

```bash
sudo sysctl -w vm.max_map_count=262144
sudo sysctl -w fs.file-max=200000
sudo sysctl -w net.core.somaxconn=1024
sudo sysctl -w vm.swappiness=1
```

Persist across reboot:

```bash
cat <<'EOF' | sudo tee /etc/sysctl.d/99-open-defender.conf
vm.max_map_count=262144
fs.file-max=200000
net.core.somaxconn=1024
vm.swappiness=1
EOF
sudo sysctl --system
```

Verify:

```bash
sysctl vm.max_map_count fs.file-max net.core.somaxconn vm.swappiness
```

## 7.2 Transparent Huge Pages (THP) note

Elasticsearch is generally more stable with THP disabled.

Temporary (until reboot):

```bash
echo never | sudo tee /sys/kernel/mm/transparent_hugepage/enabled
echo never | sudo tee /sys/kernel/mm/transparent_hugepage/defrag
```

Verify THP state:

```bash
cat /sys/kernel/mm/transparent_hugepage/enabled
cat /sys/kernel/mm/transparent_hugepage/defrag
```

Use your distro's startup mechanism (`systemd` unit, `rc.local`, or cloud-init) if you need THP disabled persistently.

## 7.3 Symptom-to-knob quick map

- Elasticsearch bootstrap errors mentioning `max virtual memory areas` -> increase `vm.max_map_count`.
- Frequent `too many open files` in proxy/log components -> raise container `ulimits.nofile` and confirm host `fs.file-max`.
- Burst traffic drops/reset behavior at ingress -> increase `net.core.somaxconn` and check HAProxy/Squid FD limits.
- Latency spikes under memory pressure -> reduce `vm.swappiness` and right-size container heap/memory limits.

## 8) Compose Runtime Tuning Patterns

Keep the base compose file unchanged; place host-specific tuning in an override passed as an extra `-f`.

Example invocation from repo root:

```bash
docker compose --env-file .env -f deploy/docker/docker-compose.yml -f deploy/docker/docker-compose.override.yml up -d
```

## 8.1 Example override snippet

```yaml
services:
  elasticsearch:
    environment:
      ES_JAVA_OPTS: "-Xms1g -Xmx1g"
    ulimits:
      memlock:
        soft: -1
        hard: -1
      nofile:
        soft: 65536
        hard: 65536

  logstash:
    environment:
      LS_JAVA_OPTS: "-Xms512m -Xmx512m"

  kibana:
    environment:
      NODE_OPTIONS: "--max-old-space-size=1024"

  postgres:
    command:
      - postgres
      - -c
      - max_connections=200
      - -c
      - shared_buffers=512MB
      - -c
      - effective_cache_size=1536MB

  haproxy:
    ulimits:
      nofile:
        soft: 65536
        hard: 65536

  squid:
    ulimits:
      nofile:
        soft: 65536
        hard: 65536
```

Tune these values to host capacity:

- Elasticsearch heap (`ES_JAVA_OPTS`) should stay below available RAM after accounting for all other services.
- Logstash and Kibana heap can stay modest for low-volume dev, but should be increased for high ingest/query volume.
- Postgres memory knobs must fit remaining memory budget after Elasticsearch/JVM services.
- `nofile` is a high-impact knob for proxy + log pipeline stability under concurrent traffic.

## 8.2 Runtime verification

Check effective container config:

```bash
docker compose --env-file .env -f deploy/docker/docker-compose.yml -f deploy/docker/docker-compose.override.yml config
```

Check service health and capacity signals:

```bash
curl -s http://localhost:9200/_cluster/health?pretty
curl -s http://localhost:9200/_cat/nodes?v
docker compose --env-file .env -f deploy/docker/docker-compose.yml logs --tail=120 elasticsearch logstash kibana
```

## 9) Infra Drift Checklist

Use this when validating deployment consistency:

- Compose env file source is consistent (`.env` at repo root).
- Proxy ACL envs (`OD_SQUID_ALLOWED_CLIENT_CIDRS`, `OD_TRUSTED_PROXY_CIDRS`) match expected topology.
- `OD_FILEBEAT_SECRET` matches across Filebeat -> Logstash -> event-ingester path.
- Elasticsearch auth and reporting auth values are aligned (`ELASTIC_PASSWORD`, `OD_REPORTING_ELASTIC_*`).
- Stream names across runtime config and compose env still match (`classification-jobs`, `page-fetch-jobs`).
- Prometheus targets correspond to enabled service set/profile.

## 10) Production Hardening Notes

- Replace all `changeme-*` values before deployment.
- Restrict published ports to required interfaces only (especially proxy and data-plane ports).
- Use secrets manager injection for credentials/tokens.
- Enforce TLS and cert trust boundaries where local/dev defaults currently use plaintext inter-service HTTP.
- Version and review infra-config changes with the same rigor as application code (alert thresholds, ACLs, retention, and auth settings materially affect security posture).
