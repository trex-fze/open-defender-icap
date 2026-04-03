# Stage 12 Checklist - Canonical Taxonomy Lockdown

## Discovery and Design

- [x] Confirm canonical taxonomy list and naming conventions with stakeholders.
- [x] Define alias mapping rules (legacy labels -> canonical labels).
- [x] Finalize unknown fallback reasons and metadata schema.

## Canonical Artifact

- [x] Create `config/canonical-taxonomy.json` with stable IDs.
- [x] Include `Unknown / Unclassified` category and approved unknown subcategories.
- [x] Add schema/consistency tests for canonical taxonomy file.
- [x] Verify all 41 top-level categories and full subcategory lists are present.

## Activation Profile Persistence

- [x] Add DB schema for taxonomy activation profile storage.
- [x] Seed default activation profile.
- [x] Add repository methods to load/save activation profile.
- [x] Enforce canonical version checks on save.

## Admin API / Backend Enforcement

- [x] Add canonical taxonomy loader utility.
- [x] Add category/subcategory normalization and validation layer.
- [x] Enforce unknown fallback when labels are non-canonical.
- [x] Prevent taxonomy structure mutations by default (`TAXONOMY_LOCKED`).
- [x] Add optional maintenance flag `OD_TAXONOMY_MUTATION_ENABLED`.
- [x] Emit audit event for blocked mutation attempts.
- [x] Add `PUT /api/v1/taxonomy/activation` endpoint.
- [x] Return activation flags from `GET /api/v1/taxonomy`.
- [x] Ensure `Unknown / Unclassified` toggle controls whether unclassified traffic is allowed (no hidden overrides).

## Classification Pipeline

- [x] Apply validation/fallback in LLM worker persistence path.
- [x] Apply validation/fallback in reclassification worker path.
- [x] Ensure manual unblock/classification paths also validate taxonomy labels.

## Web Admin

- [x] Make taxonomy structure read-only (remove create/edit/delete controls).
- [x] Render full canonical taxonomy list (all categories and subcategories).
- [x] Add category/subcategory enable/disable checkboxes.
- [x] Add parent-child toggle behavior in UI.
- [x] Add Save button with dirty-state and loading state.
- [x] Add Reset button to reload persisted profile.
- [x] Render canonical taxonomy version and read-only banner.
- [x] Handle `TAXONOMY_LOCKED` responses gracefully in UI.

## Observability

- [x] Add `taxonomy_fallback_total{reason}` metric.
- [x] Add logs for fallback events (with safe context).
- [x] Add dashboard/runbook note for fallback monitoring. (docs/runbooks/stage10-web-admin-operator-runbook.md)

## Testing

- [x] Unit tests for normalization/alias/validation/fallback.
- [x] Integration tests for taxonomy mutation lock behavior. (services/admin-api/src/taxonomy.rs::mutation_tests)
- [x] Integration tests for canonical persistence in classification paths. (workers/llm-worker/src/main.rs::classification_persists_canonical_labels_and_flags)
- [x] Integration tests for activation profile save/reload behavior. (services/admin-api/src/taxonomy.rs::tests)
- [x] Frontend tests confirming taxonomy read-only UX.
- [x] Frontend tests for checkbox toggle + save + reset workflow.
- [x] Tests verifying unknown toggle allow/deny behavior.

## Documentation and Rollout

- [x] Update `docs/api-catalog.md` taxonomy endpoint semantics.
- [x] Update `docs/user-guide.md` taxonomy operating model.
- [x] Update `docs/architecture.md` with canonical taxonomy ownership.
- [x] Add rollout + rollback notes for taxonomy lock enforcement. (docs/runbooks/stage10-web-admin-operator-runbook.md)

## Acceptance

- [x] No production path can persist non-canonical taxonomy labels.
- [x] Unknown/unmapped labels always persist as `Unknown / Unclassified`.
- [x] Taxonomy structure CRUD is blocked by default and audited.
- [x] Frontend taxonomy page lists all canonical entries with checkbox controls.
- [x] Operators can enable/disable via checkbox and persist by clicking Save.
- [x] Unknown toggle ON allows unclassified traffic; OFF blocks it.
- [x] Frontend taxonomy page is read-only for structure and explains governance + checkbox contract.
