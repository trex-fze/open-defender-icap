# Stage 12 RFC - Canonical Taxonomy Lockdown and Unknown Fallback

**Parent references**: `docs/engine-adaptor-spec.md` sections 9, 13, 14, 27  
**Related docs**: `docs/user-guide.md`, `docs/api-catalog.md`, `docs/architecture.md`, `rfc/stage-09-content-aware-classification.md`  
**Status**: Implemented (with 2026-03 canonical prompt hardening)

## 1) Problem Statement

The current taxonomy UI/API allows operators to create/edit/delete categories and subcategories. For enterprise content filtering, taxonomy must be controlled and predictable:

- classifications should map to an approved canonical taxonomy
- taxonomy drift must be prevented (no ad-hoc category creation in production)
- out-of-model items must be assigned to `Unknown / Unclassified`

Without this control, policy behavior, analytics, and compliance reporting become inconsistent.

## 2) Goals

1. Introduce a fixed canonical taxonomy (41 top-level categories + approved subcategories).
2. Make taxonomy structure read-only in production paths (UI + API).
3. Enforce classification output validation against canonical taxonomy.
4. Route non-matching labels to `Unknown / Unclassified` with explicit reason metadata.
5. Preserve operator control over allow/deny behavior purely via checkbox activation (no hidden overrides).
6. Provide a checkbox-based enable/disable model for categories and subcategories, with explicit Save/Reset workflow.

## 3) Non-Goals

- Building a full taxonomy authoring workflow in this stage.
- Migrating historical records to new category names beyond deterministic mapping.
- Replacing model providers or prompt stacks.

## 4) Proposed Model

### 4.1 Canonical Taxonomy Source of Truth

Add and version a canonical taxonomy artifact in-repo:

- `config/canonical-taxonomy.json`

This file includes:

- `version`
- top-level `categories[]`
- each category with stable `id`, `name`, `subcategories[]` (`id`, `name`)
- required terminal category: `Unknown / Unclassified`

### 4.2 Taxonomy Mutability Policy

- UI (`/taxonomy`) becomes read-only for category/subcategory structure.
- API endpoints that mutate taxonomy structure return `403` (or `405`) unless an explicit maintenance flag is enabled.
- Policy defaults and action maps remain editable through dedicated policy/config endpoints.

### 4.3 Activation / Enforcement Model

Taxonomy structure is fixed, and activation state fully controls enforcement:

- each category has `enabled: boolean`
- each subcategory has `enabled: boolean`
- decision rule:
  1. obtain `(category, subcategory)` from classifier (or fallback to unknown)
  2. if a subcategory has an explicit checkbox state, use it
  3. otherwise inherit the parent category state
  4. `enabled = true` => **Allow**, `enabled = false` => **Deny**
- disabling a category denies all child subcategories (UI will keep child controls in sync)
- `Unknown / Unclassified` is operator-controlled: ON allows unclassified traffic, OFF blocks it

Activation state is persisted separately from canonical taxonomy definitions and is the only governing rule for runtime decisions.

### 4.4 Classification Validation and Fallback

On every classifier result (LLM worker + manual unblock/classification flows):

1. Normalize category/subcategory labels (trim, case-fold, alias map).
2. Validate against canonical taxonomy.
3. If not matched:
   - set `category = "Unknown / Unclassified"`
   - set `subcategory = "Insufficient evidence"` (or canonical unknown bucket)
   - add `taxonomy_fallback_reason` metadata (`unknown_category`, `unknown_subcategory`, `low_confidence`, etc.)

### 4.5 Telemetry and Audit

- Increment metric `taxonomy_fallback_total{reason=...}`.
- Emit audit/event row for fallback transitions (sampled for high-volume paths if needed).
- Add dashboard panel for fallback ratio trend.

## 5) API/UI Contract Changes

### 5.1 Admin API

- `GET /api/v1/taxonomy` returns canonical taxonomy + version + activation profile (category + subcategory enabled flags, effective state, updated metadata).
- `PUT /api/v1/taxonomy/activation` persists checkbox selections (category/subcategory enabled flags) and is the only supported way to change enforcement.
- Mutation endpoints for taxonomy structure (create/update/delete category/subcategory) are disabled by default (unless maintenance flag enabled).

Required `PUT /api/v1/taxonomy/activation` request payload:

```json
{
  "version": "2026-03-25",
  "categories": [
    {
      "id": "adult-sexual-content",
      "enabled": true,
      "subcategories": [
        { "id": "pornography", "enabled": true },
        { "id": "nudity", "enabled": false }
      ]
    }
  ]
}
```

Response payload:

```json
{
  "version": "2026-03-25",
  "updated_at": "2026-03-25T12:00:00Z",
  "updated_by": "admin@local"
}
```

Error payload for blocked structure mutation attempts:

```json
{
  "error": "TAXONOMY_LOCKED",
  "message": "taxonomy structure is managed from canonical-taxonomy.json"
}
```

### 5.2 Web Admin

- Taxonomy page must list every canonical category and subcategory in canonical order.
- Remove/hide create/edit/delete controls for categories/subcategories.
- Render checkbox controls for category/subcategory enable/disable with helper text (ON = allow, OFF = deny).
- Add `Save` button for explicit persistence (`PUT /api/v1/taxonomy/activation`), disabled when pristine, showing loading state while saving.
- Add `Reset` button to reload the last saved activation profile.
- Add explanatory banner: "Structure is centrally managed; only enable/disable settings are editable here."
- Prevent conflicting parent/child states (disabling category disables children in UI, enabling category re-enables children but honors prior explicit overrides if needed).
- Unknown checkbox requires warning text explaining allow/deny impact on unclassified traffic.

## 6) Data and Migration Strategy

1. Seed canonical taxonomy to persistence table(s) (if taxonomy table exists), or load directly from `config/canonical-taxonomy.json` at runtime.
2. Add deterministic alias mapping for existing common labels.
3. For historic entries with invalid taxonomy labels, perform one-time backfill to `Unknown / Unclassified` where mapping is impossible.

## 7) Security and Compliance Impact

- Reduced risk of operator-introduced taxonomy drift.
- Better auditability due to deterministic category universe.
- Improved reporting consistency across time.

## 8) Acceptance Criteria

1. Taxonomy structure in production is read-only.
2. Classifier outputs never persist non-canonical taxonomy labels.
3. Unknown labels are redirected to `Unknown / Unclassified` with reason metadata.
4. Checkbox state is the sole governing rule for allow=enabled / deny=disabled decisions (including unknown fallback).
5. Taxonomy page lists the entire canonical taxonomy with checkbox controls and no structure CRUD actions.
6. Save/Reset workflow persists activation profile and survives page reload / service restart.
7. Tests cover validation, fallback, read-only structure behavior, checkbox enforcement, and activation save flow (including unknown toggle behavior).

## 9) Post-Implementation Hardening (2026-03)

- LLM content-aware prompts now embed canonical taxonomy IDs directly (not just post-hoc mapping).
- Non-canonical model responses are logged and retried before persistence.
- Alias coverage extended for social-media variants (`social networking`, `social network`, `general social networking`) to reduce unnecessary fallback events.
