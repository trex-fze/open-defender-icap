# Stage 11 RFC - RBAC and User/Group Management

**Parent references**: `docs/engine-adaptor-spec.md` sections 8, 10, 13, 14, 23, 27  
**Related docs**: `docs/api-catalog.md`, `docs/user-guide.md`, `docs/architecture.md`  
**Status**: Proposed

## 1) Problem Statement

Current RBAC is token/JWT-claim based and enforced per endpoint, but the platform lacks first-class identity management:

- no persisted users/groups
- no role bindings via API/CLI/UI
- no group-driven policy administration workflows
- UI settings page still uses mock role matrix data

This blocks enterprise-ready delegated administration, least-privilege operations, and auditable identity governance.

## 2) Goals

1. Add a persistent IAM model for users, groups, roles, and bindings.
2. Support user/group management workflows in Admin API, Web Admin, and odctl.
3. Keep backend as source of truth for authorization decisions.
4. Maintain compatibility with current static token + JWT flows during migration.
5. Provide full auditability for identity and authorization changes.

## 3) Non-Goals

- Full SCIM provisioning in this stage (can follow in a later stage).
- Replacing existing OIDC validation logic entirely in one release.
- Multi-tenant partition redesign.

## 4) Current-State Findings

- Admin API role enforcement is hardcoded role sets in `services/admin-api/src/auth.rs`.
- Policy Engine has separate auth role logic in `services/policy-engine/src/auth.rs`, creating drift risk.
- No IAM persistence tables currently exist in `services/admin-api/migrations`.
- Web admin RBAC settings view uses mock data in `web-admin/src/data/mockData.ts`.
- Policy decision model supports `user_id`/`group_ids`, but ICAP adaptor currently forwards neither (`user_id: None`, no groups).

## 5) Proposed Authorization Model

### 5.1 Principals

- user
- group
- service account

### 5.2 Effective Authorization

Effective roles are computed as:

- direct user roles
- union with group-inherited roles
- optional compatibility fallback to JWT claim roles when principal is not yet provisioned

### 5.3 Role Set

Retain existing built-in role names for compatibility:

- `policy-admin`
- `policy-editor`
- `policy-viewer`
- `review-approver`
- `auditor`

Add internal permission mapping and tie endpoint checks to permissions over time while preserving role constants.

## 6) Data Model (Admin DB)

Add new IAM tables (prefix `iam_`):

- `iam_users`
  - `id uuid pk`, `subject text unique null`, `email text unique`, `display_name text`, `status text`, `last_login_at timestamptz`, `created_at`, `updated_at`
- `iam_groups`
  - `id uuid pk`, `name text unique`, `description text`, `status text`, `created_at`, `updated_at`
- `iam_group_members`
  - `group_id uuid fk`, `user_id uuid fk`, `created_at`, unique `(group_id, user_id)`
- `iam_roles`
  - `id uuid pk`, `name text unique`, `description text`, `builtin bool`, `created_at`
- `iam_role_permissions`
  - `role_id uuid fk`, `permission text`, unique `(role_id, permission)`
- `iam_user_roles`
  - `user_id uuid fk`, `role_id uuid fk`, `created_at`, unique `(user_id, role_id)`
- `iam_group_roles`
  - `group_id uuid fk`, `role_id uuid fk`, `created_at`, unique `(group_id, role_id)`
- `iam_service_accounts`
  - `id uuid pk`, `name text unique`, `token_hash text`, `status text`, `created_at`, `rotated_at`
- `iam_audit_events`
  - `id uuid pk`, `actor text`, `action text`, `target_type text`, `target_id text`, `payload jsonb`, `created_at`

## 7) Admin API Surface

### 7.1 Users

- `GET /api/v1/iam/users`
- `POST /api/v1/iam/users`
- `GET /api/v1/iam/users/:id`
- `PUT /api/v1/iam/users/:id`
- `DELETE /api/v1/iam/users/:id` (soft disable preferred)

### 7.2 Groups

- `GET /api/v1/iam/groups`
- `POST /api/v1/iam/groups`
- `GET /api/v1/iam/groups/:id`
- `PUT /api/v1/iam/groups/:id`
- `DELETE /api/v1/iam/groups/:id`

### 7.3 Memberships

- `GET /api/v1/iam/groups/:id/members`
- `POST /api/v1/iam/groups/:id/members`
- `DELETE /api/v1/iam/groups/:id/members/:user_id`

### 7.4 Roles and Bindings

- `GET /api/v1/iam/roles`
- `POST /api/v1/iam/users/:id/roles`
- `DELETE /api/v1/iam/users/:id/roles/:role`
- `POST /api/v1/iam/groups/:id/roles`
- `DELETE /api/v1/iam/groups/:id/roles/:role`
- `GET /api/v1/iam/effective-roles?user_id=...`

### 7.5 Service Accounts

- `GET /api/v1/iam/service-accounts`
- `POST /api/v1/iam/service-accounts`
- `POST /api/v1/iam/service-accounts/:id/rotate`
- `DELETE /api/v1/iam/service-accounts/:id`

## 8) Web Admin Changes

Replace mock RBAC settings with live IAM pages:

- `/settings/iam/users`
- `/settings/iam/groups`
- `/settings/iam/roles`
- `/settings/iam/service-accounts`
- `/settings/iam/audit`

Required capabilities:

- user/group CRUD
- membership management
- user/group role assignment
- effective-role troubleshooting view
- service-account lifecycle and rotate workflow

## 9) CLI Changes (`odctl`)

Add command tree:

- `odctl iam users list|create|update|disable`
- `odctl iam groups list|create|update|delete`
- `odctl iam groups members add|remove|list`
- `odctl iam roles list|assign-user|revoke-user|assign-group|revoke-group`
- `odctl iam service-accounts list|create|rotate|disable`
- `odctl iam whoami`

## 10) Decision-Path Identity Enrichment

- Parse optional ICAP headers (`X-User`, `X-Group`, `X-Client-IP`) in adaptor.
- Populate policy decision request `user_id` and `group_ids`.
- Validate and sanitize identity header values.

## 11) Security and Compliance

- Hash service-account tokens at rest.
- Prefer soft-delete and immutable audit records for IAM lifecycle events.
- Add strict deny behavior for malformed auth claims/headers.
- Add authz matrix tests for every protected endpoint.

## 12) Backward Compatibility and Migration

- Keep existing role constants and endpoint role checks during migration.
- Introduce compatibility mode:
  - DB-backed role resolution enabled by default
  - optional temporary fallback to JWT claim roles if principal missing
- Keep static token support, but map static token to service-account principal semantics.

## 13) Acceptance Criteria

- Persistent IAM schema exists and migrations apply cleanly.
- IAM API endpoints for users/groups/roles/memberships/service accounts are implemented and protected.
- Web Admin provides full live user/group/role management (no mock matrix in production routes).
- odctl supports IAM lifecycle operations and whoami introspection.
- ICAP adaptor forwards user/group identity context into policy engine requests.
- Security suite verifies 401/403/200 RBAC matrix outcomes for protected APIs.
