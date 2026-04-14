# Stage 15 Implementation Plan - Cursor Pagination Hard Cutover

## Objective

Ship a load-resilient pagination contract for high-volume admin list endpoints by replacing mixed list responses with cursor/keyset pagination and migrating backend + web-admin + odctl in one release unit.

## Completed Work

- [x] Added shared cursor pagination primitives in `services/admin-api/src/pagination.rs` (`cursor_limit`, cursor encode/decode, `CursorPaged<T>`).
- [x] Converted Admin API list handlers to cursor pagination:
  - `services/admin-api/src/classifications.rs`
  - `services/admin-api/src/classification_requests.rs`
  - `services/admin-api/src/cli_logs.rs`
  - `services/admin-api/src/main.rs` (`/overrides` list)
  - `services/admin-api/src/iam.rs` (users/groups/service-accounts/audit lists)
- [x] Added keyset-supporting indexes in `services/admin-api/migrations/0016_cursor_pagination_indexes.sql`.
- [x] Migrated web-admin data layer + views to cursor traversal and bounded list windows:
  - hooks in `web-admin/src/hooks/*`
  - pages in `web-admin/src/pages/*`
  - shared UI in `web-admin/src/components/PaginationControls.tsx`
  - cursor types in `web-admin/src/types/pagination.ts`
- [x] Migrated odctl list surfaces to cursor contract (`--limit`, `--cursor`) in `cli/odctl/src/main.rs`.
- [x] Updated API reference in `docs/api-catalog.md`.

## Verification Checklist

- [x] `cargo check -p admin-api`
- [x] `cargo check -p odctl`
- [x] `npm test -- --run` (web-admin)
- [x] `npm run build` (web-admin)
- [x] Docker runtime smoke for converted routes confirms `{ data, meta }` envelope and cursor chaining behavior.
- [x] odctl runtime smoke confirms converted list commands operate against live admin-api.

## Follow-Up Recommendations

- [x] Convert remaining offset/page-based policy/reporting list APIs to cursor parity. *(Completed in Stage 22: `implementation-plan/stage-22-cursor-parity-policy-reporting.md`.)*
- [x] Add explicit endpoint-level perf baselines (`EXPLAIN ANALYZE`, p95 latency trend) to ops docs. *(Captured in operator runbook baseline section.)*
- [x] Populate `meta.prev_cursor` semantics when backward cursor navigation is required server-side. *(Implemented for core cursor endpoints with directional cursor envelopes and server-side backward traversal support.)*
