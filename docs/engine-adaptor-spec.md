# 1. Executive Summary
- Deliver a Rust-based ICAP decision platform around Squid that classifies, caches, and enforces policy on web requests while React dashboards, Redis caching, Elasticsearch/Kibana analytics, and a Rust CLI equip all stakeholders.
- Hot-path decisions remain sub-10 ms by leveraging deterministic normalization, override/policy checks, and multi-tier caches; LLM classification runs asynchronously with robust caching and auditing to avoid repeated inference.
- Docker + docker-compose drive development/test, ensuring reproducible environments with Squid, Redis, Postgres, Elasticsearch, Kibana, Rust services, workers, and React UI exposed on ports ≥19000.
- Comprehensive taxonomy, policy engine, analytics, audit, and CLI tooling make the solution deployment-ready, testable, and operations-friendly with clear evidence artifacts.

# 2. Assumptions
- Postgres 15 is the transactional data store; infrastructure supplies secure networking, HA options, and secret management.
- Enterprise IdP (OIDC) available for admin UI/API/CLI auth; service accounts use mTLS/API tokens.
- LLM provider reachable via outbound HTTPS; prompts/responses sanitized and logged without sensitive payloads.
- Squid already deployed/maintained; SSL bump permitted where policy allows.
- All user-facing services bind to ports 19000–19019 (Admin API 19000, UI 19001, CLI gRPC 19002, metrics 19005, etc.).
- Infrastructure supports Docker, docker-compose, Redis Cluster, Elasticsearch 8.x, Kibana 8.x.

# 3. Applicable RFCs / Standards Mapping
| RFC/Standard | Title | Relevance | Components | Implementation Notes | Compliance Decisions |
| --- | --- | --- | --- | --- | --- |
| RFC 3507 | Internet Content Adaptation Protocol (ICAP) | Governs Squid⇔Adaptor interaction | Squid, ICAP adaptor | Support REQMOD/RESPMOD, preview, OPTIONS, status codes | No deviation; fail-open configurable per RFC guidance |
| RFC 2616 / 7230-7235 | HTTP/1.1 Semantics | Header preservation, method semantics | ICAP adaptor, Policy API | Do not modify payload except headers required for policy; respect hop-by-hop rules | Normalize headers before logging |
| RFC 3986 | URI Syntax | Canonicalization for cache keys | Normalizer, cache, classifier | Lower-case host, percent-decode, remove fragments | Punycode via RFC 5890 |
| RFC 5890/5891 | IDNA | Internationalized domains | Normalizer, taxonomy | Convert to ASCII using UTS #46 | Reject mixed script anomalies |
| RFC 2818 / 8446 | TLS over HTTPS/TLS 1.3 | SSL bump behaviors, mTLS | Squid, API endpoints | Maintain cert trust, log SNI | Enforce TLS 1.2+ |
| RFC 3339 | Date/Time | Logging/audit timestamps | All services | Store UTC ISO8601 | Mandatory |
| RFC 1918 / 4291 | IPv4/IPv6 addressing | Analytics by IP | Reporting, UI | Support IPv6 canonical format | Dual-stack indexing |
| RFC 3261 (reference) | SIP header style | Only for header canonicalization; no direct dependency | Normalizer | Borrow canonicalization rules | n/a |
| JSON:API / OpenAPI 3.1 | API contracts | Admin/report APIs | Admin API, CLI | Provide schemas, validation | Documented via OpenAPI |
| OWASP ASVS 4.0 | App security | Security tests, auth | All services/UI/CLI | Enforce RBAC, rate limits | Security review step |
| NIST 800-53 Moderate | Security controls | Logging/audit, access | SOC workflows | Provide audit trails, separation of duties | Evidence captured |

# 4. High-Level Architecture
1. Squid proxies traffic, enforces base ACLs, and delegates policy decisions via ICAP REQMOD/RESPMOD to the Rust adaptor.
2. ICAP adaptor performs normalization, cache/override/policy lookups, and issues immediate verdicts; unknown cases enqueue async classification jobs while responding with safe temporary actions.
3. Redis provides shared caches and queues (Streams) ensuring already-classified sites are reused and coordination occurs across adaptor instances.
4. Postgres persists canonical data (classifications, policies, overrides, audits, reports).
5. LLM worker and reclassification worker consume Redis Streams, call LLM providers, validate JSON, and write verdicts while updating caches and emitting audit events.
6. Admin REST API (Axum) aggregates policy, overrides, review, reporting, health endpoints on port 19000; React frontend consumes these APIs on port 19001.
7. Elasticsearch stores decision logs/events; Kibana provides SOC dashboards emphasizing IP analytics, health, and operations.
8. Rust CLI (`odctl`) manages configuration, policies, cache, tests, and health via REST/gRPC on port 19002.

# 5. Textual Architecture Diagram
```
Clients -> Squid Proxy (ACLs, SSL bump)
        -> ICAP REQMOD/RESPMOD -> Rust ICAP Adaptor svc-icap
            -> In-Process LRU Cache
            -> Redis Cluster (Shared Cache + Streams)
            -> Postgres (Policies, Classifications, Overrides, Audit)
            -> Policy Engine svc-policy
            -> Event Publisher -> Elasticsearch -> Kibana Dashboards
            -> Classification Queue -> LLM Worker svc-llm-worker -> LLM Provider
            -> Reclassification Worker svc-reclass
Admin API svc-admin (19000) -> React UI (19001)
CLI odctl -> Admin API/Health (19002)
Observability: Prometheus exporters + logs -> Elasticsearch, metrics -> Kibana Observability
```

# 6. Detailed Component Design
## 6.1 Squid Proxy
- **Purpose**: Network gateway; handles SSL bump, caching, authentication, bandwidth controls.
- **Responsibilities**: Apply static ACLs, capture metadata for ICAP headers, failover behavior.
- **Interfaces**: ICAP services at `icap://icap-adaptor:1344/reqmod` and `/respmod`; logging to access.log.
- **Dependencies**: TLS certificates, user directory for auth, Filebeat for logs.
- **Scaling**: Horizontal scaling via load balancers; each instance points to ICAP pool.
- **Failure Modes**: ICAP timeout → fallback action (config); SSL bump failure; log shipping delay.
- **Logs/Metrics**: access.log, cache.log, ICAP status codes; metrics via Squid SNMP or Prometheus exporter.
- **Tests**: ICAP OPTIONS, fail-open/fail-close toggles, ACL precedence.
- **Evidence**: squid.conf snippet showing ICAP service, test logs verifying handshake.

## 6.2 ICAP Adaptor Service (`svc-icap`)
- **Purpose**: Provide synchronous verdicts to Squid with minimal latency.
- **Responsibilities**: Parse ICAP messages, normalize requests, query caches and policies, send decisions, enqueue classification jobs, emit events.
- **Interfaces**: ICAP TCP port 1344, Redis cache client, Policy Engine gRPC, Postgres (read/write), Redis Streams for jobs, Prometheus metrics endpoint (19005), logging to stdout.
- **Dependencies**: Redis, Postgres, Policy Engine, Config service.
- **Scaling**: Stateless pods; scale horizontally; uses Redis locks for per-key classification placeholder.
- **Failure Modes**: Redis unavailable (fallback to DB), DB unavailable (fail-open/close), policy engine unreachable (cached policies), queue publish failure (retry/backoff).
- **Logs/Metrics**: `squid_to_icap_latency`, `cache_hit_ratio`, `cache_miss_ratio`, `first_seen_domain_rate`, verdict stats.
- **Tests**: Unit (normalization, cache), integration (ICAP handshake), perf (latency), security (input validation).
- **Evidence**: service image, ICAP regression report, metrics screenshot.

## 6.3 Policy Engine (`svc-policy`)
- **Purpose**: Evaluate tenant policies with precedence (user/group/IP/category/time/risk/exceptions).
- **Responsibilities**: Compile DSL, serve policy decisions via gRPC/REST, maintain policy versioning.
- **Interfaces**: gRPC to ICAP adaptor, REST to admin UI/CLI, Postgres for policy storage, Redis for cache of compiled policies.
- **Scaling**: In-memory compiled trees per tenant; watchers for policy changes.
- **Failure Modes**: DSL compile failure (reject changes), stale cache (version mismatch), DB offline (read from warm caches, limit TTL).
- **Logs/Metrics**: `policy_decision_latency`, rule hit counts, `override_rate`.
- **Tests**: Unit (precedence), integration (policy update propagation), security (auth).
- **Evidence**: policy DSL spec, compiled policy tests.

