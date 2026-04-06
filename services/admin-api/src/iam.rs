use crate::auth::{require_roles, PrincipalType, UserContext, ROLE_IAM_ADMIN, ROLE_IAM_VIEW};
use crate::{
    pagination::{cursor_limit, decode_cursor, encode_cursor, CursorPaged},
    ApiError, AppState,
};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::Duration as ChronoDuration;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{types::chrono::Utc, PgPool, Row};
use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone)]
pub struct IamService {
    pool: PgPool,
}

impl IamService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn list_users(&self) -> Result<Vec<IamUserRecord>, IamError> {
        let users = sqlx::query_as::<_, IamUserRecord>(
            r#"
            SELECT id, username, subject, email, display_name, status, last_login_at, created_at, updated_at
            FROM iam_users
            ORDER BY created_at DESC
        "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(users)
    }

    pub async fn get_user(&self, id: Uuid) -> Result<IamUserRecord, IamError> {
        let user = sqlx::query_as::<_, IamUserRecord>(
            r#"
            SELECT id, username, subject, email, display_name, status, last_login_at, created_at, updated_at
            FROM iam_users
            WHERE id = $1
        "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        user.ok_or(IamError::NotFound("user".into()))
    }

    pub async fn find_user_by_subject(
        &self,
        subject: &str,
    ) -> Result<Option<IamUserRecord>, IamError> {
        let user = sqlx::query_as::<_, IamUserRecord>(
            r#"
            SELECT id, username, subject, email, display_name, status, last_login_at, created_at, updated_at
            FROM iam_users
            WHERE subject = $1
        "#,
        )
        .bind(subject)
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<IamUserRecord>, IamError> {
        let user = sqlx::query_as::<_, IamUserRecord>(
            r#"
            SELECT id, username, subject, email, display_name, status, last_login_at, created_at, updated_at
            FROM iam_users
            WHERE email = $1
        "#,
        )
        .bind(email.trim())
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    pub async fn create_user(&self, payload: CreateUserRequest) -> Result<IamUserRecord, IamError> {
        if payload.username.trim().is_empty() {
            return Err(IamError::Validation("username required".into()));
        }
        let email = payload
            .email
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let password_hash = if let Some(raw_password) = payload.password.as_deref() {
            let trimmed = raw_password.trim();
            if trimmed.is_empty() {
                None
            } else {
                if trimmed.len() < 8 {
                    return Err(IamError::Validation(
                        "password must be at least 8 characters".into(),
                    ));
                }
                Some(hash_token(trimmed)?)
            }
        } else {
            None
        };
        let must_change_password = payload.must_change_password.unwrap_or(true);
        let record = sqlx::query_as::<_, IamUserRecord>(
            r#"
            INSERT INTO iam_users (id, username, subject, email, display_name, status, password_hash, password_updated_at, must_change_password)
            VALUES ($1, $2, $3, $4, $5, COALESCE($6, 'active'), $7, CASE WHEN $7 IS NOT NULL THEN NOW() ELSE NULL END, $8)
            RETURNING id, username, subject, email, display_name, status, last_login_at, created_at, updated_at
        "#,
        )
        .bind(Uuid::new_v4())
        .bind(payload.username.trim().to_string())
        .bind(payload.subject)
        .bind(email)
        .bind(payload.display_name)
        .bind(payload.status)
        .bind(password_hash)
        .bind(must_change_password)
        .fetch_one(&self.pool)
        .await
        .map_err(map_db_error)?;
        Ok(record)
    }

    pub async fn update_user(
        &self,
        id: Uuid,
        payload: UpdateUserRequest,
    ) -> Result<IamUserRecord, IamError> {
        let record = sqlx::query_as::<_, IamUserRecord>(
            r#"
            UPDATE iam_users
            SET username = COALESCE($2, username),
                subject = COALESCE($3, subject),
                email = COALESCE($4, email),
                display_name = COALESCE($5, display_name),
                status = COALESCE($6, status),
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, username, subject, email, display_name, status, last_login_at, created_at, updated_at
        "#,
        )
        .bind(id)
        .bind(payload.username)
        .bind(payload.subject)
        .bind(payload.email)
        .bind(payload.display_name)
        .bind(payload.status)
        .fetch_optional(&self.pool)
        .await?;
        record.ok_or(IamError::NotFound("user".into()))
    }

    pub async fn disable_user(&self, id: Uuid) -> Result<(), IamError> {
        let result = sqlx::query(
            r#"UPDATE iam_users SET status = 'disabled', updated_at = NOW() WHERE id = $1"#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(IamError::NotFound("user".into()));
        }
        Ok(())
    }

    pub async fn list_groups(&self) -> Result<Vec<IamGroupRecord>, IamError> {
        let groups = sqlx::query_as::<_, IamGroupRecord>(
            r#"
            SELECT id, name, description, status, created_at, updated_at
            FROM iam_groups
            ORDER BY name
        "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(groups)
    }

    pub async fn get_group(&self, id: Uuid) -> Result<IamGroupRecord, IamError> {
        let group = sqlx::query_as::<_, IamGroupRecord>(
            r#"
            SELECT id, name, description, status, created_at, updated_at
            FROM iam_groups
            WHERE id = $1
        "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        group.ok_or(IamError::NotFound("group".into()))
    }

    pub async fn create_group(
        &self,
        payload: CreateGroupRequest,
    ) -> Result<IamGroupRecord, IamError> {
        if payload.name.trim().is_empty() {
            return Err(IamError::Validation("name required".into()));
        }
        let record = sqlx::query_as::<_, IamGroupRecord>(
            r#"
            INSERT INTO iam_groups (id, name, description, status)
            VALUES ($1, $2, $3, COALESCE($4, 'active'))
            RETURNING id, name, description, status, created_at, updated_at
        "#,
        )
        .bind(Uuid::new_v4())
        .bind(payload.name.trim().to_string())
        .bind(payload.description)
        .bind(payload.status)
        .fetch_one(&self.pool)
        .await
        .map_err(map_db_error)?;
        Ok(record)
    }

    pub async fn update_group(
        &self,
        id: Uuid,
        payload: UpdateGroupRequest,
    ) -> Result<IamGroupRecord, IamError> {
        let record = sqlx::query_as::<_, IamGroupRecord>(
            r#"
            UPDATE iam_groups
            SET name = COALESCE($2, name),
                description = COALESCE($3, description),
                status = COALESCE($4, status),
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, name, description, status, created_at, updated_at
        "#,
        )
        .bind(id)
        .bind(payload.name)
        .bind(payload.description)
        .bind(payload.status)
        .fetch_optional(&self.pool)
        .await?;
        record.ok_or(IamError::NotFound("group".into()))
    }

    pub async fn delete_group(&self, id: Uuid) -> Result<(), IamError> {
        let result = sqlx::query("DELETE FROM iam_groups WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(IamError::NotFound("group".into()));
        }
        Ok(())
    }

    pub async fn list_group_members(&self, group_id: Uuid) -> Result<Vec<IamUserRecord>, IamError> {
        let members = sqlx::query_as::<_, IamUserRecord>(
            r#"
            SELECT u.id, u.username, u.subject, u.email, u.display_name, u.status, u.last_login_at, u.created_at, u.updated_at
            FROM iam_group_members gm
            JOIN iam_users u ON u.id = gm.user_id
            WHERE gm.group_id = $1
            ORDER BY u.email
        "#,
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(members)
    }

    pub async fn add_group_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), IamError> {
        sqlx::query(
            r#"INSERT INTO iam_group_members (group_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
        )
        .bind(group_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_group_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), IamError> {
        let result =
            sqlx::query(r#"DELETE FROM iam_group_members WHERE group_id = $1 AND user_id = $2"#)
                .bind(group_id)
                .bind(user_id)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(IamError::NotFound("membership".into()));
        }
        Ok(())
    }

    pub async fn list_roles(&self) -> Result<Vec<IamRoleRecord>, IamError> {
        let rows = sqlx::query(
            r#"
            SELECT r.id,
                   r.name,
                   r.description,
                   r.builtin,
                   r.created_at,
                   COALESCE(array_agg(p.permission) FILTER (WHERE p.permission IS NOT NULL), '{}') AS permissions
            FROM iam_roles r
            LEFT JOIN iam_role_permissions p ON p.role_id = r.id
            GROUP BY r.id
            ORDER BY r.name
        "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let roles = rows
            .into_iter()
            .map(|row| IamRoleRecord {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                builtin: row.get("builtin"),
                created_at: row.get("created_at"),
                permissions: row
                    .get::<Option<Vec<Option<String>>>, _>("permissions")
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .collect(),
            })
            .collect();
        Ok(roles)
    }

    pub async fn assign_role_to_user(
        &self,
        user_id: Uuid,
        role: &str,
    ) -> Result<Vec<String>, IamError> {
        let role_id = self.role_id_by_name(role).await?;
        sqlx::query(
            r#"INSERT INTO iam_user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
        )
        .bind(user_id)
        .bind(role_id)
        .execute(&self.pool)
        .await?;
        self.user_roles(user_id).await
    }

    pub async fn revoke_role_from_user(
        &self,
        user_id: Uuid,
        role: &str,
    ) -> Result<Vec<String>, IamError> {
        let role_id = self.role_id_by_name(role).await?;
        sqlx::query(r#"DELETE FROM iam_user_roles WHERE user_id = $1 AND role_id = $2"#)
            .bind(user_id)
            .bind(role_id)
            .execute(&self.pool)
            .await?;
        self.user_roles(user_id).await
    }

    pub async fn assign_role_to_group(
        &self,
        group_id: Uuid,
        role: &str,
    ) -> Result<Vec<String>, IamError> {
        let role_id = self.role_id_by_name(role).await?;
        sqlx::query(
            r#"INSERT INTO iam_group_roles (group_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
        )
        .bind(group_id)
        .bind(role_id)
        .execute(&self.pool)
        .await?;
        self.group_roles(group_id).await
    }

    pub async fn revoke_role_from_group(
        &self,
        group_id: Uuid,
        role: &str,
    ) -> Result<Vec<String>, IamError> {
        let role_id = self.role_id_by_name(role).await?;
        sqlx::query(r#"DELETE FROM iam_group_roles WHERE group_id = $1 AND role_id = $2"#)
            .bind(group_id)
            .bind(role_id)
            .execute(&self.pool)
            .await?;
        self.group_roles(group_id).await
    }

    pub async fn user_roles(&self, user_id: Uuid) -> Result<Vec<String>, IamError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT r.name
            FROM iam_roles r
            WHERE r.id IN (
                SELECT role_id FROM iam_user_roles WHERE user_id = $1
                UNION
                SELECT gr.role_id
                FROM iam_group_roles gr
                JOIN iam_group_members gm ON gm.group_id = gr.group_id
                WHERE gm.user_id = $1
            )
            ORDER BY r.name
        "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| row.get::<Option<String>, _>("name"))
            .collect())
    }

    pub async fn group_roles(&self, group_id: Uuid) -> Result<Vec<String>, IamError> {
        let rows = sqlx::query(
            r#"
            SELECT r.name
            FROM iam_group_roles gr
            JOIN iam_roles r ON r.id = gr.role_id
            WHERE gr.group_id = $1
            ORDER BY r.name
        "#,
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| row.get::<Option<String>, _>("name"))
            .collect())
    }

    pub async fn list_service_accounts(&self) -> Result<Vec<ServiceAccountRecord>, IamError> {
        let accounts = sqlx::query_as::<_, ServiceAccountRecord>(
            r#"
            SELECT id, name, description, status, token_hint, created_at, last_rotated_at
            FROM iam_service_accounts
            ORDER BY name
        "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(accounts)
    }

    pub async fn get_service_account(&self, id: Uuid) -> Result<ServiceAccountRecord, IamError> {
        let account = sqlx::query_as::<_, ServiceAccountRecord>(
            r#"
            SELECT id, name, description, status, token_hint, created_at, last_rotated_at
            FROM iam_service_accounts
            WHERE id = $1
        "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        account.ok_or(IamError::NotFound("service_account".into()))
    }

    pub async fn create_service_account(
        &self,
        payload: CreateServiceAccountRequest,
    ) -> Result<(ServiceAccountRecord, String), IamError> {
        if payload.name.trim().is_empty() {
            return Err(IamError::Validation("name required".into()));
        }
        let token = generate_token();
        let hash = hash_token(&token)?;
        let hint = token_hint(&token);
        let record = sqlx::query_as::<_, ServiceAccountRecord>(
            r#"
            INSERT INTO iam_service_accounts (id, name, description, token_hash, token_hint, status, last_rotated_at)
            VALUES ($1, $2, $3, $4, $5, COALESCE($6, 'active'), NOW())
            RETURNING id, name, description, status, token_hint, created_at, last_rotated_at
        "#,
        )
        .bind(Uuid::new_v4())
        .bind(payload.name.trim().to_string())
        .bind(payload.description)
        .bind(hash)
        .bind(hint)
        .bind(payload.status)
        .fetch_one(&self.pool)
        .await
        .map_err(map_db_error)?;
        if !payload.roles.is_empty() {
            self.replace_service_account_roles(record.id, &payload.roles)
                .await?;
        }
        Ok((record, token))
    }

    pub async fn rotate_service_account(
        &self,
        id: Uuid,
        payload: Option<UpdateServiceAccountRoles>,
    ) -> Result<(ServiceAccountRecord, String), IamError> {
        let token = generate_token();
        let hash = hash_token(&token)?;
        let hint = token_hint(&token);
        let record = sqlx::query_as::<_, ServiceAccountRecord>(
            r#"
            UPDATE iam_service_accounts
            SET token_hash = $2,
                token_hint = $3,
                last_rotated_at = NOW()
            WHERE id = $1
            RETURNING id, name, description, status, token_hint, created_at, last_rotated_at
        "#,
        )
        .bind(id)
        .bind(hash)
        .bind(hint)
        .fetch_optional(&self.pool)
        .await?;
        let record = record.ok_or(IamError::NotFound("service_account".into()))?;
        if let Some(update) = payload {
            if !update.roles.is_empty() {
                self.replace_service_account_roles(record.id, &update.roles)
                    .await?;
            }
        }
        Ok((record, token))
    }

    pub async fn disable_service_account(&self, id: Uuid) -> Result<(), IamError> {
        let result =
            sqlx::query(r#"UPDATE iam_service_accounts SET status = 'disabled' WHERE id = $1"#)
                .bind(id)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(IamError::NotFound("service_account".into()));
        }
        Ok(())
    }

    pub async fn service_account_roles(&self, id: Uuid) -> Result<Vec<String>, IamError> {
        let rows = sqlx::query(
            r#"
            SELECT r.name
            FROM iam_service_account_roles sr
            JOIN iam_roles r ON r.id = sr.role_id
            WHERE sr.service_account_id = $1
            ORDER BY r.name
        "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| row.get::<Option<String>, _>("name"))
            .collect())
    }

    pub async fn effective_permissions_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<EffectiveAccess, IamError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT r.id, r.name
            FROM iam_roles r
            WHERE r.id IN (
                SELECT role_id FROM iam_user_roles WHERE user_id = $1
                UNION
                SELECT gr.role_id
                FROM iam_group_roles gr
                JOIN iam_group_members gm ON gm.group_id = gr.group_id
                WHERE gm.user_id = $1
            )
        "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let role_ids: Vec<Uuid> = rows
            .iter()
            .filter_map(|row| row.get::<Option<Uuid>, _>("id"))
            .collect();
        let role_names: HashSet<String> = rows
            .into_iter()
            .filter_map(|row| row.get::<Option<String>, _>("name"))
            .collect();
        let permissions = if role_ids.is_empty() {
            HashSet::new()
        } else {
            let rows = sqlx::query(
                r#"SELECT DISTINCT permission FROM iam_role_permissions WHERE role_id = ANY($1)"#,
            )
            .bind(&role_ids)
            .fetch_all(&self.pool)
            .await?;
            rows.into_iter()
                .filter_map(|row| row.get::<Option<String>, _>("permission"))
                .collect()
        };

        Ok(EffectiveAccess {
            roles: role_names,
            permissions,
        })
    }

    pub async fn effective_permissions_for_service_account(
        &self,
        service_account_id: Uuid,
    ) -> Result<EffectiveAccess, IamError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT r.id, r.name
            FROM iam_service_account_roles sr
            JOIN iam_roles r ON r.id = sr.role_id
            WHERE sr.service_account_id = $1
        "#,
        )
        .bind(service_account_id)
        .fetch_all(&self.pool)
        .await?;

        let role_ids: Vec<Uuid> = rows
            .iter()
            .filter_map(|row| row.get::<Option<Uuid>, _>("id"))
            .collect();
        let role_names: HashSet<String> = rows
            .into_iter()
            .filter_map(|row| row.get::<Option<String>, _>("name"))
            .collect();
        let permissions = if role_ids.is_empty() {
            HashSet::new()
        } else {
            let rows = sqlx::query(
                r#"SELECT DISTINCT permission FROM iam_role_permissions WHERE role_id = ANY($1)"#,
            )
            .bind(&role_ids)
            .fetch_all(&self.pool)
            .await?;
            rows.into_iter()
                .filter_map(|row| row.get::<Option<String>, _>("permission"))
                .collect()
        };

        Ok(EffectiveAccess {
            roles: role_names,
            permissions,
        })
    }

    pub async fn verify_service_token(
        &self,
        token: &str,
    ) -> Result<Option<ServiceAccountPrincipal>, IamError> {
        let hint = token_hint(token);
        let candidates = sqlx::query_as::<_, ServiceAccountSecret>(
            r#"
            SELECT id, name, token_hash
            FROM iam_service_accounts
            WHERE status = 'active' AND token_hint = $1
        "#,
        )
        .bind(hint)
        .fetch_all(&self.pool)
        .await?;

        for candidate in candidates {
            if verify_hash(token, &candidate.token_hash)? {
                let access = self
                    .effective_permissions_for_service_account(candidate.id)
                    .await?;
                return Ok(Some(ServiceAccountPrincipal {
                    id: candidate.id,
                    name: candidate.name,
                    roles: access.roles,
                    permissions: access.permissions,
                }));
            }
        }
        Ok(None)
    }

    pub async fn record_iam_event(
        &self,
        actor: Option<String>,
        action: &str,
        target_type: &str,
        target_id: Option<String>,
        payload: Value,
    ) {
        let _ = sqlx::query(
            r#"
            INSERT INTO iam_audit_events (id, actor, action, target_type, target_id, payload)
            VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        )
        .bind(Uuid::new_v4())
        .bind(actor)
        .bind(action)
        .bind(Some(target_type.to_string()))
        .bind(target_id)
        .bind(payload)
        .execute(&self.pool)
        .await;
    }

    pub async fn list_iam_audit(&self, limit: i64) -> Result<Vec<IamAuditRecord>, IamError> {
        let events = sqlx::query_as::<_, IamAuditRecord>(
            r#"
            SELECT id, actor, action, target_type, target_id, payload, created_at
            FROM iam_audit_events
            ORDER BY created_at DESC
            LIMIT $1
        "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(events)
    }

    async fn replace_service_account_roles(
        &self,
        id: Uuid,
        roles: &[String],
    ) -> Result<(), IamError> {
        let role_ids = self.role_ids_by_name(roles).await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(r#"DELETE FROM iam_service_account_roles WHERE service_account_id = $1"#)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for role_id in role_ids {
            sqlx::query(
                r#"INSERT INTO iam_service_account_roles (service_account_id, role_id) VALUES ($1, $2)"#,
            )
            .bind(id)
            .bind(role_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn role_id_by_name(&self, role: &str) -> Result<Uuid, IamError> {
        let row = sqlx::query("SELECT id FROM iam_roles WHERE name = $1")
            .bind(role)
            .fetch_optional(&self.pool)
            .await?;
        row.and_then(|r| r.get::<Option<Uuid>, _>("id"))
            .ok_or_else(|| IamError::Validation(format!("unknown role: {}", role)))
    }

    async fn role_ids_by_name(&self, roles: &[String]) -> Result<Vec<Uuid>, IamError> {
        if roles.is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query("SELECT id, name FROM iam_roles WHERE name = ANY($1)")
            .bind(roles)
            .fetch_all(&self.pool)
            .await?;
        let mut ids = Vec::with_capacity(rows.len());
        let mut found = HashSet::new();
        for row in &rows {
            if let (Some(id), Some(name)) = (
                row.get::<Option<Uuid>, _>("id"),
                row.get::<Option<String>, _>("name"),
            ) {
                ids.push(id);
                found.insert(name);
            }
        }
        let missing: Vec<String> = roles
            .iter()
            .filter(|r| !found.contains(r.as_str()))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(IamError::Validation(format!(
                "unknown role(s): {}",
                missing.join(", ")
            )));
        }
        Ok(ids)
    }

    pub async fn bootstrap_local_admin(&self, default_password: &str) -> Result<(), IamError> {
        if default_password.trim().is_empty() {
            return Err(IamError::Validation(
                "OD_DEFAULT_ADMIN_PASSWORD cannot be empty".into(),
            ));
        }

        let row = sqlx::query(
            r#"
            SELECT u.id
            FROM iam_users u
            JOIN iam_user_roles ur ON ur.user_id = u.id
            JOIN iam_roles r ON r.id = ur.role_id
            WHERE r.name = 'policy-admin' AND u.status = 'active'
            LIMIT 1
        "#,
        )
        .fetch_optional(&self.pool)
        .await?;
        if row.is_some() {
            return Ok(());
        }

        let password_hash = hash_token(default_password)?;
        let user_id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        let admin_id = sqlx::query(
            r#"
            INSERT INTO iam_users (id, username, email, display_name, status, password_hash, password_updated_at, must_change_password)
            VALUES ($1, 'admin', 'admin@local', 'Default Admin', 'active', $2, NOW(), TRUE)
            ON CONFLICT (email) DO UPDATE
                SET username = COALESCE(iam_users.username, EXCLUDED.username),
                    password_hash = COALESCE(iam_users.password_hash, EXCLUDED.password_hash),
                    password_updated_at = COALESCE(iam_users.password_updated_at, EXCLUDED.password_updated_at),
                    must_change_password = iam_users.must_change_password
            RETURNING id
        "#,
        )
        .bind(user_id)
        .bind(password_hash)
        .fetch_one(&mut *tx)
        .await?
        .get::<Uuid, _>("id");

        let role_id = sqlx::query("SELECT id FROM iam_roles WHERE name = 'policy-admin'")
            .fetch_one(&mut *tx)
            .await?
            .get::<Uuid, _>("id");
        sqlx::query(
            r#"INSERT INTO iam_user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
        )
        .bind(admin_id)
        .bind(role_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(())
    }

    pub async fn authenticate_local_user(
        &self,
        username_or_email: &str,
        password: &str,
        max_failed_attempts: i32,
        lockout_seconds: i64,
    ) -> Result<LocalAuthenticatedUser, IamError> {
        let row = sqlx::query(
            r#"
            SELECT id, username, email, display_name, status, password_hash, failed_login_attempts, locked_until, must_change_password
            FROM iam_users
            WHERE LOWER(email) = LOWER($1) OR LOWER(COALESCE(username, '')) = LOWER($1)
            LIMIT 1
        "#,
        )
        .bind(username_or_email.trim())
        .fetch_optional(&self.pool)
        .await?;

        let row = row.ok_or(IamError::InvalidCredentials)?;
        let user_id = row.get::<Uuid, _>("id");
        let status = row.get::<String, _>("status");
        if status != "active" {
            return Err(IamError::Disabled);
        }

        let locked_until = row.get::<Option<sqlx::types::chrono::DateTime<Utc>>, _>("locked_until");
        if let Some(locked_until) = locked_until {
            if locked_until > Utc::now() {
                return Err(IamError::Locked(locked_until));
            }
        }

        let hash = row
            .get::<Option<String>, _>("password_hash")
            .ok_or(IamError::InvalidCredentials)?;
        let valid = verify_hash(password, &hash)?;
        if !valid {
            let failed = row.get::<i32, _>("failed_login_attempts") + 1;
            let lock_until = if failed >= max_failed_attempts {
                Some(Utc::now() + ChronoDuration::seconds(lockout_seconds.max(1)))
            } else {
                None
            };
            sqlx::query(
                r#"
                UPDATE iam_users
                SET failed_login_attempts = $2,
                    locked_until = $3,
                    updated_at = NOW()
                WHERE id = $1
            "#,
            )
            .bind(user_id)
            .bind(failed)
            .bind(lock_until)
            .execute(&self.pool)
            .await?;
            return Err(IamError::InvalidCredentials);
        }

        sqlx::query(
            r#"
            UPDATE iam_users
            SET failed_login_attempts = 0,
                locked_until = NULL,
                last_login_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
        "#,
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        let access = self.effective_permissions_for_user(user_id).await?;
        Ok(LocalAuthenticatedUser {
            id: user_id,
            username: row.get::<Option<String>, _>("username"),
            email: row
                .get::<Option<String>, _>("email")
                .unwrap_or_else(|| row.get::<Option<String>, _>("username").unwrap_or_default()),
            display_name: row.get::<Option<String>, _>("display_name"),
            roles: access.roles,
            permissions: access.permissions,
            must_change_password: row.get::<bool, _>("must_change_password"),
        })
    }

    pub async fn set_user_password(
        &self,
        user_id: Uuid,
        password: &str,
        must_change_password: bool,
    ) -> Result<(), IamError> {
        if password.trim().is_empty() {
            return Err(IamError::Validation("password is required".into()));
        }
        if password.trim().len() < 8 {
            return Err(IamError::Validation(
                "password must be at least 8 characters".into(),
            ));
        }
        let hash = hash_token(password.trim())?;
        let updated = sqlx::query(
            r#"
            UPDATE iam_users
            SET password_hash = $2,
                password_updated_at = NOW(),
                must_change_password = $3,
                failed_login_attempts = 0,
                locked_until = NULL,
                updated_at = NOW()
            WHERE id = $1
        "#,
        )
        .bind(user_id)
        .bind(hash)
        .bind(must_change_password)
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(IamError::NotFound("user".into()));
        }
        Ok(())
    }

    pub async fn change_password(
        &self,
        user_id: Uuid,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), IamError> {
        let row = sqlx::query("SELECT password_hash FROM iam_users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Err(IamError::NotFound("user".into()));
        };
        let existing = row
            .get::<Option<String>, _>("password_hash")
            .ok_or(IamError::InvalidCredentials)?;
        if !verify_hash(current_password, &existing)? {
            return Err(IamError::InvalidCredentials);
        }
        self.set_user_password(user_id, new_password, false).await
    }

    pub async fn create_user_token(
        &self,
        user_id: Uuid,
        payload: CreateUserTokenRequest,
    ) -> Result<UserTokenWithSecret, IamError> {
        if payload.name.trim().is_empty() {
            return Err(IamError::Validation("token name is required".into()));
        }

        let token = generate_token();
        let hash = hash_token(&token)?;
        let hint = token_hint(&token);
        let record = sqlx::query_as::<_, UserTokenRecord>(
            r#"
            INSERT INTO iam_user_tokens (id, user_id, name, token_hash, token_hint, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, user_id, name, token_hint, status, created_at, last_used_at, expires_at
        "#,
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(payload.name.trim().to_string())
        .bind(hash)
        .bind(hint)
        .bind(payload.expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(map_db_error)?;

        Ok(UserTokenWithSecret {
            token,
            token_record: record,
        })
    }

    pub async fn list_user_tokens(&self, user_id: Uuid) -> Result<Vec<UserTokenRecord>, IamError> {
        let rows = sqlx::query_as::<_, UserTokenRecord>(
            r#"
            SELECT id, user_id, name, token_hint, status, created_at, last_used_at, expires_at
            FROM iam_user_tokens
            WHERE user_id = $1
            ORDER BY created_at DESC
        "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn revoke_user_token(&self, user_id: Uuid, token_id: Uuid) -> Result<(), IamError> {
        let result = sqlx::query(
            "UPDATE iam_user_tokens SET status = 'disabled' WHERE id = $1 AND user_id = $2",
        )
        .bind(token_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(IamError::NotFound("user_token".into()));
        }
        Ok(())
    }

    pub async fn verify_user_token(
        &self,
        token: &str,
    ) -> Result<Option<LocalAuthenticatedUser>, IamError> {
        let hint = token_hint(token);
        let candidates = sqlx::query(
            r#"
            SELECT t.id, t.user_id, t.token_hash,
                   u.username, u.email, u.display_name, u.status,
                   t.expires_at
            FROM iam_user_tokens t
            JOIN iam_users u ON u.id = t.user_id
            WHERE t.status = 'active' AND t.token_hint = $1
        "#,
        )
        .bind(hint)
        .fetch_all(&self.pool)
        .await?;

        for row in candidates {
            let expires_at = row.get::<Option<sqlx::types::chrono::DateTime<Utc>>, _>("expires_at");
            if let Some(value) = expires_at {
                if value <= Utc::now() {
                    continue;
                }
            }
            let hash = row.get::<String, _>("token_hash");
            if !verify_hash(token, &hash)? {
                continue;
            }
            let status = row.get::<String, _>("status");
            if status != "active" {
                continue;
            }
            let user_id = row.get::<Uuid, _>("user_id");
            let access = self.effective_permissions_for_user(user_id).await?;
            let _ = sqlx::query("UPDATE iam_user_tokens SET last_used_at = NOW() WHERE id = $1")
                .bind(row.get::<Uuid, _>("id"))
                .execute(&self.pool)
                .await;

            let username = row.get::<Option<String>, _>("username");
            let email = row
                .get::<Option<String>, _>("email")
                .unwrap_or_else(|| username.clone().unwrap_or_default());

            return Ok(Some(LocalAuthenticatedUser {
                id: user_id,
                username,
                email,
                display_name: row.get::<Option<String>, _>("display_name"),
                roles: access.roles,
                permissions: access.permissions,
                must_change_password: false,
            }));
        }
        Ok(None)
    }
}

#[derive(Error, Debug)]
pub enum IamError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("account disabled")]
    Disabled,
    #[error("account locked until {0}")]
    Locked(sqlx::types::chrono::DateTime<Utc>),
}

impl From<argon2::password_hash::Error> for IamError {
    fn from(err: argon2::password_hash::Error) -> Self {
        IamError::Crypto(err.to_string())
    }
}

fn map_db_error(err: sqlx::Error) -> IamError {
    if let sqlx::Error::Database(db_err) = &err {
        if db_err.constraint().is_some() {
            return IamError::Validation(db_err.to_string());
        }
    }
    IamError::Db(err)
}

pub(crate) fn map_iam_error(err: IamError) -> (StatusCode, Json<ApiError>) {
    match err {
        IamError::NotFound(resource) => (
            StatusCode::NOT_FOUND,
            Json(ApiError::new(
                "NOT_FOUND",
                format!("{} not found", resource),
            )),
        ),
        IamError::Validation(msg) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new("VALIDATION_ERROR", msg)),
        ),
        IamError::Db(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        ),
        IamError::Crypto(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("CRYPTO_ERROR", err)),
        ),
        IamError::InvalidCredentials => (
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("INVALID_CREDENTIALS", "invalid credentials")),
        ),
        IamError::Disabled => (
            StatusCode::FORBIDDEN,
            Json(ApiError::new("ACCOUNT_DISABLED", "account disabled")),
        ),
        IamError::Locked(until) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiError::new(
                "ACCOUNT_LOCKED",
                format!("account locked until {}", until.to_rfc3339()),
            )),
        ),
    }
}

#[derive(Clone, Debug)]
pub struct LocalAuthenticatedUser {
    pub id: Uuid,
    pub username: Option<String>,
    pub email: String,
    pub display_name: Option<String>,
    pub roles: HashSet<String>,
    pub permissions: HashSet<String>,
    pub must_change_password: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct IamUserRecord {
    pub id: Uuid,
    pub username: Option<String>,
    pub subject: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub status: String,
    pub last_login_at: Option<sqlx::types::chrono::DateTime<Utc>>,
    pub created_at: sqlx::types::chrono::DateTime<Utc>,
    pub updated_at: sqlx::types::chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct IamGroupRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub created_at: sqlx::types::chrono::DateTime<Utc>,
    pub updated_at: sqlx::types::chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IamRoleRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub builtin: bool,
    pub created_at: sqlx::types::chrono::DateTime<Utc>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct ServiceAccountRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub token_hint: Option<String>,
    pub created_at: sqlx::types::chrono::DateTime<Utc>,
    pub last_rotated_at: Option<sqlx::types::chrono::DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct ServiceAccountSecret {
    pub id: Uuid,
    pub name: String,
    pub token_hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct IamAuditRecord {
    pub id: Uuid,
    pub actor: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub payload: Option<Value>,
    pub created_at: sqlx::types::chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub subject: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub status: Option<String>,
    pub password: Option<String>,
    pub must_change_password: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub username: Option<String>,
    pub subject: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetUserPasswordRequest {
    pub password: String,
    pub must_change_password: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserTokenRequest {
    pub name: String,
    pub expires_at: Option<sqlx::types::chrono::DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct UserTokenRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub token_hint: Option<String>,
    pub status: String,
    pub created_at: sqlx::types::chrono::DateTime<Utc>,
    pub last_used_at: Option<sqlx::types::chrono::DateTime<Utc>>,
    pub expires_at: Option<sqlx::types::chrono::DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct UserTokenWithSecret {
    pub token: String,
    pub token_record: UserTokenRecord,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssignRoleRequest {
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateServiceAccountRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateServiceAccountRoles {
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ServiceAccountWithToken {
    pub account: ServiceAccountRecord,
    pub token: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct UserDetails {
    pub user: IamUserRecord,
    pub roles: Vec<String>,
    pub groups: Vec<IamGroupRecord>,
}

#[derive(Debug, Serialize)]
pub struct GroupDetails {
    pub group: IamGroupRecord,
    pub members: Vec<IamUserRecord>,
    pub roles: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct UserCursor {
    created_at: sqlx::types::chrono::DateTime<Utc>,
    id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
struct GroupCursor {
    name: String,
    id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServiceAccountCursor {
    name: String,
    id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
struct AuditCursor {
    created_at: sqlx::types::chrono::DateTime<Utc>,
    id: Uuid,
}

#[derive(Clone, Debug)]
pub struct EffectiveAccess {
    pub roles: HashSet<String>,
    pub permissions: HashSet<String>,
}

#[derive(Clone, Debug)]
pub struct ServiceAccountPrincipal {
    pub id: Uuid,
    pub name: String,
    pub roles: HashSet<String>,
    pub permissions: HashSet<String>,
}

fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(48)
        .collect()
}

fn hash_token(token: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();
    let hash = argon.hash_password(token.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

fn verify_hash(token: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(token.as_bytes(), &parsed)
        .is_ok())
}

fn token_hint(token: &str) -> String {
    let trimmed = token.trim();
    let len = trimmed.len();
    if len <= 8 {
        trimmed.to_string()
    } else {
        trimmed[len - 8..].to_string()
    }
}

#[derive(Debug, Serialize)]
pub struct WhoAmIResponse {
    pub actor: String,
    pub principal_type: PrincipalType,
    pub principal_id: Option<Uuid>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

pub async fn list_users_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<CursorPaged<UserDetails>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;

    let limit = cursor_limit(query.limit);
    let cursor = query
        .cursor
        .as_deref()
        .map(decode_cursor::<UserCursor>)
        .transpose()
        .map_err(|message| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::new("INVALID_CURSOR", message)),
            )
        })?;

    let cursor_created_at = cursor.as_ref().map(|c| c.created_at);
    let cursor_id = cursor.as_ref().map(|c| c.id).unwrap_or_else(Uuid::nil);

    let iam = state.iam();
    let users = sqlx::query_as::<_, IamUserRecord>(
        r#"
        SELECT id, username, subject, email, display_name, status, last_login_at, created_at, updated_at
        FROM iam_users
        WHERE ($1::timestamptz IS NULL OR (created_at, id) < ($1, $2))
        ORDER BY created_at DESC, id DESC
        LIMIT $3
    "#,
    )
    .bind(cursor_created_at)
    .bind(cursor_id)
    .bind((limit + 1) as i64)
    .fetch_all(iam.pool())
    .await
    .map_err(|err| map_iam_error(map_db_error(err)))?;

    let has_more = users.len() > limit as usize;
    let mut users = users;
    if has_more {
        users.truncate(limit as usize);
    }

    let mut results = Vec::with_capacity(users.len());
    for record in users {
        let roles = iam.user_roles(record.id).await.map_err(map_iam_error)?;
        let groups = sqlx::query_as::<_, IamGroupRecord>(
            r#"
            SELECT g.id, g.name, g.description, g.status, g.created_at, g.updated_at
            FROM iam_group_members gm
            JOIN iam_groups g ON g.id = gm.group_id
            WHERE gm.user_id = $1
            ORDER BY g.name
        "#,
        )
        .bind(record.id)
        .fetch_all(iam.pool())
        .await
        .map_err(|err| map_iam_error(map_db_error(err)))?;
        results.push(UserDetails {
            user: record,
            roles,
            groups,
        });
    }

    let next_cursor = if has_more {
        results.last().and_then(|last| {
            encode_cursor(&UserCursor {
                created_at: last.user.created_at,
                id: last.user.id,
            })
            .ok()
        })
    } else {
        None
    };

    Ok(Json(CursorPaged::new(
        results,
        limit,
        has_more,
        next_cursor,
    )))
}

pub async fn create_user_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<UserDetails>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let record = iam.create_user(payload).await.map_err(map_iam_error)?;
    let details = UserDetails {
        user: record.clone(),
        roles: vec![],
        groups: vec![],
    };
    state
        .log_iam_event(
            "iam.user.create",
            Some(user.actor.clone()),
            "user",
            Some(record.id.to_string()),
            json!({
                "username": record.username,
                "email": record.email,
                "status": record.status,
            }),
        )
        .await;
    Ok(Json(details))
}

pub async fn get_user_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserDetails>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let record = iam.get_user(id).await.map_err(map_iam_error)?;
    let roles = iam.user_roles(id).await.map_err(map_iam_error)?;
    let groups = sqlx::query_as::<_, IamGroupRecord>(
        r#"
        SELECT g.id, g.name, g.description, g.status, g.created_at, g.updated_at
        FROM iam_group_members gm
        JOIN iam_groups g ON g.id = gm.group_id
        WHERE gm.user_id = $1
        ORDER BY g.name
    "#,
    )
    .bind(id)
    .fetch_all(iam.pool())
    .await
    .map_err(|err| map_iam_error(map_db_error(err)))?;
    Ok(Json(UserDetails {
        user: record,
        roles,
        groups,
    }))
}

pub async fn update_user_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<Json<UserDetails>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let record = iam.update_user(id, payload).await.map_err(map_iam_error)?;
    let roles = iam.user_roles(id).await.map_err(map_iam_error)?;
    let groups = sqlx::query_as::<_, IamGroupRecord>(
        r#"
        SELECT g.id, g.name, g.description, g.status, g.created_at, g.updated_at
        FROM iam_group_members gm
        JOIN iam_groups g ON g.id = gm.group_id
        WHERE gm.user_id = $1
        ORDER BY g.name
    "#,
    )
    .bind(id)
    .fetch_all(iam.pool())
    .await
    .map_err(|err| map_iam_error(map_db_error(err)))?;
    state
        .log_iam_event(
            "iam.user.update",
            Some(user.actor.clone()),
            "user",
            Some(id.to_string()),
            json!({
                "status": record.status,
            }),
        )
        .await;
    Ok(Json(UserDetails {
        user: record,
        roles,
        groups,
    }))
}

pub async fn delete_user_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    iam.disable_user(id).await.map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.user.disable",
            Some(user.actor.clone()),
            "user",
            Some(id.to_string()),
            json!({}),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_user_password_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<SetUserPasswordRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    state
        .iam()
        .set_user_password(
            id,
            payload.password.as_str(),
            payload.must_change_password.unwrap_or(true),
        )
        .await
        .map_err(map_iam_error)?;

    state
        .log_iam_event(
            "iam.user.password.set",
            Some(user.actor.clone()),
            "user",
            Some(id.to_string()),
            json!({
                "must_change_password": payload.must_change_password.unwrap_or(true),
            }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_user_token_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CreateUserTokenRequest>,
) -> Result<Json<UserTokenWithSecret>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let created = state
        .iam()
        .create_user_token(id, payload)
        .await
        .map_err(map_iam_error)?;

    state
        .log_iam_event(
            "iam.user.token.create",
            Some(user.actor.clone()),
            "user",
            Some(id.to_string()),
            json!({
                "token_name": created.token_record.name,
                "token_id": created.token_record.id,
            }),
        )
        .await;

    Ok(Json(created))
}

pub async fn list_user_tokens_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<UserTokenRecord>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let tokens = state
        .iam()
        .list_user_tokens(id)
        .await
        .map_err(map_iam_error)?;
    Ok(Json(tokens))
}

pub async fn revoke_user_token_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path((id, token_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    state
        .iam()
        .revoke_user_token(id, token_id)
        .await
        .map_err(map_iam_error)?;

    state
        .log_iam_event(
            "iam.user.token.revoke",
            Some(user.actor.clone()),
            "user",
            Some(id.to_string()),
            json!({ "token_id": token_id }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_groups_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<CursorPaged<GroupDetails>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let limit = cursor_limit(query.limit);
    let cursor = query
        .cursor
        .as_deref()
        .map(decode_cursor::<GroupCursor>)
        .transpose()
        .map_err(|message| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::new("INVALID_CURSOR", message)),
            )
        })?;
    let cursor_name = cursor.as_ref().map(|c| c.name.clone()).unwrap_or_default();
    let cursor_id = cursor.as_ref().map(|c| c.id).unwrap_or_else(Uuid::nil);

    let iam = state.iam();
    let groups = sqlx::query_as::<_, IamGroupRecord>(
        r#"
        SELECT id, name, description, status, created_at, updated_at
        FROM iam_groups
        WHERE ($1::text = '' OR (name, id) > ($1, $2))
        ORDER BY name ASC, id ASC
        LIMIT $3
    "#,
    )
    .bind(&cursor_name)
    .bind(cursor_id)
    .bind((limit + 1) as i64)
    .fetch_all(iam.pool())
    .await
    .map_err(|err| map_iam_error(map_db_error(err)))?;
    let has_more = groups.len() > limit as usize;
    let mut groups = groups;
    if has_more {
        groups.truncate(limit as usize);
    }
    let mut results = Vec::with_capacity(groups.len());
    for group in groups {
        let members = iam
            .list_group_members(group.id)
            .await
            .map_err(map_iam_error)?;
        let roles = iam.group_roles(group.id).await.map_err(map_iam_error)?;
        results.push(GroupDetails {
            group,
            members,
            roles,
        });
    }

    let next_cursor = if has_more {
        results.last().and_then(|last| {
            encode_cursor(&GroupCursor {
                name: last.group.name.clone(),
                id: last.group.id,
            })
            .ok()
        })
    } else {
        None
    };

    Ok(Json(CursorPaged::new(
        results,
        limit,
        has_more,
        next_cursor,
    )))
}

pub async fn create_group_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Json(payload): Json<CreateGroupRequest>,
) -> Result<Json<GroupDetails>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let group = iam.create_group(payload).await.map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.group.create",
            Some(user.actor.clone()),
            "group",
            Some(group.id.to_string()),
            json!({ "name": group.name }),
        )
        .await;
    Ok(Json(GroupDetails {
        group,
        members: vec![],
        roles: vec![],
    }))
}

pub async fn get_group_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<GroupDetails>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let group = iam.get_group(id).await.map_err(map_iam_error)?;
    let members = iam.list_group_members(id).await.map_err(map_iam_error)?;
    let roles = iam.group_roles(id).await.map_err(map_iam_error)?;
    Ok(Json(GroupDetails {
        group,
        members,
        roles,
    }))
}

pub async fn update_group_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateGroupRequest>,
) -> Result<Json<GroupDetails>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let record = iam.update_group(id, payload).await.map_err(map_iam_error)?;
    let members = iam.list_group_members(id).await.map_err(map_iam_error)?;
    let roles = iam.group_roles(id).await.map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.group.update",
            Some(user.actor.clone()),
            "group",
            Some(id.to_string()),
            json!({ "status": record.status }),
        )
        .await;
    Ok(Json(GroupDetails {
        group: record,
        members,
        roles,
    }))
}

pub async fn delete_group_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    iam.delete_group(id).await.map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.group.delete",
            Some(user.actor.clone()),
            "group",
            Some(id.to_string()),
            json!({}),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_member_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<AddMemberRequest>,
) -> Result<Json<Vec<IamUserRecord>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    iam.add_group_member(id, payload.user_id)
        .await
        .map_err(map_iam_error)?;
    let members = iam.list_group_members(id).await.map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.group.member.add",
            Some(user.actor.clone()),
            "group",
            Some(id.to_string()),
            json!({ "user_id": payload.user_id }),
        )
        .await;
    Ok(Json(members))
}

pub async fn list_group_members_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<IamUserRecord>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let members = state
        .iam()
        .list_group_members(id)
        .await
        .map_err(map_iam_error)?;
    Ok(Json(members))
}

pub async fn remove_member_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path((group_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    iam.remove_group_member(group_id, user_id)
        .await
        .map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.group.member.remove",
            Some(user.actor.clone()),
            "group",
            Some(group_id.to_string()),
            json!({ "user_id": user_id }),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn assign_user_role_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<AssignRoleRequest>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    if payload.role.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new("VALIDATION_ERROR", "role required")),
        ));
    }
    let iam = state.iam();
    let roles = iam
        .assign_role_to_user(id, payload.role.trim())
        .await
        .map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.user.role.assign",
            Some(user.actor.clone()),
            "user",
            Some(id.to_string()),
            json!({ "role": payload.role }),
        )
        .await;
    Ok(Json(roles))
}

