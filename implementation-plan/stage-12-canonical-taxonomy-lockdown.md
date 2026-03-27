# Stage 12 Implementation Plan - Canonical Taxonomy Lockdown

This plan is intentionally explicit so a coding agent can execute it with minimal ambiguity.

## 0) Scope and Execution Rules

- Do not change policy semantics except taxonomy validation/fallback behavior.
- Do not remove historical data; add deterministic migration/backfill only.
- Prefer additive changes with feature-flagged rollout when practical.

### Scope Confirmation (Remaining Tasks)

Stage 12 delivery items are now satisfied:

1. Fallback/activation monitoring plus rollout + rollback notes captured in `docs/runbooks/stage10-web-admin-operator-runbook.md`.
2. Backend integration coverage added for taxonomy mutation lock, canonical persistence (LLM + policy), and activation profile save/reload in the respective test modules.
3. Canonical classification hardening completed for content-aware jobs: canonical taxonomy IDs are injected into prompts, non-canonical responses are retried, and taxonomy aliases were expanded for social-media variants.

No additional tasks remain in the Stage 12 scope.

## 1) Deliverables

1. Canonical taxonomy artifact (`config/canonical-taxonomy.json`) with all approved categories/subcategories.
2. Backend taxonomy read-only enforcement for structure mutations.
3. Classification normalization + canonical validation + unknown fallback.
4. Backend activation-profile persistence for category/subcategory enable flags.
5. Web taxonomy page that fully lists canonical entries with checkbox enable/disable and Save.
6. Updated docs/runbooks/API catalog.
7. Unit/integration/e2e tests proving enforcement.

## 2) Detailed Work Breakdown

### A. Canonical Taxonomy Artifact

1. Create `config/canonical-taxonomy.json`.
2. Include:
   - `version` string
   - `categories` array
   - stable IDs for category and subcategory entries
   - `Unknown / Unclassified` category with approved fallback subcategories
3. Add schema validation test for this file (JSON structure + uniqueness rules).
4. Add completeness test ensuring all 40 top-level categories are present.

### B. Activation Profile Data Model

Target code areas:

- `services/admin-api/migrations/*`
- `services/admin-api/src/taxonomy.rs`

Implementation steps:

1. Add migration for activation state persistence:
   - `taxonomy_activation_profiles` (`id`, `version`, `updated_by`, `updated_at`)
   - `taxonomy_activation_entries` (`profile_id`, `category_id`, `subcategory_id`, `enabled`)
2. Seed default profile:
   - all categories/subcategories enabled by default (including Unknown)
   - store explicit checkbox state for every category and subcategory
3. Add repository methods:
   - `load_activation_profile()`
   - `save_activation_profile(payload, actor)`
4. Enforce optimistic version check (`version` in request must match canonical version).

### C. Backend Loading + Validation

Target code areas:

- `services/admin-api/src/taxonomy.rs`
- `services/llm-worker/src/*` classification persistence path
- `services/reclass-worker/src/*` reclassification path

Implementation steps:

1. Add a canonical taxonomy loader utility:
   - Parse JSON once at startup.
   - Build lookup maps for case-insensitive matching.
2. Add normalization function:
   - trim/whitespace collapse
   - case-fold
   - alias remap table (hardcoded map or config file)
3. Add validator function returning one of:
   - `Valid(category, subcategory)`
   - `Fallback { reason, normalized_input }`
4. Apply validator before persisting any category/subcategory.
5. On fallback, write canonical unknown values and attach reason metadata.

### D. Taxonomy Mutations Lockdown (Admin API)

Target code areas:

- `services/admin-api/src/taxonomy.rs`
- route wiring in `services/admin-api/src/main.rs`

Implementation steps:

1. Keep read endpoint(s) enabled.
2. For category/subcategory create/update/delete endpoints:
   - return error `TAXONOMY_LOCKED` by default.
3. Optional flag (maintenance only):
   - `OD_TAXONOMY_MUTATION_ENABLED=false` default.
   - if true, allow mutations (for emergency only).
4. Add explicit audit log entries for blocked mutation attempts.

### E. Taxonomy Activation API (Admin API)

Target code areas:

- `services/admin-api/src/taxonomy.rs`
- route wiring in `services/admin-api/src/main.rs`

Implementation steps:

1. Extend `GET /api/v1/taxonomy` response to include activation flags:
   - category enabled
   - subcategory enabled
2. Add `PUT /api/v1/taxonomy/activation`:
   - validate IDs exist in canonical taxonomy
   - apply parent-child rules (disabled parent => effective disabled children)
   - persist in `taxonomy_activation_*` tables
3. Return save metadata (`updated_at`, `updated_by`, `version`).
4. Emit audit event `taxonomy.activation.update`.