## 6.4 Classification Service (`svc-classify`)
- **Purpose**: Manage classification records, domain allow/deny overrides, and manual actions.
- **Responsibilities**: CRUD for classifications, manual overrides, review assignment, classification lookup APIs.
- **Interfaces**: REST endpoints, Postgres, Redis invalidation pub/sub, Elasticsearch for audit.
- **Scaling**: Stateless; caches read results; uses optimistic locking on classification versions.
- **Failure Modes**: conflicting overrides, DB contention, invalid user input.
- **Logs/Metrics**: override creation, taxonomy activation changes, `unknown_classification_rate`.
- **Tests**: Unit (CRUD validation), integration (policy engine consumption), security (RBAC).
- **Evidence**: API responses, audit entries.

## 6.5 LLM Worker (`svc-llm-worker`)
- **Purpose**: Perform asynchronous classification jobs via LLM plus deterministic heuristics.
- **Responsibilities**: Consume Redis Streams, fetch metadata, build prompts, call LLM provider, validate JSON, persist classification, update cache, emit audits.
- **Interfaces**: Redis Streams, LLM HTTPS API, Postgres, Redis cache, Elasticsearch.
- **Failure Modes**: LLM timeout, invalid JSON, rate limiting, network failure. Retries with exponential backoff; fallback to heuristic classification and mark `unknown`.
- **Logs/Metrics**: `llm_invocation_count`, `llm_timeout_rate`, `llm_invalid_response_rate`, classification latency.
- **Tests**: Unit (prompt builder, validation), integration (LLM stub), performance (batch throughput), security (prompt injection tests).
- **Evidence**: worker logs, classification records, monitoring dashboards.

## 6.6 Reclassification Worker (`svc-reclass`)
- **Purpose**: Refresh stale/low-confidence entries, handle taxonomy/model updates, manual triggers.
- **Responsibilities**: Monitor reclassification queue, generate jobs, coordinate TTL resets, ensure version compliance.
- **Interfaces**: Postgres, Redis, classification APIs, queue.
- **Failure Modes**: backlog overflow, duplicate jobs, version mismatch.
- **Logs/Metrics**: `reclassification_backlog`, `llm_invocation_count` (subset), job durations.
- **Tests**: Unit (job scheduling), integration (cache invalidation), perf (bulk reclass), security (auth on triggers).
- **Evidence**: job reports, backlog dashboards.

## 6.7 Review/Override Service (within `svc-classify`)
- **Purpose**: Manage manual reviews, user override submissions, approvals, expirations.
- **Responsibilities**: Enforce workflow states, notify policy engine, log audits.
- **Interfaces**: REST, WebSocket for UI notifications, Postgres, email/SIEM hooks.

## 6.8 Event Ingestion Pipeline
- **Purpose**: Deliver structured events from adaptor/policies/workers to Elasticsearch.
- **Responsibilities**: Buffer events, perform schema validation, bulk insert, handle retries.
- **Interfaces**: Kafka optional; minimal design uses Redis Stream -> event ingester service -> ES bulk API.
- **Metrics**: `elasticsearch_index_failures`, ingestion latency.

## 6.9 Reporting/Analytics (`svc-report`)
- **Purpose**: Serve dashboards/reports with IP-focused analytics.
- **Responsibilities**: Query Elasticsearch aggregations, convert to API responses, support exports.
- **Interfaces**: Elasticsearch/SQL, Admin API.

## 6.10 Admin API Gateway (`svc-admin`)
- **Purpose**: External API aggregator for UI/CLI; enforces RBAC.
- **Responsibilities**: Auth via OIDC, request fan-out to policy/classify/report services, caching for metadata.

## 6.11 React Admin UI (`web-admin`)
- **Purpose**: Provide dashboards, investigations, policy mgmt, overrides, audits, health, cache inspection.
- **Responsibilities**: SPA with role-based navigation; integrate charts/tables.

## 6.12 CLI (`odctl`)
- **Purpose**: Terminal tool for admins/devops.
- **Responsibilities**: Provide commands for env/config validation, policies/overrides import/export, cache, classification inspection, reclassification, health, smoke tests, migrations, and report/audit queries. (Taxonomy structure now lives in `config/canonical-taxonomy.json`; the CLI no longer seeds taxonomy tables.)

## 6.13 Observability subsystem
- **Purpose**: Provide metrics/logs/traces/dashboards/alerts using Elasticsearch, Kibana, Prometheus.

(Detailed responsibilities, interfaces, dependencies, scaling, failure modes, logs, metrics, tests, evidence provided per component in Appendix tables.)

# 7. Content Filtering Taxonomy
For each category: definition, subcategories, sample site types, enterprise action tendency, default risk, edge cases.

1. **Adult / Sexual Content**: Explicit sexual imagery/services. Subcategories: Pornography, Nudity, Escort Services, Sexual Education. Samples: adult tube sites, cam services. Action: Block. Risk: High. Edge: health/education contexts flagged via tags.
2. **Gambling**: Wagering and betting. Subs: Casinos, Sports Betting, Lottery, Fantasy Sports. Sample: bet365. Action: Block. Risk: High. Edge: state lottery info (monitor).
3. **Violence / Weapons**: Depictions or sales of weapons. Subs: Firearms Retail, Militant Content, Violent Media, DIY Weapons. Action: Warn/Block. Risk: Medium-High. Edge: news coverage.
4. **Drugs / Controlled Substances**: Illegal drugs or paraphernalia. Subs: Recreational Marijuana, Prescription Abuse, Paraphernalia Shops, Harm Reduction. Action: Block (except harm reduction monitor). Risk: High.
5. **Hate / Extremism**: Content promoting hatred. Subs: Extremist Propaganda, Hate Forums, Ideological Recruitment. Action: Block. Risk: Critical. Edge: academic archives flagged as `monitor`.
6. **Illegal / Criminal Activity**: Instructions or facilitation. Subs: Hacking Guides, Fraud Tutorials, Black Markets. Action: Block. Risk: Critical.
7. **Malware / Phishing / Fraud**: Malicious campaigns. Subs: Phishing, Malware Delivery, Scam Sites. Action: Block, safe browsing integration. Risk: Critical.
8. **Social Media**: Platforms enabling social networking. Subs: General Social, Professional Networks, Microblogging, UGC Communities. Action: Allow/Monitor. Risk: Medium.
9. **Streaming / Entertainment**: Video/audio streaming. Subs: Video OTT, Music Streaming, Podcast Platforms. Action: Monitor/Allow; Risk Medium.
10. **Games**: Gaming portals, MMOs. Subs: Browser Games, Game Downloads, Lootbox Stores. Action: Monitor/Allow. Risk: Medium.
11. **Shopping / E-Commerce**: Retail. Subs: Marketplaces, Retail Brands, Auctions. Action: Allow. Risk: Low.
12. **News / Media**: News outlets. Subs: National News, Local News, Blogs. Action: Allow. Risk: Low.
13. **Education**: Schools, learning resources. Subs: Universities, MOOCs, K-12 resources. Action: Allow. Risk: Low.
14. **Government / Public Services**: Government portals. Subs: Federal, State, Municipal. Action: Allow. Risk: Low.
15. **Finance / Banking**: Financial institutions. Subs: Retail Banking, Investment, Payment Processors. Action: Allow. Risk: Medium (fraud watch).
16. **Health / Medical**: Healthcare info/providers. Subs: Hospitals, Health News, Medical Forums. Action: Allow/Monitor. Risk: Medium.
17. **Religion / Belief**: Religious orgs. Subs: Churches, Faith Forums. Action: Allow. Risk: Low.
18. **Travel / Transportation**: Airlines, booking. Subs: Airlines, Hotels, Maps. Action: Allow. Risk: Low.
19. **Technology / IT**: Tech news, vendors. Subs: Software Vendors, Dev Docs, IT Blogs. Action: Allow. Risk: Low.
20. **Web Infrastructure / Hosting / CDN**: Hosting providers, CDNs. Subs: VPS providers, CDN, DNS. Action: Monitor; Risk: Medium due to shared content.
21. **Business / Corporate**: Enterprise websites. Subs: B2B Services, Corporate Sites. Action: Allow.
22. **Email / Messaging / Collaboration**: Webmail, chat. Subs: Webmail, Team Chat, Video Conferencing. Action: Monitor; Risk Medium.
23. **File Sharing / Storage**: Cloud storage, P2P. Subs: Enterprise Storage, Personal Cloud, P2P Gateways. Action: Monitor/Block depending. Risk: Medium-High.
24. **AI / LLM Tools**: AI portals. Subs: Chatbots, Model Hosting, Prompt Libraries. Action: Monitor/Review. Risk: Medium.
25. **Search Engines / Portals**: Search and directories. Subs: Global Search, Specialty Search. Action: Allow.
26. **Kids / Family**: Child-friendly portals. Subs: Learning Games, Parenting. Action: Allow; Risk Low.
27. **Dating / Relationships**: Dating services. Subs: General Dating, Adult Dating, Matchmaking. Action: Warn/Block (adult). Risk Medium.
28. **Job Search / Career**: Job boards. Subs: Job Aggregators, Professional Services. Action: Allow.
29. **Forums / Communities**: Discussion boards. Subs: Tech Forums, Off-topic, Anonymous Boards. Action: Monitor due to risk of mixed content.
30. **Reference / Knowledge**: Encyclopedias, wikis. Subs: Encyclopedic, Q&A, Documentation. Action: Allow.
31. **Proxy / VPN / Anonymizers**: Circumvention tools. Subs: VPN Services, Web Proxies, Tor Gateways. Action: Block/Require Approval. Risk: High.
32. **Remote Access / Tunneling**: Remote desktop, SSH gateways. Subs: RDP SaaS, SSH jump hosts. Action: Monitor/Require Approval. Risk: High.
33. **Cryptocurrency / Blockchain**: Exchanges, wallets. Subs: Exchanges, Mining Pools, NFT Markets. Action: Monitor/Review. Risk: Medium-High.
34. **Real Estate**: Property listings. Subs: Residential, Commercial. Action: Allow.
35. **Food / Dining**: Restaurants, delivery. Subs: Delivery Apps, Recipe Sites. Action: Allow.
36. **Sports**: Sports news, leagues. Subs: Scores, Fantasy (overlaps with gambling). Action: Allow/Monitor.
37. **Arts / Culture**: Museums, galleries. Subs: Art Museums, Cultural Events. Action: Allow.
38. **Personal Blogs / Pages**: Individual sites. Subs: Lifestyle Blogs, Portfolio Sites. Action: Monitor.
39. **URL Shorteners / Redirectors**: Redirect services. Subs: Public Shorteners, Enterprise Shorteners. Action: Monitor; Risk Medium (obfuscation).
40. **Unknown / Unclassified**: Default for insufficient data. Action: Monitor/Warn; Risk Medium.

