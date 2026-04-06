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

All routes require `X-Admin-Token` or a JWT with the listed roles. High-volume list endpoints use cursor pagination: `?limit=<int>&cursor=<opaque>` and return `{ data, meta }` where `meta` contains `limit`, `has_more`, and `next_cursor`. Existing policy/reporting list routes continue to use `page`/`page_size`.

### Overrides

| Method | Path | Description | Roles | Request Schema | Response |
| --- | --- | --- | --- | --- | --- |
| `GET` | `/api/v1/overrides` | List override records. | `policy-viewer`. | Cursor pagination query: `limit`, `cursor`. | Cursor-paged list of `OverrideRecord`. |
| `POST` | `/api/v1/overrides` | Create an override. | `policy-editor`. | `{ scope_type: "domain", scope_value, action: "allow"\|"block", reason?, created_by?, expires_at?, status? }`. | Newly created `OverrideRecord`. |
| `PUT`/`DELETE` | `/api/v1/overrides/:id` | Update or delete an override. | `policy-editor`. | Same payload as create (for PUT). | Updated `OverrideRecord` or `204 No Content`. |

Override precedence note: policy-engine evaluates active domain overrides before classification/policy rules. A domain override applies to both apex + subdomains, and when multiple overrides match, the most-specific scope wins.

### Authentication

| Method | Path | Description | Auth | Request Schema | Response |
| --- | --- | --- | --- | --- | --- |
| `POST` | `/api/v1/auth/login` | Local username/password login (local/hybrid auth mode). | Public route. | `{ username, password }` | `{ access_token, expires_in, user { id, username, email, roles, permissions, must_change_password } }` |
| `GET` | `/api/v1/auth/mode` | Resolve active authentication mode for UI behavior. | Public route. | — | `{ mode: "local" | "hybrid" | "oidc" }` |
| `POST` | `/api/v1/auth/change-password` | Change the authenticated local user's password. | Authenticated user principal. | `{ current_password, new_password }` | `204 No Content` |

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
| `GET` | `/api/v1/taxonomy` | Returns the canonical taxonomy (41 categories + subcategories) with the current activation profile. Structure is read-only. | `policy-viewer`. | — | `{ version, updated_at, updated_by, categories: [{ id, name, enabled, locked, subcategories: [...] }] }` |
| `PUT` | `/api/v1/taxonomy/activation` | Saves checkbox state for every category/subcategory. IDs must match the canonical file and `Unknown / Unclassified` cannot be disabled. | `policy-editor` (`ROLE_TAXONOMY_EDIT`). | `ActivationUpdateRequest`: `{ version, categories: [{ id, enabled, subcategories: [{ id, enabled }] }] }`. Version must match the canonical taxonomy version. | `{ version, updated_at, updated_by }`. Also increments `taxonomy_activation_changes_total`. |

> **Note:** Category/subcategory creation and deletion endpoints have been removed; taxonomy structure is governed solely by `config/canonical-taxonomy.json` and operator toggles only control allow/deny state.

### Reporting

| Method | Path | Description | Query Params |
| --- | --- | --- | --- |
| `GET` | `/api/v1/reporting/traffic` | Elastic-powered traffic summary with inferred-action/domain/category fallbacks when structured fields are sparse. | `range`, `top_n`, `bucket`. |
| `GET` | `/api/v1/reporting/status` | Reporting data-quality coverage counters for the selected time range. | `range`; response includes `total_docs`, `action_docs`, `category_docs`, `domain_docs`. |

### Cache & Diagnostics