### F. Web Admin Read-Only Taxonomy UX with Checkboxes

Target code areas:

- `web-admin/src/pages/TaxonomyPage.tsx`
- any taxonomy hooks under `web-admin/src/hooks/`

Implementation steps:

1. Remove/hide category/subcategory CRUD controls.
2. Render full canonical taxonomy hierarchy (all 40 categories + all subcategories).
3. Add checkbox per category and per subcategory.
4. Add parent-child toggle behavior:
   - toggling category updates effective state of children in UI
   - child toggles allowed only when parent enabled
5. Add Save button:
   - disabled when no unsaved changes
   - sends `PUT /api/v1/taxonomy/activation`
6. Add Reset button to reload last persisted profile.
7. Show lock banner explaining structure vs activation permissions.
8. If API returns lock/validation error, surface clear toast/error with actionable text.

### G. Metrics and Observability

1. Add metric counter: `taxonomy_fallback_total{reason}`.
2. Add logs for fallback decisions with sampled payload context.
3. Add dashboard/runbook note for expected fallback baseline.

4. Add metric `taxonomy_activation_changes_total` for successful profile saves.

### H. Documentation

Update these files:

- `docs/api-catalog.md` (taxonomy endpoint semantics)
- `docs/user-guide.md` (taxonomy is fixed, not operator-authored)
- `docs/architecture.md` (canonical taxonomy source of truth)
- `docs/runbooks/*` (operator guidance for checkbox save workflow)
- rollout notes in `docs/iam-rollout.md` only if auth/taxonomy interactions are relevant

### I. Enforcement Logic Integration

1. Update policy evaluation layer (wherever final allow/deny decision is made) to consume activation profile:
   - compute effective enabled flag for `(category_id, subcategory_id)` using child override > parent fallback rule.
   - `true` => allow, `false` => deny.
2. Ensure unknown fallback follows the same rule (no hidden default allows/denies).
3. When category disabled but subcategory explicitly enabled, honor child state; frontend should prevent conflicting states but backend must handle gracefully.
4. If activation profile missing/corrupt, fail safe (deny) and emit alert, or load last known-good profile.

## 3) Test Strategy

### Unit Tests

1. Canonical taxonomy JSON validity:
   - unique IDs
   - unique names per scope
   - unknown category present
2. Normalization/alias mapping behavior.
3. Validator fallback reasons.

### Integration Tests

1. Admin API taxonomy mutation endpoints blocked by default.
2. Classification persistence always stores canonical labels.
3. Unknown labels map to `Unknown / Unclassified`.
4. Activation profile save succeeds and persists across reloads.
5. Unknown category cannot be disabled.

### Frontend Tests

1. Taxonomy page has no create/edit/delete controls.
2. Full canonical list renders from API.
3. Checkbox interactions update dirty-state and enable Save.
4. Save posts expected payload and shows success state.
5. Read-only structure notice is visible.

## 4) Rollout Plan

1. Deploy backend with validator + mutation lock in shadow mode (log only optional).
2. Validate fallback rates in staging.
3. Enable strict persistence enforcement in production.
4. Deploy frontend read-only taxonomy page.
5. Monitor fallback metrics and blocked mutation events for 1 week.

## 5) Rollback Plan

1. Re-enable mutation endpoints via `OD_TAXONOMY_MUTATION_ENABLED=true` if emergency content operations are blocked.
2. Disable strict fallback enforcement only if critical classification paths break.
3. Keep canonical file unchanged; revert code path if necessary.

## 6) Codex Execution Notes

For a coding agent executing this plan:

1. Implement sections A->H in order.
2. After each section, run the closest test subset before proceeding.
3. Do not skip docs updates; they are part of acceptance.
4. Ensure no UI route depends on mock taxonomy creation paths after completion.
5. `TaxonomyPage` must show all canonical entries and support checkbox Save/Reset workflow.

## 7) Codex 5.1 Precision Contract

Use these concrete rules while implementing:

1. Do not invent new taxonomy names; use only entries from `config/canonical-taxonomy.json`.
2. Persist activation state separately from taxonomy definitions.
3. Keep taxonomy structure immutable unless `OD_TAXONOMY_MUTATION_ENABLED=true`.
4. `GET /api/v1/taxonomy` must return deterministic order matching canonical file.
5. `PUT /api/v1/taxonomy/activation` must be idempotent.
6. If request includes unknown IDs, return `400 VALIDATION_ERROR` with exact failing IDs.
7. If request attempts to disable `Unknown / Unclassified`, return `400 VALIDATION_ERROR`.
8. Frontend Save button behavior:
   - disabled when form is pristine
   - loading state while request in flight
   - success banner on completion
   - error banner with backend message on failure
9. Add tests covering parent toggle + child override semantics.