pub async fn revoke_user_role_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path((id, role)): Path<(Uuid, String)>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let roles = iam
        .revoke_role_from_user(id, role.trim())
        .await
        .map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.user.role.revoke",
            Some(user.actor.clone()),
            "user",
            Some(id.to_string()),
            json!({ "role": role }),
        )
        .await;
    Ok(Json(roles))
}

pub async fn assign_group_role_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<AssignRoleRequest>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    if payload.role.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new("VALIDATION_ERROR", "role required")),
        ));
    }
    let iam = state.iam();
    let roles = iam
        .assign_role_to_group(id, payload.role.trim())
        .await
        .map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.group.role.assign",
            Some(user.actor.clone()),
            "group",
            Some(id.to_string()),
            json!({ "role": payload.role }),
        )
        .await;
    Ok(Json(roles))
}

pub async fn revoke_group_role_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path((id, role)): Path<(Uuid, String)>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let roles = iam
        .revoke_role_from_group(id, role.trim())
        .await
        .map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.group.role.revoke",
            Some(user.actor.clone()),
            "group",
            Some(id.to_string()),
            json!({ "role": role }),
        )
        .await;
    Ok(Json(roles))
}

pub async fn list_roles_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
) -> Result<Json<Vec<IamRoleRecord>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let roles = state.iam().list_roles().await.map_err(map_iam_error)?;
    Ok(Json(roles))
}

