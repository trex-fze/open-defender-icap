# Stage 5 RFC Addendum – Admin API, UI & CLI

**Parent Sections**: `docs/engine-adaptor-spec.md` §§13, 14, 18, 19, 23.

## Objectives
1. Expand Admin API with policy, override, reporting, cache inspection endpoints.
2. Build React admin UI routes (dashboards, investigations, policy mgmt, allow/deny list, health).
3. Implement CLI (`odctl`) commands for env validation, policy/override import-export, cache ops, smoke tests.
4. Enforce RBAC/SSO (OIDC) across API/UI/CLI.

## Checklist
- [x] Admin API endpoints per Spec §23 (policy decision support, override CRUD, reporting queries).
- [x] React UI navigation + role-aware views per Spec §18 (live hooks w/ mock fallback, role-aware routing, gradients/typography implemented).
- [x] CLI command tree per Spec §19 with table/json output (clap-based `odctl` covering policy, override, review, cache, reporting, logs, smoke).
- [x] Auth integration (OIDC flows, token storage) – Spec §§18–19 (Admin API enforcing HS256 JWTs, React AuthProvider storing access tokens, `odctl auth login` implementing OIDC device flow with refreshable sessions).
- [x] Unit + e2e tests (React Testing Library, Cypress, CLI integration) – Spec §25 (Vitest suites cover auth/context/hooks, odctl integration tests mock Admin API, and Cypress runs login/dashboard/investigations/policies/overrides/reports with axe serious+critical.)
- [x] Accessibility + UX guidelines (fonts, gradients, responsive) – Spec §18 instructions (contrast polish applied, focus styles for scrollable tables, typography/color tokens match RFC, axe suite passing).

## Admin API Contract (Spec §23)

| Resource | Method | Path | Description | RBAC Roles |
| --- | --- | --- | --- | --- |
| Policies | GET | `/api/v1/policies?include=drafts&search=foo` | Paginated list, filter by status/text, include latest published+draft metadata and linked taxonomy stats. | `policy-viewer`, `policy-editor`, `policy-admin` |
| Policies | POST | `/api/v1/policies` | Create draft policy `{name, description, rules_yaml, segment}`; responds with version + audit id. | `policy-editor`, `policy-admin` |
| Policies | PUT | `/api/v1/policies/:id` | Update draft using optimistic `If-Match`; rejects published revisions. | `policy-editor`, `policy-admin` |
| Policies | POST | `/api/v1/policies/:id/publish` | Promote draft to active, snapshot into `classification_versions`, invalidate cache. | `policy-admin` |
| Policies | POST | `/api/v1/policies/:id/validate` | Dry-run compile DSL against sample traffic; returns lint + impact stats. | `policy-editor`, `policy-admin` |
| Overrides | GET/POST/PUT/DELETE | `/api/v1/overrides` | Existing endpoints plus filters (`scope_type`, `status`, `creator`, `expires_before`) and `DELETE /api/v1/overrides/bulk` for CSV payloads. | Override roles (unchanged) |
| Taxonomy | GET/POST/PUT/DELETE | `/api/v1/taxonomy/categories`, `/api/v1/taxonomy/subcategories` | CRUD wrappers around taxonomy tables from migration `0004`. | `policy-editor`, `policy-admin` |
| Cache | GET | `/api/v1/cache-entries/:key` | Inspect cache entry (value, expires_at, source). | `policy-viewer`, `auditor` |
| Cache | DELETE | `/api/v1/cache-entries/:key` | Purge cache entry + emit invalidation to Redis stream. | `policy-admin` |
| Reporting | GET | `/api/v1/reporting/traffic?range=24h&top_n=10` | Returns live Elasticsearch traffic trend + top blocked domains/categories with fallback semantics when sparse fields are present. | `auditor`, `policy-admin` |
| CLI Logs | GET | `/api/v1/cli-logs?operator_id=alice@example.com` | Fetch `cli_operation_logs` for auditing CLI invocations. | `auditor`, `policy-admin` |

Shared behaviors:
- Lists return `{ "data": [...], "meta": { "page": 1, "page_size": 50, "has_more": false } }`.
- Errors reuse `{ code, message, details }` envelope defined in Stage 4.
- Mutations emit `AuditEvent` entries and, where applicable, trigger `CacheInvalidator` to keep ICAP adaptor + workers consistent.
- `page` defaults to 1, `page_size` defaults to 50 (max 200). `sort` query uses `field:dir` syntax (`created_at:desc`).
- Bulk override import/export capped at 100 rows per job; progress tracked via `cli_operation_logs`.