# 8. Data Model / Schema
| Entity | Purpose | Key Fields | Indexes | Retention | Owner | Tests |
| --- | --- | --- | --- | --- | --- | --- |
| requests | Record each decision event | id (uuid), timestamp (timestamptz), source_ip, user_id, device_id, normalized_key, verdict, action_reason, trace_id | (timestamp), (source_ip,timestamp), (user_id,timestamp) | 90d hot, 1y cold | Policy engine | CRUD, partition rotation |
| normalized_destinations | Canonical domain/subdomain/url keys | normalized_key, entity_level, domain, subdomain, url_hash | PK normalized_key | permanent | Classification svc | normalization tests |
| domains | Domain metadata | domain, registrant, first_seen, last_seen | (domain) unique | 2y | Classification | domain lookup |
| urls | Full URL metadata | url_hash, domain, path, query_fingerprint | (url_hash) | 180d | Classification | canonicalization |
| classifications | Current verdicts | id, normalized_key, taxonomy_version, model_version, primary_category, subcategory, risk_level, confidence, recommended_action, sfw, flags, ttl, status | (normalized_key), (taxonomy_version) | until replaced + 2y | Classification | versioning |
| classification_versions | Historical verdicts | classification_id, version, changed_by, reason | (classification_id) | 2y | Audit | version history |
| taxonomy_activation_profiles / taxonomy_activation_entries | Canonical activation state | profile metadata (`id`, `version`, `updated_by`, `updated_at`), checkbox states (`category_id`, `subcategory_id`, `enabled`) | (profile_id, category_id, subcategory_id) | permanent | Product | activation save tests |
| policies | Tenant policy objects | policy_id, tenant_id, version, status, compiled_hash, created_by | (tenant_id,version) | history kept | Policy engine | DSL parse |
| policy_rules | Atomic rules | rule_id, policy_id, priority, conditions JSONB, outcome | (policy_id, priority) | same as policies | Policy engine | precedence tests |
| cache_entries | Cache materialization metadata | key, value_json, expires_at, source | (key) | 7d | Cache team | TTL tests |
| overrides | Manual overrides | override_id, scope (domain/user/ip), action, reason, created_by, expires_at | (scope_type, scope_value) | until expiry + 1y | Classification | workflow tests |
| reclassification_jobs | Background tasks | job_id, reason, scope, status, created_at, started_at, completed_at | (status) | 1y | Reclass worker | job lifecycle |
| users/groups/devices | Directory mirror | id, name, attributes, last_synced | (id) | 1y after deletion | IAM | sync tests |
| ip_intelligence | Metadata per IP | ip, location, owner, device_refs | (ip) | 90d | Reporting | enrichment tests |
| audit_events | Immutable audits | event_id, timestamp, actor, action, target, payload_hash, data JSONB | (timestamp), (actor), (action) | 2y | Security | hash chain |
| reporting_aggregates | Precomputed stats | id, period_start, dimension (ip/user/device/category), metrics JSONB | (dimension,period_start) | 1y | Reporting | refresh tests |
| cli_operation_logs | CLI usage | id, operator_id, command, args_hash, result, timestamp | (operator_id,timestamp) | 1y | DevOps | CLI logging |
| ui_action_audit | UI actions | id, user_id, route, action, payload, timestamp | (user_id,timestamp) | 1y | Product | UI audit tests |

# 9. Cache and Persistence Design
- **In-process cache**: per adaptor instance LRU (size 10k) storing `CacheEntry` with TTL 5 min; stores structured verdicts; invalidated on Redis pub/sub messages.
- **Redis cache**: Keys `verdict:{level}:{normalized_key}:policy{policy_version}`; values JSON (verdict + metadata). TTL 24 h default, 6 h for mixed-content, 4 h for low-confidence. Use Redis Cluster with replicas; sentinel for failover. Atomic writes with Lua script ensuring placeholder status not overwritten until classification completes. Placeholder entry fields: `status=pending`, `expires_at` short TTL 15 min.
- **Persistence**: Postgres `classifications` table authoritative; includes TTL and `next_refresh_at` to trigger reclassification jobs. `classification_versions` retains diff, reason, actor.
- **Canonicalization**: apply RFC 3986 normalization, punycode, lowercase, remove default ports, sort query params excluding tracking keys, limit path depth for caching strategy. Domain-level classification default; escalate to subdomain if subdomain matches dynamic list or heuristics; escalate to URL when path contains risky segments or override.
- **TTL rules**: extend on cache hits (sliding window) up to 72 h; low-confidence entries flagged for refresh at half TTL; model/taxonomy version change invalidates keys by prefix flush.
- **Stale handling**: on TTL expiry, adaptor returns cached result but triggers background refresh if `next_refresh_at` passed; for critical categories TTL locked to 24 h to ensure quick updates.
- **Auditability**: each cache mutation recorded with classification_id, previous version, actor (`system`, `llm-worker`, `admin`).

