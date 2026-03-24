# Open Defender ICAP – Architecture Guide

This document expands on `docs/engine-adaptor-spec.md` with implementation-ready views of the platform architecture. It is intended for architects, senior engineers, DevOps/SRE, and security reviewers.

## 1. Logical Architecture

| Layer | Components | Responsibilities |
| --- | --- | --- |
| **Proxy** | Squid + SSL bump | Client authentication, ICAP invocation, metadata forwarding, base ACLs |
| **Decision Plane** | ICAP adaptor (`svc-icap`), Policy Engine (`svc-policy`) | Normalize requests, evaluate policies, coordinate caches, emit ICAP verdicts |
| **Classification Plane** | LLM Worker, Reclass Worker, Redis Streams | Async classification, reclassification, verdict persistence |
| **Management Plane** | Admin API, React UI, CLI (`odctl`) | Policy/override admin, review queue, reporting, health |
| **Data Plane** | Postgres, Redis, Elasticsearch/Kibana | Durable data, distributed cache, analytics/observability |

### 1.1 Component Interactions
1. **Client → Squid**: HTTP(S) traffic. Squid authenticates users, performs SSL bump, and invokes ICAP REQMOD.
2. **Squid → ICAP adaptor**: ICAP message includes metadata headers (`X-Client-IP`, `X-User`, etc.).
3. **Adaptor**: Parses ICAP, normalizes requests, checks multi-tier cache, queries policy engine when needed, returns ICAP verdict.
4. **Policy Engine**: Evaluates policies (user/IP/category/time/location) and returns `PolicyDecision` with action + metadata.
5. **Async Pipeline**: On cache miss without classification, adaptor enqueues job for LLM worker (future stage). Worker classifies, writes to Postgres, updates Redis, triggers reclassification if needed.
6. **Management Layer**: Admin API exposes overrides, review queue, reporting. UI/CLI consume these APIs. CLI also drives migrations, smoke tests, cache inspection.
7. **Observability**: Structured logs/events shipped to Elasticsearch; metrics exported via Prometheus; Kibana dashboards provide SOC/ops visibility.

```mermaid
flowchart LR
    subgraph Clients
        A[Users/Devices]
    end
    subgraph Proxy Layer
        B[Squid Proxy]
        C[ICAP Adaptor]
    end
    subgraph Decision Plane
        D[Policy Engine]
        E[(Redis Cache)]
        F[(Postgres<br/>classifications/&nbsp;overrides)]
    end
    subgraph Classification & Fetch
        STREAM[Redis Streams<br/>classification-jobs]
        G[LLM Worker]
        H[Reclass Worker]
        PF[Page Fetcher]
        CRAWL[Crawl4AI]
        PAGE[(Postgres<br/>page_contents)]
        PEND[(Postgres<br/>classification_requests)]
    end
    subgraph Management Plane
        I[Admin API]
        J[React UI]
        K[odctl CLI]
        EI[Event Ingester]
        FB[Filebeat]
    end
    subgraph Observability
        L[(Elasticsearch/Kibana)]
        M[Prometheus]
    end

    A --> B -->|ICAP REQMOD| C -->|PolicyDecisionRequest| D
    D -->|PolicyDecision| C
    C -->|Cache lookup| E
    D -->|Persist verdict| F
    C -->|Enqueue job| STREAM --> G & H
    G -->|Pending record| PEND
    G -->|Verdict update| F
    G -->|Cache update| E
    H -->|TTL refresh| STREAM
    H -->|Override writes| F
    FB --> EI -->|Telemetry| L
    C -->|Events| L
    D -->|Events| L
    EI -->|Page fetch job| PF -->|HTTP crawl| CRAWL --> PF
    PF -->|Store excerpt| PAGE --> I
    F --> I
    PEND --> I
    I --> J
    I --> K
    I --> D
    C -->|Metrics| M
    G -->|Metrics| M
    EI -->|Metrics| M
    PF -->|Metrics| M
```

## 2. Detailed Component Views

### 2.1 ICAP Adaptor (`svc-icap`)
- **Inputs**: ICAP REQMOD messages with embedded HTTP request; metadata headers.
- **Submodules**:
  - `icap` parser – RFC 3507 compliant.
  - `normalizer` – domain/url canonicalization (RFC 3986/5890).
  - `cache` – in-memory/Tokio RwLock + Redis client for distributed cache.
  - `policy_client` – `reqwest` HTTP client to Policy Engine API.
  - Future: `queue_publisher`, `override_lookup`, `audit_emitter`.
- **Outputs**: ICAP responses (204 for allow/monitor, 200 with 403 body for block/warn/review).
- **Metrics**: `squid_to_icap_latency`, `cache_hit_ratio`, `policy_decision_latency`, `llm_invocation_count` (future), etc.

### 2.2 Policy Engine (`svc-policy`)
- **Current State**: Axum service exposing `/api/v1/decision` plus admin endpoints.
- **Current Enhancements**: Loads policy DSL from `config/policies.json`, exposes `/api/v1/policies` (list) and `/api/v1/policies/reload` to refresh without restart.
- **Database Option**: When `database_url` is configured, the service applies migrations in `services/policy-engine/migrations/`, seeds policies from the DSL file if the DB is empty, and serves policy list/create/simulate routes backed by Postgres (`policies`, `policy_rules` tables).
- **Access Control**: Admin endpoints require an `X-Admin-Token` header when `admin_token` is set; the CLI reads this from `OD_ADMIN_TOKEN`.
- **Future Enhancements**: Persistent policy CRUD UI/CLI with approvals, simulation endpoint, RBAC, audit events.
- **Interfaces**: REST (JSON) for ICAP adaptor + admin tools; eventually gRPC for low-latency decision path.