pub async fn list_service_accounts_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<CursorPaged<ServiceAccountDetails>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let limit = cursor_limit(query.limit);
    let cursor = query
        .cursor
        .as_deref()
        .map(decode_cursor::<ServiceAccountCursor>)
        .transpose()
        .map_err(|message| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::new("INVALID_CURSOR", message)),
            )
        })?;
    let cursor_name = cursor.as_ref().map(|c| c.name.clone()).unwrap_or_default();
    let cursor_id = cursor.as_ref().map(|c| c.id).unwrap_or_else(Uuid::nil);

    let iam = state.iam();
    let accounts = sqlx::query_as::<_, ServiceAccountRecord>(
        r#"
        SELECT id, name, description, status, token_hint, created_at, last_rotated_at
        FROM iam_service_accounts
        WHERE ($1::text = '' OR (name, id) > ($1, $2))
        ORDER BY name ASC, id ASC
        LIMIT $3
    "#,
    )
    .bind(&cursor_name)
    .bind(cursor_id)
    .bind((limit + 1) as i64)
    .fetch_all(iam.pool())
    .await
    .map_err(|err| map_iam_error(map_db_error(err)))?;
    let has_more = accounts.len() > limit as usize;
    let mut accounts = accounts;
    if has_more {
        accounts.truncate(limit as usize);
    }
    let mut results = Vec::with_capacity(accounts.len());
    for account in accounts {
        let roles = iam
            .service_account_roles(account.id)
            .await
            .map_err(map_iam_error)?;
        results.push(ServiceAccountDetails { account, roles });
    }

    let next_cursor = if has_more {
        results.last().and_then(|last| {
            encode_cursor(&ServiceAccountCursor {
                name: last.account.name.clone(),
                id: last.account.id,
            })
            .ok()
        })
    } else {
        None
    };

    Ok(Json(CursorPaged::new(
        results,
        limit,
        has_more,
        next_cursor,
    )))
}