# 10. Policy Evaluation Design
- **Evaluation Order**: (1) System blocklist, (2) Manual overrides, (3) Allowlist, (4) Device-specific exceptions, (5) User-specific rules, (6) Group rules, (7) Source IP/subnet policies, (8) Location/site policies, (9) Time-of-day schedules, (10) Category/Subcategory decisions, (11) Risk/confidence thresholds, (12) Default tenant outcome.
- **Allowlist precedence**: highest except for explicit system blocklist (malware).
- **Unknown handling**: default `monitor` or `warn` per tenant. Optionally `require-approval` for high-risk roles.
- **Low-confidence**: if confidence <0.6, degrade recommended action to `monitor` and require operator confirmation via Pending Sites or override workflows.
- **High-risk**: categories 1, 5, 6, 7, 31, 32 override to `block` even if LLM suggested `warn`.
- **Repeated violations**: track per user/IP; escalate to `require-approval` after N events within window.
- **Exceptions**: Temporary domain allow/deny overrides; create audit entry with expiration.
- **Coaching page flow**: For `warn` or `require-approval`, adaptor returns redirect to captive portal with reason, classification summary, override request link.
- **Auditability**: Response includes `policy_rule_id`, `policy_version`, `trace_id`. All decisions logged to `audit_events`.

# 11. Squid + ICAP Integration Design
- **Why Squid**: Mature proxy with SSL bump, caching, ACLs, and ICAP support; reduces effort vs building proxy.
- **Squid responsibilities**: Handle client auth, SSL bump, caching static content, initial ACL filtering, log raw requests.
- **External adaptor responsibilities**: Advanced classification, policy evaluation, caching, logging, enforcement decisions, async classification.
- **Stay out of Squid**: LLM interactions, complex policy DSL, analytics, reporting, management UI.
- **Metadata use**: Squid populates ICAP headers (`X-Client-IP`, `X-User`, `X-Group`, `X-Device`, `X-Location`, `X-SSL-Bumped`, `X-Trace-Id`). TLS SNI provided even without SSL bump for domain-level classification.
- **ICAP specifics**: Use preview feature; adaptor responds 204 when no modification. Fail-open vs fail-close toggled via Squid ACL `adaptation_service_set`. eCAP reserved for future inline features.

# 12. Rust Backend Architecture
- **Workspace Layout** (as described earlier). Each crate uses latest Rust stable, `tokio` runtime.
- **Crate responsibilities**:
  - `common-types`: shared structs/enums (NormalizedRequest, Decision, ClassificationVerdict).
  - `config-core`: layered config loader (files, env, CLI) using `config` + `serde`.
  - `cache-client`: Redis abstraction with tracing, metrics, fallback logic.
  - `policy-dsl`: parser/compiler for JSON DSL.
  - `elastic-client`: typed Elasticsearch queries.
  - `audit-core`: audit event builder.
- **Services**: `icap-adaptor`, `policy-engine`, `admin-api`, `report-api` follow Axum/Tonic patterns.
- **Workers**: `llm-worker`, `reclass-worker` share queue modules.
- **Async runtime**: `tokio` multi-threaded. HTTP server via `axum` + `tower`. ICAP server built with `tokio` + custom parser.
- **Serialization**: `serde_json`, `serde_with`. Config via `config`. Logging via `tracing`, `tracing-opentelemetry`. Metrics via `prometheus` crate.
- **DB access**: `sqlx` with compile-time checks; migrations via `sqlx migrate`; CLI command `odctl migrate run` wraps it.
- **Testing libraries**: `tokio-test`, `proptest`, `insta`, `wiremock` for HTTP mocks, `testcontainers` for integration.

# 13. Frontend Architecture (React)
- **Tooling**: Vite + TypeScript, React 18, React Router v6, React Query for data fetching, Zustand for global state (user session, filters), Victory/Recharts for charts, Elastic UI (EUI) components for tables/filters, custom theme using IBM Plex Sans + color palette (teal/amber/charcoal gradient backgrounds).
- **Information Architecture**:
  - Dashboard Home: KPIs, line charts for allow/block trends, stacked area for categories, top IPs.
  - IP Investigation View: timeline, Sankey IP→category→action, table of requests, ability to trigger reclass.
  - User Investigation View: similar layout with user metadata.
  - Device Investigation View: device info, associated IPs, violations.
  - Policy Management: list, edit, preview DSL, simulation tool.
  - Override Management: table of overrides, creation modal, expiration controls.
  - Domain Allow / Deny list: prioritized manual exceptions with expiry, reason, and audit trail.
  - Classification Lookup: search normalized keys, show history.
  - Report Builder: drag/drop metrics/dimensions, exports CSV/JSON.
  - Audit Trail Viewer: filter by actor/action/time.
  - System Health/Status: service status cards, metrics spark lines.
  - Cache Inspection View: key search, TTL view.
  - Reclassification Ops: job list, trigger refresh, backlog metrics.
  - Configuration Summary: environment details, versions.
- **Routes**: `/dashboard`, `/investigation/ip/:ip`, `/investigation/user/:id`, `/investigation/device/:id`, `/policies`, `/overrides`, `/review`, `/lookup`, `/reports`, `/audit`, `/health`, `/cache`, `/reclassification`, `/config`.
- **State Management**: Auth context with OIDC tokens; React Query caching; Zustand for layout/global filters; WebSocket for live health.
- **Testing**: Jest + React Testing Library for components, Cypress/Playwright for E2E (running against docker-compose). Visual regression via Storybook Chromatic.
- **Deployment**: Vite build -> static assets served by Nginx container (port 19001). CI artifacts stored with hash.

# 14. CLI Architecture and Command Design
- **Binary**: `odctl` (Rust + Clap + Colorized output).
- **Auth**: uses service account token or interactive OIDC device flow; stored in `~/.odctl/config` with file permissions 600.
- **Command Tree**:
  - `odctl env check`
  - `odctl config validate --file config.yaml`
  - `odctl policy import/export --format json --file policies.json`
  - `odctl override import/export`
  - `odctl cache lookup --key domain:wikipedia.org`
  - `odctl cache invalidate --key ...`
  - `odctl classify inspect --key domain:example.com`
  - `odctl reclass trigger --scope domain:example.com`
  - `odctl health check --component all`
  - `odctl smoke run`
  - `odctl migrate run`
  - `odctl report query --ip 10.1.2.3 --range 24h`
  - `odctl audit query --actor user@example.com`
  - `odctl debug trace --trace-id <id>`
- **Flags**: `--output (table|json|yaml)`, `--endpoint`, `--auth-token`, `--tenant`, `--dry-run`.
- **Output**: Table via `tabled` crate; JSON for automation.
- **Error Handling**: exit codes per command, descriptive errors, suggestion for `--verbose`. Retries on transient errors.
- **Tests**: Unit for parser, integration with docker-compose stack, golden tests for outputs.
- **Docker Usage**: `docker run --rm -v ~/.odctl:/root/.odctl odctl smoke run` hitting compose network.