| Method | Path | Description | Notes |
| --- | --- | --- | --- |
| `GET`/`DELETE` | `/api/v1/cache-entries/:cache_key` | Inspect or evict cache entries (ICAP adaptor cache). | Use normalized key (e.g., `domain:example.com`). |
| `GET` | `/api/v1/cli-logs` | Retrieve CLI audit log entries. | Cursor pagination; query supports `operator_id`, `limit`, `cursor`; response is `{ data, meta }`. |
| `GET` | `/api/v1/page-contents/:normalized_key` | Fetch latest Crawl4AI homepage excerpt for operator diagnostics. | Query: `version`, `max_excerpt`. Response includes `excerpt_format` (currently `markdown`) plus hash/ttl/language/fetch metadata and fetch targeting context (`source_url`, `resolved_url`, `attempt_summary`). |
| `GET` | `/api/v1/page-contents/:normalized_key/history` | List prior crawl versions. | Query: `limit` (default 5). |
| `GET` | `/api/v1/classifications/pending` | List sites blocked pending content-aware classification. | `policy-viewer`; cursor pagination with optional `status`, `limit`, `cursor`; returns pending records (`normalized_key`, `status`, `base_url`, timestamps). Rows can auto-exit pending after terminal fallback (`unknown-unclassified/insufficient-evidence`) when repeated fetch/output failures occur. |
| `POST` | `/api/v1/classifications/:normalized_key/pending` | Upsert a pending row for a key (used by ICAP for immediate queue visibility). | `policy-editor` (service token); body `{ status?, base_url? }`; returns `202 Accepted`. In domain-first mode subdomain keys are auto-promoted to canonical `domain:<registered_domain>`. |
| `POST` | `/api/v1/classifications/:normalized_key/manual-classify` | Manually classify a pending site with taxonomy category/subcategory only. | `policy-editor`; body `{ primary_category, subcategory, reason? }`; persists policy-computed action/risk/confidence and invalidates caches. In domain-first mode subdomain keys are auto-promoted to canonical `domain:<registered_domain>`. |
| `POST` | `/api/v1/classifications/:normalized_key/unblock` | Legacy/manual endpoint that accepts explicit action/risk/confidence payloads. | `policy-editor`; body `{ action, primary_category, subcategory, risk_level, confidence?, reason? }`; persists and invalidates caches. |
| `GET` | `/api/v1/classifications` | List classified and/or unclassified keys for management UI. | `policy-viewer`; cursor pagination with query `state=all|classified|unclassified`, `q`, `limit`, `cursor`; returns unified state/category/action rows including historical `recommended_action` plus current `effective_action` and `effective_decision_source` for classified rows. `flags` may include terminal fallback provenance (`local_parse_failed`, online verification result, insufficient-evidence terminal reason). |
| `GET` | `/api/v1/classifications/export` | Export domain classification bundle for backup/share. | `policy-viewer`; optional query `q`; returns bundle schema `od-classification-bundle.v1` with taxonomy metadata + entries. |
| `POST` | `/api/v1/classifications/import` | Import domain classification bundle with merge/replace behavior. | `policy-editor`; body `{ bundle, mode=merge|replace, recompute_policy_fields=true|false, dry_run }`; taxonomy-invalid rows are rejected and returned as JSONL (`invalid_rows_jsonl`) with suggested filename (`invalid_rows_filename`) and truncation flag (`invalid_rows_truncated`). |
| `POST` | `/api/v1/classifications/flush` | Flush domain classifications in bulk. | `policy-editor`; body `{ scope=all|prefix|keys, prefix?, keys?, dry_run }`; response includes `invalid_keys` for unparseable requested keys. Apply deletes matching classification + pending + page-content rows and invalidates cache. |
| `PATCH` | `/api/v1/classifications/:normalized_key` | Update classification taxonomy labels for a key. | `policy-editor`; body `{ primary_category, subcategory, reason? }`; recomputes action via policy engine. |
| `DELETE` | `/api/v1/classifications/:normalized_key` | Remove classification/pending/page-content state for a key and invalidate cache. | `policy-editor`; returns `204 No Content`. |
| `GET` | `/health/ready`, `/health/live` | Health probes. | — |
| `GET` | `/metrics` | Prometheus metrics (review SLA, cache invalidations). | Requires DB access to sync gauges. |

