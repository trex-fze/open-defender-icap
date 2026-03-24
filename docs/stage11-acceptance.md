# Stage 11 Acceptance Notes

* Data model: migrations `0008` and `0009` apply cleanly (see `cargo test -p admin-api --tests`). Builtin roles, permissions, and audit constraints are seeded + enforced.
* Admin API: IAM endpoints (`/api/v1/iam/**`) exercised via `odctl` integration tests and Cypress UI flows. Audit logging verified via the new SQL constraint test.
* Auth: Policy engine now calls the Admin API resolver (`OD_IAM_RESOLVER_URL`). `allow_claim_fallback` flag documented in `docs/iam.md` and defaults to `true` for gradual rollout.
* Web Admin: `/settings/iam/*` replaces the mock RBAC matrix; Cypress `iam.cy.ts` covers users, groups, and service-account generation.
* CLI: `odctl iam ...` command tree ships with WireMock-backed tests validating table and JSON renderings.
* ICAP adaptor/policy: identity headers parsed, sanitized, and threaded through to decision conditions (unit-tested in `policy-engine`).
* Smoke validation: follow `docs/authz-smoke-matrix.md` plus `make compose-test` before/after deployments.
* Default bootstrap admin process is documented (`docs/iam.md`, `docs/iam-rollout.md`) so each cluster has a known break-glass token until real identities are provisioned.

Sign-off recorded on 2026-03-24 after rerunning Rust + React builds in CI.