# 15. LLM Classification Design
- **Prompt** (exact):
```
You are a website classification engine for an enterprise web proxy.
Your task is to classify a requested destination for content filtering.
Return strict JSON only. Do not add markdown. Do not add commentary.

You will receive metadata such as:
- domain
- subdomain
- full_url
- url_path
- page_title (optional)
- snippet/content summary (optional)
- destination_ip (optional)
- ssl_inspected true/false
- existing reputation signals (optional)
- external threat flags (optional)
- tenant policy context (optional)

Rules:
1. Prefer domain-level classification unless URL-level evidence clearly changes the category.
2. Use subdomain-level classification when subdomain meaningfully differs from parent domain.
3. Mark mixed_content=true for platforms that host many unrelated user-generated pages.
4. Mark unknown=true when evidence is insufficient.
5. Keep reason_summary under 40 words.
6. Confidence must be 0.0 to 1.0.
7. recommended_action must be one of: allow, block, monitor, warn, review.
8. risk_level must be one of: very_low, low, medium, high, critical.
9. Output must be valid JSON matching the schema exactly.
```
- **JSON Schema**: Provided (entity_level, normalized_key, primary_category, subcategory, risk_level, confidence, recommended_action, sfw, age_restricted, dynamic_content, mixed_content, unknown, tags[], reason_summary).
- **Example Input JSON**:
```
{
  "domain": "example-streaming.com",
  "subdomain": "live",
  "full_url": "https://live.example-streaming.com/channel/123",
  "url_path": "/channel/123",
  "page_title": "Live concert stream",
  "snippet": "Watch concerts live",
  "destination_ip": "203.0.113.10",
  "ssl_inspected": true,
  "existing_reputation": "new",
  "external_flags": [],
  "tenant_context": {"industry": "finance"}
}
```
- **Example Output JSON**:
```
{
  "entity_level": "subdomain",
  "normalized_key": "live.example-streaming.com",
  "primary_category": "Streaming / Entertainment",
  "subcategory": "Video OTT",
  "risk_level": "medium",
  "confidence": 0.82,
  "recommended_action": "monitor",
  "sfw": true,
  "age_restricted": false,
  "dynamic_content": true,
  "mixed_content": true,
  "unknown": false,
  "tags": ["user-generated", "live-stream"],
  "reason_summary": "Live streaming platform hosting user content"
}
```
- **Validation**: JSON schema validation; enforce allowed enums; ensure confidence ∈ [0,1]. On invalid JSON: log, retry once, fallback to deterministic rules + mark `unknown=true`.
- **Low-confidence handling**: If confidence <0.6, set `recommended_action` to `monitor` and require manual operator follow-up.
- **Timeout handling**: 5 s for standard model, escalate to premium model with 10 s; on repeated timeout mark as unknown.
- **Retry logic**: up to 2 attempts, backoff 1s/3s; fallback to heuristics.
- **Anti-hallucination**: Provide strict schema, verify categories exist, limit context tokens, remove user-supplied prompts via sanitizer. If LLM output contains unrecognized category/subcategory, mark `unknown` and escalate to reviewer.
- **Token minimization**: send domain metadata, truncated HTML snippet, hashed content if large.
- **Deterministic pre-checks**: Known threat intel, blocklists, heuristics (domain age) executed before LLM to skip inference when possible.

# 16. First-Seen / Reclassification Flows
## First-Seen Normal Site
1. Request arrives; adaptor normalizes domain `news.example.com`.
2. In-process cache miss → Redis miss → Postgres miss; no override.
3. Adaptor writes placeholder `pending` with TTL 15 min (action=monitor) and enqueues job; returns `monitor` to Squid with coaching header disabled.
4. LLM worker processes job (domain-level), writes verdict (category News/Media, allow) to Postgres and Redis; publishes invalidation.
5. Next request hits cache, returns `allow` within 5 ms.

## First-Seen Suspicious Site
1. Domain flagged by threat intel (new TLD). Placeholder action defaults `block` (fail-closed) while classification pending.
2. Review queue entry created for analyst verification.
3. When classification returns high risk, policy engine enforces block; if unknown, keep warn + review.

## First-Seen Mixed-Content Platform
1. Domain `cdn.usercontent.com` recognized as mixed-content; adaptor sets `mixed_content=true` and caches domain-level `monitor`.
2. If URL pattern matches flagged path (e.g., `/malware/`), escalate to URL-level classification via LLM; result stored with more specific key.

## Known Cached Site
1. Cache hit found; TTL refreshed; decision returned with `cache_hit=true` flag for logging.

## Reclassification flow
1. TTL expires or taxonomy version increments; reclass worker enqueues job.
2. Cache entry marked stale but served while new classification pending.
3. Once new classification complete, caches updated, history recorded.

# 17. Reporting and Analytics Design
- **IP-based Dashboards**: Top domains by source IP (stacked bar), top blocked domains (bar), category distribution (treemap), allow/block trend (line), first-seen domains (table), risky attempts (heatmap), uncategorized traffic (top-N), user/device correlation (Sankey), NAT/shared-IP awareness (table), time-of-day heatmap, policy violations (bar). Each includes drill-down to request details.
- **User/Device Dashboards**: Similar metrics scoped to user/device, plus compliance status, repeated violations.
- **Management Dashboard**: KPIs (allow/block %, LLM cost, unknown rate), category trends, policy coverage.
- **Security Dashboard**: Alerts for high-risk attempts, reclassification backlog, override spikes.
- **Graph Types**: line, stacked area, bar, heatmap, treemap, Sankey, top-N lists, drill-down tables, percentile charts, geo maps (IP geolocation), timeline scatter.
- **Report Builder**: choose dimensions (IP, user, device, domain, category, action), metrics (counts, bytes, violations), timeframe; exports CSV/JSON via API.

# 18. Elasticsearch / Kibana Logging and Monitoring Design
- **Logs**: Rust services log structured JSON with ECS fields; Squid logs ingested via Filebeat (adds trace_id). Audit events stored in both Postgres and `audit-events-*` index. CLI/UI actions logged.
- **Metrics**: Exposed via Prometheus; scraped metrics shipped to Elastic APM or Prometheus+Grafana; integrated in Kibana Observability.
- **Dashboards**: Service Health, ICAP latency, Cache hit ratio, LLM usage, Queue depth, Elasticsearch ingestion, Kibana usage stats, CLI command failures.
- **Alerting**: Based on metrics thresholds (e.g., `cache_hit_ratio < 0.9`, `llm_timeout_rate > 5%`, `reclassification_backlog > 1000`, `elasticsearch_index_failures > 0`), integrated into Kibana Alerting.
- **Searchable Fields**: `trace_id`, `decision_id`, `source_ip`, `user_id`, `domain`, `category`, `verdict`, `policy_rule_id`.
- **Retention**: 90d hot, 1y warm; ILM policies; PII masking for user names via hashing. Access control enforced via Elastic security features.
- **Correlation**: Trace IDs propagate from Squid to adaptor to workers to UI/CLI actions for end-to-end investigation.

# 19. Docker / Docker-Compose Development and Test Design
- **Services**: `squid`, `icap`, `policy`, `admin`, `classify`, `report`, `llm-worker`, `reclass-worker`, `redis`, `postgres`, `elasticsearch`, `kibana`, `frontend`, `cli-runner`, `event-ingester`, `prometheus`, `filebeat`, `smoke-tests`.
- **Ports**: Admin API 19000, UI 19001, CLI gRPC 19002, ICAP 1344, Redis 6379, Postgres 5432, ES 9200, Kibana 5601, Prometheus 9090.
- **Dockerfiles**: Multi-stage (builder -> runtime) for Rust (using `cargo-chef`), React (Vite + Node), CLI, workers.
- **Compose files**: `docker-compose.yml` (dev), `docker-compose.test.yml` (integration), `docker-compose.smoke.yml` (smoke). Use `depends_on` with healthchecks.
- **Env management**: `.env` file with non-secret defaults, `.env.secrets` loaded via docker secrets. Secrets (LLM API, OIDC) mounted as files.
- **Volumes**: persistent for Postgres, Redis, ES, Squid logs, Kibana config.
- **Startup sequence**: `docker-compose up -d postgres redis elasticsearch`; wait for health; run `odctl migrate run`; verify `config/canonical-taxonomy.json` is mounted/available; start rest services; run smoke tests `odctl smoke run`.
- **Post-start validation**: check `docker compose ps`, run `curl http://localhost:19000/health/ready`, open Kibana.
- **CI/CD**: pipeline uses compose to run unit/integration/perf; artifacts stored per build.

# 20. API Specifications
## 20.1 Policy Decision Lookup
- **Endpoint**: `POST /api/v1/policy/decision`
- **Request**:
```
{
  "trace_id": "uuid",
  "normalized_key": "domain:example.com",
  "entity_level": "domain",
  "source_ip": "10.1.1.5",
  "user_id": "user123",
  "group_ids": ["finance"],
  "device_id": "laptop-42",
  "location": "HQ",
  "method": "GET",
  "ssl_inspected": true,
  "policy_context": {"time": "2026-03-22T18:00:00Z"}
}
```
- **Response**:
```
{
  "decision_id": "uuid",
  "action": "allow|block|warn|monitor|review|require-approval",
  "reason": "PolicyRule#12",
  "policy_rule_id": "rule-12",
  "policy_version": "v45",
  "cache_hit": true,
  "verdict": {
    "category": "News / Media",
    "subcategory": "National",
    "confidence": 0.92,
    "risk_level": "low"
  },
  "placeholder": false,
  "ttl_seconds": 3600
}
```
- **Status Codes**: 200 success, 202 placeholder (first-seen), 400 validation error, 401 unauthorized, 403 forbidden, 429 rate limit, 500 internal.
- **Auth**: mTLS or service token. Rate limit per IP.
- **Error Contract**: `{"error_code":"VALIDATION_ERROR","message":"..."}`.
- **Tests**: UT for validation, IT for policy precedence, Smoke for cached flows.