### Identity & Access Management

| Method | Path | Description | Roles |
| --- | --- | --- | --- |
| `GET`/`POST` | `/api/v1/iam/users` | List or create IAM users (`username` primary, optional `email`, optional OIDC/hybrid `subject`). | `iam:manage` (policy-admin). `GET` is cursor paginated (`limit`, `cursor`) and returns `{ data, meta }`. `POST` requires initial local `password` and supports `must_change_password`. |
| `GET`/`PUT`/`DELETE` | `/api/v1/iam/users/:id` | Fetch or update a user; `DELETE` supports hard delete with `?hard=true`, otherwise performs disable for compatibility. | `iam:manage` for mutations, `iam:view` for reads. Protected users and last-active-admin operations return `409`. |
| `POST` | `/api/v1/iam/users/:id/disable` | Disable a user explicitly. | `iam:manage`; blocked for protected users and last active policy-admin. |
| `POST` | `/api/v1/iam/users/:id/enable` | Re-enable a disabled user. | `iam:manage`. |
| `POST`/`DELETE` | `/api/v1/iam/users/:id/roles` | Assign or revoke role bindings for a user. | `iam:manage`. |
| `POST` | `/api/v1/iam/users/:id/set-password` | Set/reset a local user's password. | `iam:manage`. |
| `GET`/`POST` | `/api/v1/iam/users/:id/tokens` | List or create personal API keys for a user. | `iam:view` / `iam:manage`; plaintext token is returned only on create. |
| `DELETE` | `/api/v1/iam/users/:id/tokens/:token_id` | Revoke a user's personal API key. | `iam:manage`. |
| `GET`/`POST` | `/api/v1/iam/groups` | List/create groups (name + description). | `iam:view` / `iam:manage`. `GET` is cursor paginated (`limit`, `cursor`) and returns `{ data, meta }`. |
| `GET`/`PUT`/`DELETE` | `/api/v1/iam/groups/:id` | Inspect or update a group. | `iam:manage` for writes, `iam:view` for reads. |
| `POST`/`DELETE` | `/api/v1/iam/groups/:id/members` | Add/remove members from a group. | `iam:manage`. |
| `POST`/`DELETE` | `/api/v1/iam/groups/:id/roles` | Assign or revoke role bindings for a group. | `iam:manage`. |
| `GET` | `/api/v1/iam/roles` | List builtin roles and permissions. | `iam:view`. |
| `GET`/`POST` | `/api/v1/iam/service-accounts` | List or create service accounts (returns hashed token + rotate endpoint). | `iam:view` / `iam:manage`. `GET` is cursor paginated (`limit`, `cursor`) and returns `{ data, meta }`. |
| `POST` | `/api/v1/iam/service-accounts/:id/rotate` | Rotate a service-account token (optionally replacing roles). | `iam:manage`. |
| `DELETE` | `/api/v1/iam/service-accounts/:id` | Disable a service account. | `iam:manage`. |
| `GET` | `/api/v1/iam/whoami` | Introspect the caller’s effective roles and permissions. | Any authenticated caller. |
| `GET` | `/api/v1/iam/audit` | Paginated IAM audit log (mutations + metadata). | `iam:view` (policy-admin or auditor). Cursor pagination with `limit` and `cursor`; response is `{ data, meta }`. |

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
| `GET` | `/metrics` | Prometheus metrics covering `llm_jobs_*`, per-provider latency, failover counters, stale-pending diversion counters (`llm_stale_pending_*`), and online context/guardrail counters (`llm_context_mode_total`, `llm_metadata_only_guardrail_total`, `llm_metadata_only_requeue_total`, `llm_metadata_only_reason_total`, `llm_fetch_failure_fallback_total`, `llm_primary_output_invalid_total`, `llm_online_verification_total`, `llm_terminal_insufficient_evidence_total`). |

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
