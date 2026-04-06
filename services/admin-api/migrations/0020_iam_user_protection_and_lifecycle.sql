ALTER TABLE iam_users
    ADD COLUMN IF NOT EXISTS is_protected BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS iam_users_is_protected_idx
    ON iam_users (is_protected);

UPDATE iam_users
SET is_protected = TRUE
WHERE LOWER(COALESCE(username, '')) = 'admin'
   OR LOWER(COALESCE(email, '')) = 'admin@local';

CREATE OR REPLACE VIEW iam_active_policy_admin_users AS
SELECT DISTINCT u.id
FROM iam_users u
WHERE u.status = 'active'
  AND (
      EXISTS (
          SELECT 1
          FROM iam_user_roles ur
          JOIN iam_roles r ON r.id = ur.role_id
          WHERE ur.user_id = u.id AND r.name = 'policy-admin'
      )
      OR EXISTS (
          SELECT 1
          FROM iam_group_members gm
          JOIN iam_group_roles gr ON gr.group_id = gm.group_id
          JOIN iam_roles r2 ON r2.id = gr.role_id
          WHERE gm.user_id = u.id AND r2.name = 'policy-admin'
      )
  );
