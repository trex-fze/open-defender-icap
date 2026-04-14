# Stage 13 Implementation Plan - Domain-First Classification Scope

## Objective

Deduplicate classification workload by canonicalizing subdomain traffic into registrable-domain keys while retaining subdomain-specific override enforcement.

## Completed Work

- [x] Add shared canonical key helper in `crates/common-types/src/normalizer.rs`.
- [x] Update ICAP adaptor enqueue path to use canonical domain keys for pending/page-fetch/classification jobs.
- [x] Keep policy-engine request evaluation on observed keys for fine-grained override matching.
- [x] Auto-promote Admin API manual classify/unblock and pending upsert/clear operations to canonical domain keys.
- [x] Add migration `0015_domain_first_classification_scope.sql` to promote legacy subdomain rows.
- [x] Validate with unit tests + runtime smoke (`REQMOD` on multiple subdomains -> single canonical pending key).

## Verification Checklist

- [x] `cargo test -p common-types`
- [x] `cargo test -p icap-adaptor`
- [x] `cargo test -p admin-api --no-run`
- [x] Docker smoke: `www.smoke-domainfirst.test` and `api.smoke-domainfirst.test` both enqueue as `domain:smoke-domainfirst.test`

## Follow-Up Recommendations

- [x] Add PSL-based registrable domain derivation (replace simple last-two-label reduction). *(Implemented in `crates/common-types/src/normalizer.rs` via PSL-aware parsing with heuristic fallback.)*
- [x] Add metrics for canonicalization collapse ratio (subdomain requests -> domain key). *(Added `classification_canonicalization_total`, `classification_canonicalization_collapsed_total`, and `classification_canonicalization_collapse_ratio` in `services/icap-adaptor/src/metrics.rs`.)*
- [x] Add optional tenant-domain exception list if operational telemetry shows collisions. *(Decision: conditionally deferred; implement only when collision telemetry crosses documented trigger thresholds in runbook.)*
