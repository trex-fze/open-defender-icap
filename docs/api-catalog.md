# API Catalog

This reference lists every HTTP endpoint exposed by the services in this repository, along with authentication requirements and the shapes of the expected payloads. Use it as a quick lookup when building tooling, writing tests, or documenting new routes.

**Conventions**

- `X-Admin-Token` refers to machine/service-account tokens.
- Local interactive login uses `POST /api/v1/auth/login` and returns bearer tokens for `Authorization: Bearer ...`.
- When OIDC mode is enabled, bearer tokens must contain the roles noted in each table. The policy engine trusts the Admin API IAM resolver, so credentials accepted by `/api/v1/iam/whoami` flow downstream.
- Timestamps are ISO 8601 strings (`2026-03-24T10:15:30Z`).
- All JSON bodies use UTF‑8 and should include `Content-Type: application/json`.
- Metrics endpoints return Prometheus text exposition format and do not require auth.

---

## Policy Engine (`policy-engine`)

| Method | Path | Description | Auth / Headers | Request Schema | Response |
| --- | --- | --- | --- | --- | --- |
| `POST` | `/api/v1/decision` | Evaluate a normalized key + context and return a policy decision. | None (hot path). | `DecisionRequest`: `{ normalized_key, entity_level, source_ip, user_id?, group_ids?, category_hint?, risk_hint?, confidence_hint? }`. | `PolicyDecision` JSON with `action`, `category`, `risk`, `reason`. |
| `GET` | `/api/v1/policies` | List the current in-memory policy (rules + version). | `X-Admin-Token` with `policy-viewer` role (or OIDC JWT). | — | `PolicyListResponse` containing `policy_id`, `version`, array of rule summaries. |
| `POST` | `/api/v1/policies/reload` | Reload policy from file/DB (DB mode). | `policy-editor` role. | — | `PolicyListResponse`. |
| `POST` | `/api/v1/policies` | Create a new policy document (DB mode). | `policy-editor` role. | `PolicyCreateRequest`: `{ name, version, created_by?, rules: [PolicyRule] }`. Rules follow the DSL defined in `crates/policy-dsl`. | `PolicyListResponse` with the newly active policy. |
| `PUT` | `/api/v1/policies/:id` | Update metadata or rules for `:id` (`current` allowed). | `policy-editor` role. | `PolicyUpdateRequest`: `{ version?, status?, notes?, rules? }`. | `PolicyListResponse`. |
| `POST` | `/api/v1/policies/simulate` | Simulate a policy decision without persisting it. | `policy-viewer` role. | Same as `DecisionRequest`. | `SimulationResponse` (`decision`, `matched_rule_id`, `policy_version`). |
| `GET` | `/health/ready` | Liveness/readiness probe. | None. | — | `{"status":"OK"}`. |

---

## Admin API (`admin-api`)

All routes require `X-Admin-Token` or a JWT with the listed roles. Pagination parameters follow the pattern `?page=<int>&page_size=<int>` unless otherwise stated.

### Overrides

| Method | Path | Description | Roles | Request Schema | Response |
| --- | --- | --- | --- | --- | --- |
| `GET` | `/api/v1/overrides` | List override records (filters via query: `scope_type`, `status`, `search`). | `policy-viewer`. | — | Paged list of `OverrideRecord`. |
| `POST` | `/api/v1/overrides` | Create an override. | `policy-editor`. | `{ scope_type: "domain"\|"user"\|"ip", scope_value, action: allow/block/warn/monitor/review/require-approval, reason?, created_by?, expires_at?, status? }`. | Newly created `OverrideRecord`. |
| `PUT`/`DELETE` | `/api/v1/overrides/:id` | Update or delete an override. | `policy-editor`. | Same payload as create (for PUT). | Updated `OverrideRecord` or `204 No Content`. |

### Authentication

| Method | Path | Description | Auth | Request Schema | Response |
| --- | --- | --- | --- | --- | --- |
| `POST` | `/api/v1/auth/login` | Local username/password login (local/hybrid auth mode). | Public route. | `{ username, password }` | `{ access_token, expires_in, user { id, username, email, roles, permissions, must_change_password } }` |

### Review Queue

| Method | Path | Description | Roles | Request Schema | Response |
| --- | --- | --- | --- | --- | --- |
| `GET` | `/api/v1/review-queue` | List reviews (`status`, `assign`, `search` query params). | `review-approver` or `policy-viewer`. | — | Array of `ReviewRecord`. |
| `POST` | `/api/v1/review-queue/:id/resolve` | Resolve a review entry. | `review-approver`. | `{ status: "approved"\|"rejected"\|..., decided_by?, decision_notes?, decision_action? }`. | Updated `ReviewRecord`. |