### 2.3 Cache Layer (Redis + Memory)
- In-memory cache ensures sub-millisecond lookups per adaptor instance.
- Redis stores JSON `PolicyDecision` keyed as `verdict:{entity_level}:{normalized_key}:policy{version}` with TTL.
- Future: keyspace notifications to invalidate adaptor caches on updates.

### 2.4 Classification & Reclassification Workers
- **LLM Worker**: Consumes Redis Streams, builds prompts, calls LLM, validates JSON, persists classification, updates caches, emits audit events. When `requires_content` is set, the worker waits until `page_contents` has a fresh excerpt and records interim state in `classification_requests` so operators can see what is blocked.
- **Reclass Worker**: Scheduled jobs for TTL expiry, taxonomy/model version upgrades, manual reclass triggers. Every refresh job now republishes both the classification and base-URL crawl job so repeated validations are still content-aware.

### 2.5 Management Plane
- **Admin API**: Aggregates policy, overrides, review queue, reporting endpoints with OIDC auth. New endpoints expose pending classifications (`GET /api/v1/classifications/pending`) and allow manual unblock/reclassify actions that immediately update caches.
- **React UI**: Dashboards, investigations, policy mgmt, review queue, health, cache inspection, and a “Pending Sites” view that surfaces everything stuck in `ContentPending` so analysts can approve or escalate.
- **CLI (`odctl`)**: Commands for env validation, policy/override import/export, cache inspection/invalidation, reclass triggers, smoke tests, migrations, taxonomy seeding, **plus** `odctl classification pending|unblock` for security teams who prefer terminal workflows.

- **Postgres**: Authoritative store for policies, classifications, overrides, review queue, audits, taxonomy, `page_contents` (Stage 9 Crawl4AI excerpts), and the new `classification_requests` table that tracks blocked keys awaiting content-aware verdicts.
- **Redis**: Distributed cache + queue coordination (Streams) + ephemeral job metadata.
- **Elasticsearch**: Structured event/audit storage; Kibana dashboards.

## 3. Request/Response Flows

### 3.1 Hot Path Decision Flow
1. Squid sends ICAP REQMOD to adaptor.
2. Adaptor parses ICAP, normalizes request, builds `PolicyDecisionRequest`.
3. Cache lookup:
   - Hit → return cached `PolicyDecision`.
   - Miss → call Policy Engine.
4. Policy Engine returns decision (allow/block/warn/etc.).
5. Adaptor caches verdict, returns ICAP response to Squid.
6. Squid enforces action (allow, block redirect page, warn, etc.).

### 3.2 Content-First Flow
1. Cache miss on an allow/monitor action causes the adaptor to return `PolicyAction::ContentPending`, serving a branded holding page and writing a `classification_requests` row.
2. The adaptor enqueues both a base-URL Crawl4AI job and a `requires_content` classification job.
3. Page fetcher crawls the site, stores `page_contents`, and updates status to `ok` (or `failed`).
4. LLM worker sees the `requires_content` flag, waits until `page_contents` is ready, then builds a prompt with the captured text, persists the classification, invalidates cache, and deletes the `classification_requests` row.
5. Subsequent requests hit Redis/Postgres and are immediately allowed or blocked with a content-backed verdict.

### 3.3 Override Flow (Future)
1. Admin defines override via API/UI/CLI (scope: user/IP/domain).
2. Policy engine/ adaptor checks override store before policy evaluation.
3. Overrides audit events emitted and TTL managed.

## 4. Data Model Snapshot
- `classifications` (normalized_key, taxonomy_version, verdict fields, TTL).
- `policies` / `policy_rules` (compiled DSL, priorities, outcomes).
- `overrides`, `review_queue`, `audit_events`, `reporting_aggregates` (per Spec §20).
- `page_contents` + `classification_requests` (Stage 9 content-aware pipeline storing Crawl4AI excerpts and pending keys).

## 5. Deployment Architecture
- **Local Dev**: `deploy/docker/docker-compose.yml` orchestrates Squid, adaptor, policy engine, Redis, Postgres, Elasticsearch, Kibana, Prometheus, workers, UI, and an odctl runner; `docker-compose.test.yml` / `docker-compose.smoke.yml` provide trimmed stacks for CI and quick validation.
- **Prod**:
  - Squid cluster fronted by load balancer; adaptor pods behind service mesh.
  - Redis cluster (sentinel or managed) for cache/queue; Postgres HA (Patroni or managed service).
  - Workers scaled via HPA based on queue depth.
  - Observability stack (Elastic/Kibana) sized for daily ingest.
  - Blue/green deployment for services; schema migrations run via CLI before rollout.

## 6. Security & Compliance Considerations
- mTLS between Squid and adaptor (future enhancement) and between services.
- OIDC/OAuth2 for admin API/UI/CLI auth with RBAC roles (admin, analyst, auditor, read-only).
- Audit trail stored in Postgres + Elasticsearch with hash chaining.
- Data masking/hashing for PII in logs/metrics; role-based field-level access in Kibana.

## 7. Future Work Mapping
- Stage addenda in `rfc/` define upcoming RFC scope (policy core, persistence, async classification, admin UI/CLI, reporting/observability, testing/ops).
- Implementation plan files in `implementation-plan/` map tasks, owners, dependencies, and evidence requirements per stage.

Use this architecture guide alongside the master spec and stage addenda to drive design reviews, onboarding, and audits.
