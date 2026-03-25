# Stage 11 Checklist - RBAC and User/Group Management

## Discovery and Design

- [x] Completed repository auth/RBAC capability study.
- [x] Identified current gaps (no IAM persistence, mock UI matrix, no group forwarding in adaptor).
- [x] Finalize role-to-permission map and least-privilege policy.
- [x] Finalize compatibility strategy for static token + JWT claim fallback.

## Data Model and Migrations

- [x] Add `iam_users` table.
- [x] Add `iam_groups` table.
- [x] Add `iam_group_members` table.
- [x] Add `iam_roles` and `iam_role_permissions` tables.
- [x] Add `iam_user_roles` and `iam_group_roles` tables.
- [x] Add `iam_service_accounts` table with token hashing.
- [x] Add `iam_audit_events` table.
- [x] Seed built-in roles and permissions.
- [x] Add migration verification tests.

## Admin API

- [x] Implement IAM repositories/services.
- [x] Implement users CRUD endpoints.
- [x] Implement groups CRUD endpoints.
- [x] Implement membership endpoints.
- [x] Implement role assignment endpoints (user/group).
- [x] Implement service-account management and token rotation.
- [x] Implement effective-role introspection endpoint.
- [x] Wire all IAM mutations into audit logging.
- [x] Update API catalog docs.

## AuthN/AuthZ Engine

- [x] Refactor middleware to resolve effective roles from DB.
- [x] Keep static token compatibility via service principal mapping.
- [x] Support OIDC principal mapping by `sub` and email.
- [x] Add compatibility fallback mode flag and docs.
- [x] Add authorization unit tests for role matrix paths.

## Web Admin

- [x] Remove mock RBAC matrix dependency.
- [x] Add `/settings/iam/users` page.
- [x] Add `/settings/iam/groups` page.
- [x] Add `/settings/iam/roles` page.
- [x] Add `/settings/iam/service-accounts` page.
- [x] Add `/settings/iam/audit` page.
- [x] Implement create/edit/delete and assign/unassign flows.
- [x] Align UI role guards with backend enforcement.
- [x] Add Cypress scenarios for IAM workflows.

## CLI (`odctl`)

- [x] Add `odctl iam users ...` commands.
- [x] Add `odctl iam groups ...` commands.
- [x] Add `odctl iam roles ...` commands.
- [x] Add `odctl iam service-accounts ...` commands.
- [x] Add `odctl iam whoami` command.
- [x] Add integration tests and JSON output validation.

## ICAP Identity Enrichment

- [x] Parse `X-User` header into `user_id`.
- [x] Parse `X-Group` header into `group_ids`.
- [x] Validate and sanitize identity header values.
- [x] Add integration test for user/group-based policy conditions.

## Security and Rollout

- [x] Add authz smoke matrix for unauthenticated/unauthorized/authorized paths.
- [x] Add IAM audit integrity checks.
- [x] Add migration and rollback runbook for IAM rollout.
- [x] Run full integration suite in compose mode.
- [x] Document default bootstrap admin creation/password handling (`OD_DEFAULT_ADMIN_PASSWORD`).
- [x] Final acceptance signoff.