### Embedded Policy Admin (mirror of policy-engine)

| Method | Path | Description | Roles |
| --- | --- | --- | --- |
| `GET`/`POST` | `/api/v1/policies` | List or create policies via Admin API. | `policy-viewer` / `policy-editor`. |
| `GET`/`PUT` | `/api/v1/policies/:id` | Fetch or update a policy by ID. | `policy-viewer` / `policy-editor`. |
| `POST` | `/api/v1/policies/:id/publish` | Mark a policy version as active (publishes notes). | `policy-editor`. |
| `POST` | `/api/v1/policies/validate` | Validate a DSL payload without persisting. | `policy-editor`. |

### Taxonomy

| Method | Path | Description | Roles | Request Schema | Response |
| --- | --- | --- | --- | --- | --- |
| `GET` | `/api/v1/taxonomy` | Returns the canonical taxonomy (40 categories + subcategories) with the current activation profile. Structure is read-only. | `policy-viewer`. | — | `{ version, updated_at, updated_by, categories: [{ id, name, enabled, locked, subcategories: [...] }] }` |
| `PUT` | `/api/v1/taxonomy/activation` | Saves checkbox state for every category/subcategory. IDs must match the canonical file and `Unknown / Unclassified` cannot be disabled. | `policy-editor` (`ROLE_TAXONOMY_EDIT`). | `ActivationUpdateRequest`: `{ version, categories: [{ id, enabled, subcategories: [{ id, enabled }] }] }`. Version must match the canonical taxonomy version. | `{ version, updated_at, updated_by }`. Also increments `taxonomy_activation_changes_total`. |

> **Note:** Category/subcategory creation and deletion endpoints have been removed; taxonomy structure is governed solely by `config/canonical-taxonomy.json` and operator toggles only control allow/deny state.

### Reporting

| Method | Path | Description | Query Params |
| --- | --- | --- | --- |
| `GET` | `/api/v1/reporting/aggregates` | Returns stored aggregates for dashboards. | `dimension`, `period`, `limit`. |
| `GET` | `/api/v1/reporting/traffic` | Elastic-powered traffic summary. | `start`, `end`, `dimension`, `limit`, `filters`. |

### Cache & Diagnostics

| Method | Path | Description | Notes |
| --- | --- | --- | --- |
| `GET`/`DELETE` | `/api/v1/cache-entries/:cache_key` | Inspect or evict cache entries (ICAP adaptor cache). | Use normalized key (e.g., `domain:example.com`). |
| `GET` | `/api/v1/cli-logs` | Retrieve CLI audit log entries. | Query: `operator_id`, `limit` (default 50). |
| `GET` | `/api/v1/page-contents/:normalized_key` | Fetch latest Crawl4AI homepage HTML context (`[HEAD]`, `[TITLE]`, `[BODY]`). | Query: `version`, `max_excerpt`. Response includes metadata (hash, ttl, language, fetch status). |
| `GET` | `/api/v1/page-contents/:normalized_key/history` | List prior crawl versions. | Query: `limit` (default 5). |
| `GET` | `/api/v1/classifications/pending` | List sites blocked pending content-aware classification. | `policy-viewer` role; query: `status`, `limit`. | Array of pending records (key, base_url, updated timestamps). |
| `POST` | `/api/v1/classifications/:normalized_key/unblock` | Manually approve or reclassify a blocked site. | `policy-editor` role; body `{ action, primary_category, subcategory, risk_level, confidence?, reason? }`. | Returns the persisted classification row; also invalidates caches. |
| `GET` | `/health/ready`, `/health/live` | Health probes. | — |
| `GET` | `/metrics` | Prometheus metrics (review SLA, cache invalidations). | Requires DB access to sync gauges. |

### Identity & Access Management