## 20.2 Classification Submit
- `POST /api/v1/classifications` (admin only). Body includes normalized_key, manual verdict, reason. Response 201 with classification_id. Validation ensures category in taxonomy, reason required. Tests: UT, IT, security.

## 20.3 Classification Fetch
- `GET /api/v1/classifications/{normalized_key}` returns latest verdict, history summary. 404 if none.

## 20.4 Overrides API
- `POST /api/v1/overrides`, `GET /api/v1/overrides`, `DELETE /api/v1/overrides/{id}`. Includes scope (domain/user/ip), action, expiry.

## 20.5 Domain Allow / Deny Overrides
- Manual decisions are managed through `/api/v1/overrides` using `scope_type=domain` and `action=allow|block`.

## 20.6 Report APIs
- `GET /api/v1/reports/ip-activity?ip=10.1.1.5&range=24h&metrics=top-blocked` returns aggregated data.
- Provide endpoints for other dashboards (user/device, category trend, override stats).

## 20.7 Audit Query API
- `GET /api/v1/audit-events?actor=user&action=override.create&from=...&to=...`.

## 20.8 Cache Inspection API
- `GET /api/v1/cache/{key}` returns cache entry; `DELETE /api/v1/cache/{key}` invalidates.

## 20.9 Reclassification API
- `POST /api/v1/reclassification` with scope + reason; returns job_id. Rate-limited.

## 20.10 Health APIs
- `GET /health/live` (always 200 if process running), `GET /health/ready` (checks DB/Redis/queue). Smoke tests depend on these.

## 20.11 CLI Support APIs
- gRPC/REST endpoints for CLI commands (smoke, migrations, taxonomy). Provide `POST /api/v1/smoke/run` for automation.

For each endpoint specify auth (OIDC bearer + scopes), rate limits, idempotency (override create uses client-provided id to prevent duplicates), validation (JSON schema), tests (unit/integration/smoke). Full OpenAPI spec stored at `docs/openapi.yaml` (future work).

# 21. Detailed Implementation List
Provide per component module breakdown (sample excerpt; full list in repo doc supplements):

| Component | Module/File | Responsibilities | Interfaces | Dependencies | Logging/Metrics | Tests |
| --- | --- | --- | --- | --- | --- | --- |
| ICAP Adaptor | `src/icap_server.rs` | Listen for ICAP requests, dispatch to handlers | ICAP TCP | tokio, tracing | log trace_id, latency metrics | UT: handshake parser; IT: icap client |
| ICAP Adaptor | `src/normalizer.rs` | Normalize URL/domain | internal | `url` crate | log normalization warnings | UT: valid/invalid URLs |
| ICAP Adaptor | `src/cache.rs` | Multi-level cache client | Redis/in-mem | redis crate | log hits/misses | UT/IT for cache fallback |
| ICAP Adaptor | `src/policy_client.rs` | gRPC to policy engine | gRPC | tonic | log decision time | UT: serialization |
| ICAP Adaptor | `src/queue.rs` | Publish classification jobs | Redis Streams | redis | log failures | UT: publish errors |
| Policy Engine | `src/dsl_parser.rs` | Parse DSL | internal | pest | log parse errors | UT: precedence |
| Policy Engine | `src/evaluator.rs` | Evaluate contexts | gRPC | `policy-dsl` | metrics on latency | UT: rule combos |
| Classification Service | `src/controllers/overrides.rs` | Override APIs | REST | axum, sqlx | log actor | UT: validation |
| LLM Worker | `src/prompt_builder.rs` | Build prompt JSON | internal | serde_json | log prompt hash | UT |
| LLM Worker | `src/validator.rs` | Validate LLM JSON | internal | jsonschema | log invalid count | UT |
| LLM Worker | `src/persister.rs` | Write verdicts | Postgres | sqlx | log success/failure | UT |
| Reclass Worker | `src/scheduler.rs` | Determine jobs | Postgres | sqlx | metrics backlog | UT |
| Admin API | `src/routes/health.rs` | Health endpoints | REST | axum | log status | Unit tests |
| Report API | `src/queries/ip_activity.rs` | Query ES | Elasticsearch | elastic-client | log query latency | UT + integration |
| CLI | `src/commands/policy.rs` | Policy import/export | REST/gRPC | reqwest/tonic | log results | UT/IT |
| React | `src/pages/IpInvestigation.tsx` | Render IP view | REST hooks | React Query | instrumentation via custom hook | Jest + Cypress |

Full table enumerates all significant modules across services/workers/UI/CLI.

# 22. Function-by-Function Implementation Matrix
For each critical function provide details (sample subset; full list extends to all functions in Appendix). Example entries:

| Function | Location | Purpose | Inputs | Outputs | Side Effects | Errors | Validation | Tests | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `normalize_request(req: &IcapHttpRequest) -> Result<NormalizedRequest>` | `icap-adaptor/src/normalizer.rs` | Canonicalize HTTP request | Raw ICAP request | Normalized struct | none | Invalid URL, missing host | RFC 3986 rules, length limits | UT: valid URL, punycode, IPv6, missing host | Unit test report |
| `build_cache_key(norm: &NormalizedRequest) -> CacheKey` | `cache.rs` | Generate cache key string | normalized request | cache key | none | n/a | ensures entity level prefix | UT: domain/subdomain/url variants | UT logs |
| `lookup_cache(key)` | `cache.rs` | Multi-level cache lookup | CacheKey | Option<Decision> | accesses Redis | Redis timeout | fall back to DB | UT/IT hits/miss/timeouts | metrics screenshot |
| `evaluate_policy(ctx)` | `policy-engine/src/evaluator.rs` | Determine action via DSL | PolicyContext | Decision | increments counters | missing policy | ensures precedence ordering | UT: override precedence, low-confidence escalation | policy test report |
| `enqueue_classification(job)` | `queue.rs` | Publish to Redis Stream | job struct | ack | writes to stream | Redis unavailable | validate job size | UT, IT with redis | queue metrics |
| `process_classification(job)` | `llm-worker/src/processor.rs` | End-to-end classification | job struct | none | DB writes, Redis updates | LLM timeout, invalid response | ensures schema compliance | UT for invalid JSON, IT with mock LLM | worker logs |
| `validate_llm_response(json)` | `validator.rs` | Schema validation | JSON string | ClassificationVerdict | none | Schema error | JSON schema validation | UT: invalid enum, missing fields | test results |
| `persist_classification(verdict)` | `persister.rs` | Write DB + cache | verdict | classification_id | commits transaction | DB error | ensures version increment | UT: db failure, duplicate key | DB logs |
| `record_audit(event)` | `audit-core` | Append audit entry | AuditEvent | none | Postgres + ES writes | DB failure | ensures hash chain | UT/IT | audit sample |
| `get_ip_activity_report(params)` | `report-api/src/queries/ip.rs` | Build ES query | filters | aggregated data | ES query | ES error | validate time range | UT: invalid range, IT: real query | report evidence |
| `run_cli_smoke(args)` | `cli/src/commands/smoke.rs` | Execute smoke tests | CLI args | exit code | REST calls | endpoint down | Validate outputs | UT: param parsing, IT: hitting compose | CLI log |
| `useIpActivityQuery(ip, filters)` | `web-admin/src/hooks` | React hook for data | ip, filters | query state | fetch to API | fetch fail | ensures ip format | Jest tests, Cypress | UI screenshot |
| ... | ... | ... | ... | ... | ... | ... | ... | ... | ... |

# 23. Unit Test Matrix
Provide table referencing every function (per Section 22). Example entries:

