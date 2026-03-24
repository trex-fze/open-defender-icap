# Stage 11 Checklist - RBAC and User/Group Management

## Discovery and Design

- [x] Completed repository auth/RBAC capability study.
- [x] Identified current gaps (no IAM persistence, mock UI matrix, no group forwarding in adaptor).
- [ ] Finalize role-to-permission map and least-privilege policy.
- [ ] Finalize compatibility strategy for static token + JWT claim fallback.

## Data Model and Migrations

- [ ] Add `iam_users` table.
- [ ] Add `iam_groups` table.
- [ ] Add `iam_group_members` table.
- [ ] Add `iam_roles` and `iam_role_permissions` tables.
- [ ] Add `iam_user_roles` and `iam_group_roles` tables.
- [ ] Add `iam_service_accounts` table with token hashing.
- [ ] Add `iam_audit_events` table.
- [ ] Seed built-in roles and permissions.
- [ ] Add migration verification tests.

## Admin API

- [ ] Implement IAM repositories/services.
- [ ] Implement users CRUD endpoints.
- [ ] Implement groups CRUD endpoints.
- [ ] Implement membership endpoints.
- [ ] Implement role assignment endpoints (user/group).
- [ ] Implement service-account management and token rotation.
- [ ] Implement effective-role introspection endpoint.
- [ ] Wire all IAM mutations into audit logging.
- [ ] Update API catalog docs.

## AuthN/AuthZ Engine

- [ ] Refactor middleware to resolve effective roles from DB.
- [ ] Keep static token compatibility via service principal mapping.
- [ ] Support OIDC principal mapping by `sub` and email.
- [ ] Add compatibility fallback mode flag and docs.
- [ ] Add authorization unit tests for role matrix paths.

## Web Admin

- [ ] Remove mock RBAC matrix dependency.
- [ ] Add `/settings/iam/users` page.
- [ ] Add `/settings/iam/groups` page.
- [ ] Add `/settings/iam/roles` page.
- [ ] Add `/settings/iam/service-accounts` page.
- [ ] Add `/settings/iam/audit` page.
- [ ] Implement create/edit/delete and assign/unassign flows.
- [ ] Align UI role guards with backend enforcement.
- [ ] Add Cypress scenarios for IAM workflows.

## CLI (`odctl`)

- [ ] Add `odctl iam users ...` commands.
- [ ] Add `odctl iam groups ...` commands.
- [ ] Add `odctl iam roles ...` commands.
- [ ] Add `odctl iam service-accounts ...` commands.
- [ ] Add `odctl iam whoami` command.
- [ ] Add integration tests and JSON output validation.

## ICAP Identity Enrichment

- [ ] Parse `X-User` header into `user_id`.
- [ ] Parse `X-Group` header into `group_ids`.
- [ ] Validate and sanitize identity header values.
- [ ] Add integration test for user/group-based policy conditions.

## Security and Rollout

- [ ] Add authz smoke matrix for unauthenticated/unauthorized/authorized paths.
- [ ] Add IAM audit integrity checks.
- [ ] Add migration and rollback runbook for IAM rollout.
- [ ] Run full integration suite in compose mode.
- [ ] Final acceptance signoff.