| Method | Path | Description | Roles |
| --- | --- | --- | --- |
| `GET`/`POST` | `/api/v1/iam/users` | List or create IAM users (email + optional OIDC subject). | `iam:manage` (policy-admin). |
| `GET`/`PUT`/`DELETE` | `/api/v1/iam/users/:id` | Fetch, update, or disable a user. | `iam:manage` for mutations, `iam:view` for reads. |
| `POST`/`DELETE` | `/api/v1/iam/users/:id/roles` | Assign or revoke role bindings for a user. | `iam:manage`. |
| `GET`/`POST` | `/api/v1/iam/groups` | List/create groups (name + description). | `iam:view` / `iam:manage`. |
| `GET`/`PUT`/`DELETE` | `/api/v1/iam/groups/:id` | Inspect or update a group. | `iam:manage` for writes, `iam:view` for reads. |
| `POST`/`DELETE` | `/api/v1/iam/groups/:id/members` | Add/remove members from a group. | `iam:manage`. |
| `POST`/`DELETE` | `/api/v1/iam/groups/:id/roles` | Assign or revoke role bindings for a group. | `iam:manage`. |
| `GET` | `/api/v1/iam/roles` | List builtin roles and permissions. | `iam:view`. |
| `GET`/`POST` | `/api/v1/iam/service-accounts` | List or create service accounts (returns hashed token + rotate endpoint). | `iam:view` / `iam:manage`. |
| `POST` | `/api/v1/iam/service-accounts/:id/rotate` | Rotate a service-account token (optionally replacing roles). | `iam:manage`. |
| `DELETE` | `/api/v1/iam/service-accounts/:id` | Disable a service account. | `iam:manage`. |
| `GET` | `/api/v1/iam/whoami` | Introspect the caller’s effective roles and permissions. | Any authenticated caller. |
| `GET` | `/api/v1/iam/audit` | Paginated IAM audit log (mutations + metadata). | `iam:view` (policy-admin or auditor). |

---

## Event Ingester (`event-ingester`)

| Method | Path | Description | Auth / Headers | Request Schema | Response |
| --- | --- | --- | --- | --- | --- |
| `POST` | `/ingest/filebeat` | Primary ingest endpoint for Filebeat/Logstash. | Optional `X-Filebeat-Secret` (`OD_FILEBEAT_SECRET`). | `FilebeatEnvelope`: `{ events: [ { message, url.full, trace_id?, od.* } ] }`. Accepts raw Filebeat bulk payload. | `202 Accepted` on success, `401` if secret mismatch. |
| `GET` | `/health/ready` | Health probe. | None. | — | `HealthResponse`. |
| `GET` | `/metrics` | Prometheus counters for ingest batches, page fetch scheduling. | None. | — | Text metrics. |

---

## Admin Tooling & Workers

### LLM Worker (`llm-worker`)

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/providers` | Lists configured LLM providers (name, type, endpoint, role). Useful for operator dashboards and tests. |
| `GET` | `/metrics` | Prometheus metrics covering `llm_jobs_*`, per-provider latency, failover counters. |

### Reclass Worker (`reclass-worker`)

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/metrics` | Prometheus metrics for reclassification backlog, dispatch counts, and page-fetch enqueue totals. |

### Page Fetcher (`workers/page-fetcher`)

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/metrics` | Prometheus metrics (jobs started/completed/failed, crawl latency, Redis consumer stats). |

### ICAP Adaptor (`services/icap-adaptor`)

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/metrics` | Prometheus counters/gauges for ICAP throughput, cache operations, page fetch job publication. |

---

## Crawl4AI Service (`services/crawl4ai-service`)

| Method | Path | Description | Request Schema | Response |
| --- | --- | --- | --- | --- |
| `POST` | `/crawl` | Fetches a page via headless Chromium (used by page-fetcher). | `{ url: string (HTTP/HTTPS), normalized_key: string, max_html_bytes?: int, max_text_chars?: int }`. | `{ normalized_key, url, status, cleaned_text, raw_html, content_type, language?, title?, status_code?, metadata }`. |
| `GET` | `/healthz` | Health check for the crawler container. | — | `{ "status": "ok" }`. |

---

## Metrics-Only Endpoints

Prometheus scrapes these paths; they do not accept application payloads but are useful to know when building dashboards.

| Service | Path | Notes |
| --- | --- | --- |
| Admin API | `/metrics` | Review queue SLA, cache invalidation stats, `taxonomy_activation_changes_total`. |
| Policy Engine | (exposed via `metrics_host` in config) | Counters for decisions, reload times (feature planned). |
| ICAP Adaptor | `/metrics` | Request rates, cache hits, Redis publication metrics. |
| Event Ingester | `/metrics` | Ingest batch counts, crawl job attempts. |
| Page Fetcher | `/metrics` | Crawl throughput, latency buckets, storage errors. |
| LLM Worker | `/metrics` | Provider invocations/timeouts, job lifecycle counts. |
| Reclass Worker | `/metrics` | Backlog gauge, dispatch counters. |

---

## How to Extend

1. When you add a new route, update the appropriate table with method, path, and payload summary.
2. Link to structs (or docs) when a payload is complex; keep descriptions concise.
3. Run `markdownlint` (if installed) or use the GitHub preview to verify table formatting.