| Test Name | Function | Scenario | Input | Expected Output | Mocks | Edge Cases |
| --- | --- | --- | --- | --- | --- | --- |
| `test_normalize_request_valid_http` | `normalize_request` | Standard HTTP GET | `http://Example.com/path` | Lowercase host, path preserved | None | Case sensitivity |
| `test_normalize_request_punycode` | same | IDN domain | `http://bücher.de` | `xn--bcher-kva.de` | None | Mixed script |
| `test_normalize_request_missing_host_err` | same | No host header | request without Host | Error | None | required host |
| `test_cache_lookup_hit` | `lookup_cache` | Key present | key | decision | Redis mock returns value | n/a |
| `test_cache_lookup_timeout` | same | Redis unavailable | key | fallback to DB | Mock redis error, db success | ensures warning |
| `test_evaluate_policy_override_precedence` | `evaluate_policy` | Override + rule conflict | context with override | action=allow/block | Mocks for overrides | Precedence |
| `test_llm_validation_invalid_enum` | `validate_llm_response` | Risk level invalid | sample json | error | none | ensures schema |
| `test_persist_classification_db_failure` | `persist_classification` | DB error | verdict | retry/backoff | Mock DB fail | ensures error path |
| `test_get_ip_activity_invalid_range` | `get_ip_activity_report` | from>to | filters | validation error | None | Input validation |
| `test_cli_cache_lookup_table_output` | CLI command | Table output | args | table string | Mock API | Format |
| `test_useIpActivityQuery_error_state` | React hook | API 500 | ip filter | error state set | Mock fetch | UI error message |
| ... (covering every function) ... |

# 24. Smoke Test Suite
| ID | Purpose | Prerequisites | Steps | Expected Result | Failure Interpretation | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| ST-01 | Squid reachable | Compose up | `curl -x http://localhost:3128 http://example.com` | Receives proxy auth or response | Squid down if timeout | curl output |
| ST-02 | ICAP adaptor reachable | Squid + adaptor | `c-icap-client -i localhost -p 1344 -s reqmod ...` | 204/200 response | adaptor offline | client log |
| ST-03 | Admin API health | Services running | `curl http://localhost:19000/health/ready` | 200 JSON | dependencies down | response log |
| ST-04 | Redis connectivity | `odctl health check --component redis` | success message | redis down if fail | CLI output |
| ST-05 | Postgres connectivity | `odctl health check --component postgres` | success | DB down | CLI log |
| ST-06 | Elasticsearch connectivity | `odctl health check --component elasticsearch` | success | ES down | CLI log |
| ST-07 | Kibana availability | Browser `http://localhost:5601` or `curl` | 200 | Kibana down | screenshot |
| ST-08 | Known cached allow flow | Preload classification | HTTP request via Squid; verify decision allow | mismatch indicates cache issue | Access log |
| ST-09 | Known cached block flow | same for blocked site | block page seen | failure indicates policy issue | screenshot |
| ST-10 | First-seen domain async | Request new domain, ensure 202 placeholder + job enqueued | placeholder decision, classification completes | failure -> queue issue | logs |
| ST-11 | Report query | `odctl report query --ip ...` | JSON data returned | indicates report API down | CLI output |
| ST-12 | Audit log write/read | Create override -> fetch audit entry | entry exists | indicates audit offline | API log |
| ST-13 | CLI smoke command | `odctl smoke run` | PASS | CLI misconfig if fail | log |
| ST-14 | Docker-compose validation | `docker compose ps` all healthy | ensures environment ready | Compose logs |
| ST-15 | Frontend load/auth | open UI, login via OIDC | Dashboard loads | UI/back-end broken if fail | screenshot |

# 25. Integration Test Suite
| Test | Systems | Scenario | Input | Expected | Failure Criteria | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| IT-01 Squid→ICAP | Squid, ICAP svc | End-to-end REQMOD | HTTP requests via Squid | Policy action header | No response or wrong action | Access log + adaptor log |
| IT-02 Adaptor→Redis | ICAP, Redis | Cache hit/miss fallback | repeated requests | first miss, second hit | repeated LLM jobs indicate failure | Redis metrics |
| IT-03 Adaptor→Policy | ICAP, Policy svc | Precedence evaluation | context requiring override | expected action | mismatch | policy logs |
| IT-04 Adaptor→Postgres | ICAP, DB | Decision persistence | high volume requests | DB writes recorded | missing entries | DB query |
| IT-05 Worker→LLM | LLM worker, mock LLM | Validate prompt/response | job with metadata | stored classification | invalid JSON or no storage | worker log |
| IT-06 Override service→Policy | Classification svc, policy | Override creation → effect | create override then request | action reflect override | no change | audit logs |
| IT-07 Event pipeline→ES | Event ingester, ES | Bulk insert | generated events | searchable docs | missing docs | Kibana search |
| IT-08 Report API | Report svc, ES | IP query | API request | aggregated data | 500 error or wrong data | API logs |
| IT-09 Frontend→Backend | UI + Admin API | Review queue workflow | approve item via UI | API call success, state updated | UI error | Cypress video |
| IT-10 CLI→Backend | CLI + Admin API | Policy import/export | CLI command | success message, data present | CLI failure | CLI log |
| IT-11 CLI→compose | CLI in container hitting compose network | `odctl smoke run` | PASS | failure indicates network issue | Compose logs |

# 26. Performance Test Plan
| Test | Workload | KPI Targets | Metrics | Pass/Fail |
| --- | --- | --- | --- | --- |
| PT-01 ICAP throughput | 50k rps mixed allow/block using k6 | `squid_to_icap_latency` p95 < 40 ms, error <0.1% | latency, CPU, cache ratio | fail if thresholds exceeded |
| PT-02 Cache efficiency | 90% repeated domains | Cache hit ratio > 0.95 | `cache_hit_ratio`, Redis ops | fail if <0.9 |
| PT-03 First-seen burst | 10k unique domains/min | queue depth <500, placeholder rate manageable | `first_seen_domain_rate`, queue size | fail if backlog >1000 |
| PT-04 ICAP latency | measure per path | p99 < 80 ms | `policy_decision_latency` | fail if >80 ms |
| PT-05 Report query | 30-day IP report | <3 s response | `report_query_latency` | fail if >3s |
| PT-06 LLM backlog stress | simulate slow LLM | backlog clears <15 min | `llm_invocation_count`, `llm_timeout_rate` | fail if backlog persists |
| PT-07 Reclassification wave | taxonomy change reclass 100k entries | completes <2 hr | `reclassification_backlog` | fail if >2hr |
| PT-08 ES indexing | 30k events/sec | no drops | `elasticsearch_index_failures` | fail if >0 |
| PT-09 Kibana dashboard | load top dashboards | <5 s load time | synthetic monitoring | fail if >5s |
| PT-10 CLI bulk ops | policy import 10k rules | <60 s | CLI time | fail if >60s |

# 27. Security Test Plan
- **AuthN/AuthZ**: tests verifying OIDC tokens, mTLS, RBAC matrix (admin, auditor, analyst, read-only). Tools: ZAP, custom scripts.
- **Injection**: fuzz HTTP headers, URLs, CLI inputs. Ensure normalization rejects malicious payloads.
- **Prompt Injection**: craft malicious HTML asking LLM to ignore rules; verify validator catches invalid outputs; ensure sanitization removes `<script>` etc.
- **Replay/Idempotency**: repeated override create with same client id returns existing entry, logs attempt.
- **Audit Integrity**: tamper test verifying hash chain detection.
- **Secret/config leakage**: scanning containers for secrets, verifying environment variables not exposed via APIs.
- **PII masking**: ensure logs mask usernames where necessary; test by generating sample data.
- **Transport security**: verify TLS 1.2+ enforced, certificate validation.
- **Fail-open/close**: simulate adaptor outage; ensure Squid behavior matches config and is logged.
- **Frontend route auth**: attempt to access admin pages without permission; expect redirect/403.
- **CLI credential handling**: ensure config file permissions enforced, tokens redacted in logs.
- **Elasticsearch/Kibana access**: verify RBAC roles restrict index access.

