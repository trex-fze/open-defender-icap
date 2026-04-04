# Stage 13 RFC - Domain-First Classification Scope

**Parent Sections:** `docs/engine-adaptor-spec.md` §§20.3, 20.3.1  
**Status:** Implemented (2026-04)

## Motivation

Classifying each observed subdomain independently created avoidable queue duplication and delayed verdict convergence. Many destinations (`www`, `api`, tracking hosts) represent the same registrable domain policy posture. Stage 13 introduces domain-first scope so classification work is deduplicated without weakening manual override precision.

## Goals

1. Persist classification/pending/page-content artifacts on canonical domain keys (`domain:<registered_domain>`).
2. Keep policy decision evaluation on the observed request key so subdomain overrides still apply.
3. Auto-promote manual classification/pending operations targeting subdomain keys into canonical domain keys.
4. Preserve explicit Allow / Deny control for both domain and subdomain scopes.
5. Reduce pending backlog churn and repeated crawl/classification jobs for sibling subdomains.

## Non-Goals

- Introducing tenant-domain exception lists in v1.
- Removing subdomain overrides from operator workflows.
- Changing override precedence rules.

## Architecture Changes

1. **Canonical key helper** (`common-types::normalizer::canonical_classification_key`) maps `domain:*` or `subdomain:*` inputs to canonical domain keys.
2. **ICAP adaptor** still calls policy-engine with observed key, but enqueues pending/page-fetch/classification jobs using canonical key.
3. **Admin API** manual classify/unblock and pending upsert/clear operations auto-promote subdomain inputs to canonical domain keys.
4. **Migration 0015** promotes existing `subdomain:*` rows in `classification_requests` and `classifications` into domain keys (latest row wins per domain).

## Enforcement Contract

1. Request-time policy decision uses observed key (preserves host-specific override matching).
2. Classification evidence lifecycle uses canonical domain key (dedupe path).
3. Override precedence remains unchanged: most-specific override wins, then policy/taxonomy flow.

## Acceptance Criteria

1. Two distinct subdomains under the same registrable domain produce one canonical pending key.
2. Manual classify on `subdomain:...` persists to `domain:...` and clears canonical pending row.
3. Subdomain Allow / Deny overrides still take effect for request-time decisions.
4. Migration leaves no legacy `subdomain:*` rows in `classification_requests`/`classifications`.
