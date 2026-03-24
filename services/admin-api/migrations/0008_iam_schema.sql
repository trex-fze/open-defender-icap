CREATE TABLE IF NOT EXISTS iam_users (
    id UUID PRIMARY KEY,
    subject TEXT UNIQUE,
    email TEXT UNIQUE NOT NULL,
    display_name TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    last_login_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT iam_users_status_check CHECK (status IN ('active', 'disabled'))
);

CREATE INDEX IF NOT EXISTS iam_users_status_idx ON iam_users (status);

CREATE TABLE IF NOT EXISTS iam_groups (
    id UUID PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT iam_groups_status_check CHECK (status IN ('active', 'disabled'))
);

CREATE TABLE IF NOT EXISTS iam_group_members (
    group_id UUID NOT NULL REFERENCES iam_groups(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES iam_users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (group_id, user_id)
);

CREATE INDEX IF NOT EXISTS iam_group_members_user_idx ON iam_group_members (user_id);

CREATE TABLE IF NOT EXISTS iam_roles (
    id UUID PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    builtin BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS iam_role_permissions (
    role_id UUID NOT NULL REFERENCES iam_roles(id) ON DELETE CASCADE,
    permission TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (role_id, permission)
);

CREATE INDEX IF NOT EXISTS iam_role_permissions_permission_idx
    ON iam_role_permissions (permission);

CREATE TABLE IF NOT EXISTS iam_user_roles (
    user_id UUID NOT NULL REFERENCES iam_users(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES iam_roles(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, role_id)
);

CREATE INDEX IF NOT EXISTS iam_user_roles_role_idx ON iam_user_roles (role_id);

CREATE TABLE IF NOT EXISTS iam_group_roles (
    group_id UUID NOT NULL REFERENCES iam_groups(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES iam_roles(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (group_id, role_id)
);

CREATE INDEX IF NOT EXISTS iam_group_roles_role_idx ON iam_group_roles (role_id);

CREATE TABLE IF NOT EXISTS iam_service_accounts (
    id UUID PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    token_hash TEXT NOT NULL,
    token_hint TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_rotated_at TIMESTAMPTZ,
    CONSTRAINT iam_service_accounts_status_check CHECK (status IN ('active', 'disabled'))
);

CREATE INDEX IF NOT EXISTS iam_service_accounts_hint_idx
    ON iam_service_accounts (token_hint);

CREATE TABLE IF NOT EXISTS iam_service_account_roles (
    service_account_id UUID NOT NULL REFERENCES iam_service_accounts(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES iam_roles(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (service_account_id, role_id)
);

CREATE INDEX IF NOT EXISTS iam_service_account_roles_role_idx
    ON iam_service_account_roles (role_id);

CREATE TABLE IF NOT EXISTS iam_audit_events (
    id UUID PRIMARY KEY,
    actor TEXT,
    action TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    payload JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS iam_audit_events_target_idx
    ON iam_audit_events (target_type, target_id);

CREATE INDEX IF NOT EXISTS iam_audit_events_created_at_idx
    ON iam_audit_events (created_at DESC);

INSERT INTO iam_roles (id, name, description, builtin)
VALUES
    ('00000000-0000-0000-0000-000000000101', 'policy-admin', 'Full administrative access', TRUE),
    ('00000000-0000-0000-0000-000000000102', 'policy-editor', 'Manage policies and taxonomy', TRUE),
    ('00000000-0000-0000-0000-000000000103', 'policy-viewer', 'Read-only access to policy surfaces', TRUE),
    ('00000000-0000-0000-0000-000000000104', 'review-approver', 'Resolve manual review queue items', TRUE),
    ('00000000-0000-0000-0000-000000000105', 'auditor', 'Audit log and reporting access', TRUE)
ON CONFLICT (name) DO UPDATE
SET description = EXCLUDED.description,
    builtin = EXCLUDED.builtin;

WITH role_ids AS (
    SELECT id, name FROM iam_roles
    WHERE name IN (
        'policy-admin',
        'policy-editor',
        'policy-viewer',
        'review-approver',
        'auditor'
    )
), permissions(role_name, permission) AS (
    VALUES
        ('policy-admin', 'overrides:view'),
        ('policy-admin', 'overrides:write'),
        ('policy-admin', 'overrides:delete'),
        ('policy-admin', 'review:view'),
        ('policy-admin', 'review:resolve'),
        ('policy-admin', 'policy:view'),
        ('policy-admin', 'policy:edit'),
        ('policy-admin', 'policy:publish'),
        ('policy-admin', 'taxonomy:edit'),
        ('policy-admin', 'reporting:view'),
        ('policy-admin', 'cache:admin'),
        ('policy-admin', 'audit:view'),
        ('policy-admin', 'iam:manage'),
        ('policy-editor', 'overrides:view'),
        ('policy-editor', 'overrides:write'),
        ('policy-editor', 'policy:view'),
        ('policy-editor', 'policy:edit'),
        ('policy-editor', 'taxonomy:edit'),
        ('policy-viewer', 'overrides:view'),
        ('policy-viewer', 'review:view'),
        ('policy-viewer', 'policy:view'),
        ('policy-viewer', 'reporting:view'),
        ('review-approver', 'review:view'),
        ('review-approver', 'review:resolve'),
        ('auditor', 'reporting:view'),
        ('auditor', 'audit:view')
)
INSERT INTO iam_role_permissions (role_id, permission)
SELECT role_ids.id, permissions.permission
FROM permissions
JOIN role_ids ON role_ids.name = permissions.role_name
ON CONFLICT (role_id, permission) DO NOTHING;