pub async fn get_service_account_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<ServiceAccountDetails>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let account = iam.get_service_account(id).await.map_err(map_iam_error)?;
    let roles = iam.service_account_roles(id).await.map_err(map_iam_error)?;
    Ok(Json(ServiceAccountDetails { account, roles }))
}

pub async fn create_service_account_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Json(payload): Json<CreateServiceAccountRequest>,
) -> Result<Json<ServiceAccountWithToken>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let (account, token) = iam
        .create_service_account(payload)
        .await
        .map_err(map_iam_error)?;
    let roles = iam
        .service_account_roles(account.id)
        .await
        .map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.service_account.create",
            Some(user.actor.clone()),
            "service_account",
            Some(account.id.to_string()),
            json!({ "name": account.name }),
        )
        .await;
    Ok(Json(ServiceAccountWithToken {
        account,
        token,
        roles,
    }))
}

pub async fn rotate_service_account_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateServiceAccountRoles>,
) -> Result<Json<ServiceAccountWithToken>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    let (account, token) = iam
        .rotate_service_account(id, Some(payload))
        .await
        .map_err(map_iam_error)?;
    let roles = iam
        .service_account_roles(account.id)
        .await
        .map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.service_account.rotate",
            Some(user.actor.clone()),
            "service_account",
            Some(account.id.to_string()),
            json!({}),
        )
        .await;
    Ok(Json(ServiceAccountWithToken {
        account,
        token,
        roles,
    }))
}

