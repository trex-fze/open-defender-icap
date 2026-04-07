# Stage 22 Checklist - Cursor Parity for Policy and Reporting APIs

## Inventory
- [x] List all remaining page/offset policy/reporting endpoints.
- [x] Prioritize by data volume and UI/CLI usage.

## Backend Migration
- [ ] Convert selected endpoints to cursor contract.
- [ ] Add/verify keyset indexes via migration.
- [ ] Add backward compatibility/deprecation behavior where required.

## Client Migration
- [ ] Update web-admin hooks/types/components.
- [ ] Update odctl commands and help docs.

## Validation
- [ ] Add cursor-chain tests (forward/empty/invalid cursor).
- [ ] Verify endpoint latency and DB plan quality.

## Completion
- [ ] Mark Stage 22 complete in `implementation-plan/stage-plan.md`.
