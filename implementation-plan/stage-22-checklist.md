# Stage 22 Checklist - Cursor Parity for Policy and Reporting APIs

## Inventory
- [x] List all remaining page/offset policy/reporting endpoints.
- [x] Prioritize by data volume and UI/CLI usage.

## Backend Migration
- [x] Convert selected endpoints to cursor contract.
- [x] Add/verify keyset indexes via migration.
- [x] Add backward compatibility/deprecation behavior where required.

## Client Migration
- [x] Update web-admin hooks/types/components.
- [x] Update odctl commands and help docs.

## Validation
- [x] Add cursor-chain tests (forward/empty/invalid cursor).
- [x] Verify endpoint latency and DB plan quality.

## Completion
- [x] Mark Stage 22 complete in `implementation-plan/stage-plan.md`.
