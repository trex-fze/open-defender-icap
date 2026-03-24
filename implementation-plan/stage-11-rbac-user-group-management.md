# Stage 11 Implementation Plan - RBAC and User/Group Management

**Status**: Planned  
**Depends on**: Stage 5 and Stage 10 auth/management foundations  
**Primary targets**: `services/admin-api`, `services/policy-engine`, `services/icap-adaptor`, `web-admin`, `cli/odctl`, docs/tests

Execution checklist: `implementation-plan/stage-11-checklist.md`

## Phase A - Foundation and Schema

- A1: Add IAM schema migrations (`iam_users`, `iam_groups`, membership, roles, bindings, service accounts, IAM audit).
- A2: Seed built-in roles and permission map.
- A3: Add repository/service layer in Admin API for IAM entities.
- A4: Add compatibility flags for principal resolution fallback behavior.

Exit criteria:

- migrations apply cleanly in compose and CI
- role seed is deterministic/idempotent
- compatibility behavior documented

## Phase B - Auth and Authorization Engine Refactor

- B1: Centralize role resolution in Admin API auth middleware.
- B2: Resolve effective roles from direct + group assignments.
- B3: Add service-account auth path with hashed token verification.
- B4: Keep static token mode compatibility mapped to service-account semantics.
- B5: Add effective-role introspection endpoint (`whoami`/effective roles).

Exit criteria:

- all existing protected routes authorize through unified resolver
- no role regression on current endpoints

## Phase C - IAM API Endpoints

- C1: Users CRUD endpoints.
- C2: Groups CRUD endpoints.
- C3: Membership management endpoints.
- C4: Role assignment endpoints for users and groups.
- C5: Service-account lifecycle endpoints (create/list/rotate/disable).
- C6: IAM audit query endpoints.

Exit criteria:

- API catalog updated
- endpoint-level tests cover success + forbidden paths

## Phase D - Web Admin User/Group Management

- D1: Replace mock RBAC matrix with live IAM data.
- D2: Users page (`/settings/iam/users`).
- D3: Groups page (`/settings/iam/groups`).
- D4: Membership management UX.
- D5: Role assignment UX + effective-role view.
- D6: Service-account management page.
- D7: IAM audit page.

Exit criteria:

- no mock role matrix in production settings route
- all IAM CRUD and bindings manageable in browser with role-gated actions

## Phase E - CLI and Operational Workflows

- E1: Add `odctl iam users ...` commands.
- E2: Add `odctl iam groups ...` commands.
- E3: Add `odctl iam roles ...` assignment/revocation commands.
- E4: Add `odctl iam service-accounts ...` commands.
- E5: Add `odctl iam whoami`.
- E6: Add machine-readable output and integration tests for CI automation.

Exit criteria:

- CLI parity with IAM API capabilities
- runbook examples validated in compose

## Phase F - Decision-Path Identity Enrichment

- F1: Parse `X-User` and `X-Group` in ICAP adaptor parser.
- F2: Populate policy decision request `user_id`/`group_ids`.
- F3: Add identity sanitation and fallback behavior.
- F4: Add integration test proving user/group policy conditions can be exercised end-to-end.

Exit criteria:

- identity-aware policy rules are testable from proxy ingress path

## Phase G - Testing, Security, and Rollout

- G1: Unit tests for IAM repositories + effective-role resolver.
- G2: Integration tests for IAM CRUD + bindings.
- G3: Authz matrix tests (401/403/200) for protected resources.
- G4: Cypress tests for IAM UI workflows.
- G5: Rollout plan with compatibility mode and rollback validation.

Exit criteria:

- unit/e2e/integration suites green
- migration and rollback playbooks validated

## Definition of Done

- Persistent IAM model implemented and used for authorization decisions.
- User/group/role/service-account lifecycle available across API/UI/CLI.
- ICAP adaptor forwards identity context into policy decisions.
- Security and RBAC matrix tests pass with documented evidence.