pub async fn disable_service_account_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_ADMIN).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let iam = state.iam();
    iam.disable_service_account(id)
        .await
        .map_err(map_iam_error)?;
    state
        .log_iam_event(
            "iam.service_account.disable",
            Some(user.actor.clone()),
            "service_account",
            Some(id.to_string()),
            json!({}),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn whoami_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
) -> Result<Json<WhoAmIResponse>, (StatusCode, Json<ApiError>)> {
    let mut username = None;
    let mut email = None;
    let mut display_name = None;

    if let (PrincipalType::User, Some(user_id)) = (&user.principal_type, user.principal_id) {
        let profile =
            sqlx::query("SELECT username, email, display_name FROM iam_users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(state.iam().pool())
                .await
                .map_err(|err| map_iam_error(map_db_error(err)))?;

        if let Some(row) = profile {
            username = row.get::<Option<String>, _>("username");
            email = row.get::<Option<String>, _>("email");
            display_name = row.get::<Option<String>, _>("display_name");
        }
    }

    Ok(Json(WhoAmIResponse {
        actor: user.actor.clone(),
        principal_type: user.principal_type.clone(),
        principal_id: user.principal_id,
        roles: user.roles_list(),
        permissions: user.permissions_list(),
        username,
        email,
        display_name,
    }))
}

pub async fn list_audit_route(
    State(state): State<AppState>,
    Extension(user): Extension<UserContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<CursorPaged<IamAuditRecord>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_IAM_VIEW).map_err(|status| (status, Json(ApiError::forbidden())))?;
    let limit = cursor_limit(query.limit);
    let cursor = query
        .cursor
        .as_deref()
        .map(decode_cursor::<AuditCursor>)
        .transpose()
        .map_err(|message| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::new("INVALID_CURSOR", message)),
            )
        })?;
    let cursor_created_at = cursor.as_ref().map(|c| c.created_at);
    let cursor_id = cursor.as_ref().map(|c| c.id).unwrap_or_else(Uuid::nil);

    let events = sqlx::query_as::<_, IamAuditRecord>(
        r#"
        SELECT id, actor, action, target_type, target_id, payload, created_at
        FROM iam_audit_events
        WHERE ($1::timestamptz IS NULL OR (created_at, id) < ($1, $2))
        ORDER BY created_at DESC, id DESC
        LIMIT $3
    "#,
    )
    .bind(cursor_created_at)
    .bind(cursor_id)
    .bind((limit + 1) as i64)
    .fetch_all(state.iam().pool())
    .await
    .map_err(|err| map_iam_error(map_db_error(err)))?;

    let has_more = events.len() > limit as usize;
    let mut events = events;
    if has_more {
        events.truncate(limit as usize);
    }

    let next_cursor = if has_more {
        events.last().and_then(|last| {
            encode_cursor(&AuditCursor {
                created_at: last.created_at,
                id: last.id,
            })
            .ok()
        })
    } else {
        None
    };

    Ok(Json(CursorPaged::new(events, limit, has_more, next_cursor)))
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct ServiceAccountDetails {
    pub account: ServiceAccountRecord,
    pub roles: Vec<String>,
}