# 28. Deployment / Rollback / Handover Checklists
## Deployment Checklist
1. Verify change approvals, maintenance window, runbooks up to date.
2. Backup Postgres, export policies/classifications.
3. Deploy database migrations (`odctl migrate run`).
4. Deploy services sequentially (policy -> adaptor -> workers -> admin API -> UI) using rolling/blue-green.
5. Update Redis/ES index templates if needed.
6. Run smoke tests (Section 24) and record results.
7. Update Kibana dashboards if changed.
8. Notify stakeholders, update status page.

## Rollback Checklist
1. Revert service images to previous versions.
2. Restore Postgres snapshot if schema incompatible.
3. Flush Redis cache if corrupted.
4. Disable new features via feature flags.
5. Run smoke tests to confirm previous state.

## Handover Checklist
1. Deliver runbooks (operations, incident, classification escalations).
2. Provide dashboard links, alert definitions, on-call contact list.
3. Transfer access to Kibana, CLI, repo.
4. Conduct training session for SOC/IT.
5. Archive evidence package.

# 29. Evidence of Complete Development
| Artifact | Owner | Generation Method | Acceptance Criteria | Storage |
| --- | --- | --- | --- | --- |
| Architecture/Design Doc (this file) | Lead Architect | Markdown in repo | Approved by architecture board | Git `docs/engine-adaptor-spec.md` |
| RFC Mapping Sheet | Security Architect | Table in doc | All relevant RFCs mapped | Git/Confluence |
| Module Implementation Checklist | Tech Lead | Spreadsheet | 100% modules complete | Confluence |
| Code Review Records | Team Leads | PR history | All PRs reviewed, approvals present | GitHub |
| Unit Test Report | QA Lead | CI log | 100% pass, coverage ≥85% | CI artifacts |
| Integration Test Results | QA Lead | docker-compose IT suite | All tests pass | CI |
| Smoke Test Logs | DevOps | `odctl smoke run` output | PASS | Runbook attachments |
| Performance Test Report | Perf Engineer | k6/Gatling results | KPIs met | SharePoint |
| Security Test Report | Security Engineer | Pen test results | All critical findings resolved | Security vault |
| Deployment Record | DevOps | Change ticket | Completed with timestamps | ITSM |
| Runbook & SOP | SRE Lead | Markdown/PDF | Reviewed by Ops | Knowledge base |
| Kibana Dashboard Screenshots | SOC Lead | Kibana exports | Verified metrics | Evidence folder |
| Sample Logs/Audit | QA | Captured JSON logs | Show trace linkage | Evidence folder |
| API Contract Validation | QA | OpenAPI lint/test | Pass | Repo `docs/api-validation` |
| Schema Migration Evidence | DBA | Migration logs | Success | Ops repo |
| Docker Compose Validation | DevOps | Compose logs | Healthy | CI attachments |
| Frontend Build Artifact | Frontend Lead | Build hash | Deployable artifact | Artifact repo |
| CLI Validation | DevOps | CLI command outputs | PASS | Evidence folder |
| Release Notes | TPM | Document | Approved by stakeholders | Knowledge base |
| Rollback Plan | DevOps | Document | Reviewed | Ops repo |
| Operational Signoff | SRE Lead | Signoff form | Completed | ITSM |
| QA Signoff | QA Lead | QA exit report | Completed | ITSM |
| Security Signoff | Security Lead | Document | Completed | ITSM |
| Product Signoff | TPM | Document | Completed | ITSM |

# 30. Definition of Done by Component
| Component | DoD |
| --- | --- |
| Squid Integration | squid.conf updated, ICAP services tested, logs shipping to Elasticsearch, fail-open/close documented, smoke tests ST-01/02 pass, rollback plan documented. |
| ICAP Adaptor | Code complete with unit/integration/perf tests, metrics/logs configured, documentation + runbook, security review, rollback image, evidence of cache hit ratios. |
| Policy Engine | DSL parser/evaluator implemented, full unit tests for precedence, integration with admin UI, metrics/alerts, docs, security review, rollback plan. |
| Redis Cache Layer | Key schema implemented, TTL logic tested, monitoring of latency/hit ratios, failover tested, documentation. |
| Classifier Worker | Prompt builder/validation/persistence implemented, tests (unit/integration/security), monitoring (LLM metrics), fallback behavior documented, evidence of sample classifications. |
| Reclassification Worker | Job scheduler, backlog metrics, tests, runbook for taxonomy upgrades. |
| Reporting Backend | ES queries implemented, APIs documented, dashboards validated, tests, perf results. |
| Elasticsearch/Kibana | Index templates, ILM policies, dashboards, alerting, access controls, documentation. |
| Admin APIs | All endpoints implemented, auth/rate limiting enforced, OpenAPI doc, tests, monitoring. |
| Admin UI | React pages complete with role-based behavior, tests (unit/e2e), build artifacts, accessibility check, documentation. |
| CLI | Command tree implemented, tests, packaging (brew/deb/container), docs, security review. |
| Observability | Metrics/logs/traces wired, dashboards and alerts created, documentation. |
| Deployment Automation | Dockerfiles, compose, CI/CD pipelines, scripts validated, docs, rollback. |
| Test Automation | CI running unit/integration/smoke/perf/security suites, results stored, failure alerts configured. |

# 31. Traceability Matrix
| Requirement | Component(s) | Function(s) | Test(s) | Evidence |
| --- | --- | --- | --- | --- |
| R1: ICAP integration | Squid, ICAP adaptor | `handle_reqmod`, `normalize_request`, `evaluate_policy` | ST-01/02, IT-01 | squid.conf, adaptor logs |
| R2: No repeated classification | ICAP adaptor, Redis cache, LLM worker | `lookup_cache`, `insert_placeholder`, `process_classification` | UT cache suite, PT-02, IT-02 | Cache metrics report |
| R3: LLM classification schema | LLM worker | `build_llm_prompt`, `validate_llm_response` | UT prompt/validation, IT-05, Security prompt injection | LLM worker logs |
| R4: IP-based reporting | Report API, UI | `get_ip_activity_report`, `useIpActivityQuery` | IT-08, UI tests | Kibana dashboards |
| R5: React UI pages | React app | All page components | Jest/Cypress | UI screenshots |
| R6: CLI capabilities | CLI | `cmd_policy_import`, `cmd_smoke` etc | CLI unit/integration, ST-13 | CLI logs |
| R7: Logging/monitoring | Observability | `event_ingester`, metrics exporters | ST-07, PT metrics | Kibana dashboards |
| R8: Deployment readiness | DevOps | Compose files, Dockerfiles | Smoke tests, Deployment checklist | Compose logs, runbook |
| R9: Security compliance | All | Auth middleware, audit logging | Security tests | Security report |
| ... (complete table maintained in repo) ... |

# 32. Risks and Mitigations
| Risk | Impact | Likelihood | Mitigation | Owner |
| --- | --- | --- | --- | --- |
| LLM cost overruns | High cost | Medium | Cache aggressively, use cheaper model pipeline, monitor `llm_invocation_count` | TPM + Security |
| Redis outage | Hot-path latency spike | Medium | Redis Cluster + Sentinel, local cache fallback, alerting | SRE |
| Elasticsearch scaling issues | Reporting degradation | Medium | ILM, shard sizing, hot/warm nodes, autoscaling | SRE |
| False positives blocking business apps | Productivity loss | Medium | Review queue, manual overrides, policy simulation | Policy team |
| Prompt injection | Security incident | Low | Sanitization, schema validation, prompt guardrails | Security |
| Compliance/privacy concerns | Regulatory risk | Medium | PII masking, RBAC, audit, legal review | Security/Legal |
| Deployment complexity | Delays/outages | Medium | Automated compose/k8s pipelines, runbooks, blue/green | DevOps |

# 33. Final Recommendations
- Implement in phases: (1) ICAP adaptor + cache + policy engine + CLI health; (2) async classification workers + taxonomy; (3) reporting/UI; (4) advanced analytics and automation.
- Prioritize instrumentation/observability early to validate latency and caching goals.
- Conduct threat modeling for LLM pipeline and ensure strict schema validation before launch.
- Schedule joint dry-runs of docker-compose, smoke tests, and LLM failover scenarios before production cutover.
- Maintain close alignment between SOC, DevOps, and Product teams via shared dashboards and evidence artifacts to ensure audit readiness.