## Auth & RBAC Model (Spec §§14, 18–19)
- OIDC HS256 JWT is the primary auth path. Static `X-Admin-Token` remains for bootstrap/air-gapped scenarios only.
- Roles:
  - `policy-admin`: publish policies, delete overrides, manage RBAC + tokens.
  - `policy-editor`: edit drafts, taxonomy, overrides, run validations.
  - `policy-viewer`: read-only dashboards, cache inspect, reporting.
  - `auditor`: read reporting, CLI logs, audit trails.
- Admin API middleware validates issuer/audience/exp and extracts roles from `roles` array or space-delimited `scope` claim. Expired/invalid tokens return `401`.
- Browser UI uses Authorization Code + PKCE; CLI uses Device Code with fallback API token. Tokens cached securely (macOS Keychain, Windows Credential Manager, SecretService) along with refresh token expiry so background refresh can occur.
- React router enforces route guards via `AuthProvider` context; CLI commands check role claims before calling protected endpoints to fail fast client-side.

## React Admin UI (Spec §18)

| Route | Description | Primary Components |
| --- | --- | --- |
| `/login` | PKCE callback + device flow helper page. | `AuthProvider`, `PkceCallback`, `DevicePrompt` |
| `/dashboard` | KPI tiles (blocked URLs, review SLA, LLM latency) + trend charts bound to reporting aggregates. | `KpiGrid`, `TrendChart`, `SlaGauge` |
| `/investigations` | Search normalized keys/domains, show classification history, cache entry details, reclassification job status. | `SearchBar`, `Timeline`, `CacheCard` |
| `/policies`, `/policies/:id` | List view + editor with YAML diff, validation, publish workflow, change log. | `PolicyTable`, `PolicyEditor`, `PublishModal` |
| `/overrides` | Domain Allow / Deny CRUD table with filters and expiration reminders. | `OverrideTable`, `OverrideForm` |
| `/taxonomy` | Category/subcategory management with drag/drop tree + default action picker. | `CategoryTree`, `SubcategoryForm` |
| `/reports` | Dimension+period filters, chart + table, CSV export. | `ReportFilterBar`, `MetricsChart`, `DownloadButton` |
| `/settings/rbac` | Role assignment matrix, API token rotation, CLI audit log viewer. | `RoleMatrix`, `ApiTokenList`, `CliLogTable` |

Design language:
- Typography: Space Grotesk + IBM Plex Sans (no system defaults). Baseline grid uses `clamp()` spacing.
- Color tokens: `--ink`, `--slate`, `--citrus`, `--peach`; hero sections use subtle gradients instead of flat white.
- Motion: page fade-in + staggered KPI cards (120–150 ms), focus-visible outlines always enabled.
- Responsive: CSS grid for cards, drawer-style navigation under 768 px.

## CLI (`odctl`) Command Tree (Spec §19)

```

- Output defaults to ASCII tables; `--json` flag produces machine-readable responses. All commands honor `--token` override for CI.
- `auth login` performs OIDC device flow: display code, open browser, poll token endpoint, store encrypted refresh/access tokens. `auth status` shows expiry + roles; `auth logout` wipes tokens.
- `policy push` validates YAML locally then calls `/policies/validate` before `/policies/:id` updates. `policy pull` writes latest published policy to disk for Git review.
- `override apply` loads CSV, previews diff, and calls bulk endpoint; CLI records result to Admin API which stores row in `cli_operation_logs`.
- `smoke run` reuses Stage 4 fixtures to ensure ICAP adaptor → Redis → LLM worker pipeline healthy; CLI writes summary to reporting endpoint for dashboards.

## Testing & Accessibility (Spec §25)
- React: unit tests with React Testing Library + Jest for forms, role guards, API hooks.
- Cypress e2e: login, edit policy, apply domain allow/deny override, download report. Screenshots captured for evidence.
- CLI: `assert_cmd` + `wiremock` harness to simulate Admin API responses; device-flow path tested via mocked token endpoint.
- Accessibility: `axe-core` automated checks, keyboard focus traps, skip-link at top of layout, gradients validated for WCAG AA contrast. Mobile views verified at 375 px and 768 px breakpoints.

## Traceability Plan
| Requirement | Section | Artifact |
| --- | --- | --- |
| Dashboards & investigations | Spec §18 | React pages, screenshot evidence |
| CLI management | Spec §19 | `odctl` modules, smoke logs |
| RBAC enforcement | Spec §14, §18 | Auth middleware, Jest/Cypress tests |
| Reporting & cache inspection | Spec §23 | API routes + OpenAPI snippets |

## Resolved Decisions
- Component library: custom design tokens with lightweight primitives remains the default; no framework-level grid migration is required.
- CLI auth fallback: OIDC device flow is primary, with API token mode retained for air-gapped/bootstrap scenarios.
