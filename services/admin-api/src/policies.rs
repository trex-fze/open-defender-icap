use std::collections::HashSet;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common_types::PolicyAction;
use policy_dsl::{Conditions, PolicyRule};
use serde::{Deserialize, Serialize};
use sqlx::{types::Json as SqlJson, PgPool, Postgres, QueryBuilder, Row, Transaction};
use tracing::error;
use uuid::Uuid;

use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_EDIT, ROLE_POLICY_PUBLISH, ROLE_POLICY_VIEW},
    pagination::{PageOptions, Paged},
    ApiError, AppState,
};

const POLICY_STATUS_ACTIVE: &str = "active";
const POLICY_STATUS_DRAFT: &str = "draft";
const POLICY_STATUS_ARCHIVED: &str = "archived";

#[derive(Debug, Deserialize)]
pub struct PolicyListParams {
    page: Option<u32>,
    page_size: Option<u32>,
    status: Option<String>,
    search: Option<String>,
    #[serde(default)]
    include_drafts: bool,
}

#[derive(Debug, Serialize)]
pub struct PolicySummary {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub status: String,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub rule_count: i64,
}

#[derive(Debug, Serialize)]
pub struct PolicyDetail {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub status: String,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub rule_count: i64,
    pub rules: Vec<PolicyRuleResponse>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PolicyRuleResponse {
    pub id: String,
    pub description: Option<String>,
    pub priority: u32,
    pub action: PolicyAction,
    pub conditions: Conditions,
}

#[derive(Debug, Deserialize)]
pub struct PolicyDraftRequest {
    pub name: String,
    pub version: Option<String>,
    pub created_by: Option<String>,
    pub notes: Option<String>,
    pub rules: Vec<PolicyRulePayload>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyUpdateRequest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub status: Option<String>,
    pub notes: Option<String>,
    pub rules: Option<Vec<PolicyRulePayload>>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyPublishRequest {
    pub version: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PolicyRulePayload {
    pub id: Option<String>,
    pub description: Option<String>,
    pub priority: u32,
    pub action: String,
    #[serde(default)]
    pub conditions: Conditions,
}

#[derive(Debug, Serialize)]
pub struct PolicyValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
}

pub async fn list_policies(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(params): Query<PolicyListParams>,
) -> Result<Json<Paged<PolicySummary>>, StatusCode> {
    require_roles(&user, ROLE_POLICY_VIEW)?;
    let opts = PageOptions::new(params.page, params.page_size);
    let total = build_policy_count_query(state.pool(), &params)
        .await
        .map_err(map_db_error)?;
    if total == 0 {
        return Ok(Json(Paged::new(Vec::new(), 0, opts)));
    }
    let data = build_policy_list_query(state.pool(), &params, &opts)
        .await
        .map_err(map_db_error)?;
    Ok(Json(Paged::new(data, total, opts)))
}

pub async fn get_policy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(policy_id): Path<Uuid>,
) -> Result<Json<PolicyDetail>, StatusCode> {
    require_roles(&user, ROLE_POLICY_VIEW)?;
    let detail = fetch_policy_detail(state.pool(), policy_id)
        .await
        .map_err(map_db_error)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(detail))
}

pub async fn create_policy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(mut payload): Json<PolicyDraftRequest>,
) -> Result<Json<PolicyDetail>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    if payload.created_by.is_none() {
        payload.created_by = Some(user.actor.clone());
    }

    let rules = normalize_rules(&payload.rules)?;
    let version = payload
        .version
        .clone()
        .unwrap_or_else(|| format!("draft-{}", Utc::now().format("%Y%m%d%H%M%S")));

    let mut tx = state.pool().begin().await.map_err(map_db_error_tx)?;
    let policy_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO policies (id, name, version, status, created_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(policy_id)
    .bind(&payload.name)
    .bind(&version)
    .bind(POLICY_STATUS_DRAFT)
    .bind(&payload.created_by)
    .execute(&mut *tx)
    .await
    .map_err(map_db_error_tx)?;

    replace_policy_rules(&mut tx, policy_id, &rules)
        .await
        .map_err(map_db_error_tx)?;
    insert_policy_version(
        &mut tx,
        policy_id,
        &version,
        POLICY_STATUS_DRAFT,
        payload.created_by.as_deref(),
        payload.notes.as_deref(),
        None,
        &rules,
    )
    .await
    .map_err(map_db_error_tx)?;
    tx.commit().await.map_err(map_db_error_tx)?;

    let detail = fetch_policy_detail(state.pool(), policy_id)
        .await
        .map_err(map_db_error_tx)?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "policy not found")),
        ))?;

    state
        .log_policy_event(
            "policy.create",
            payload.created_by,
            Some(policy_id.to_string()),
            &detail,
        )
        .await;

    Ok(Json(detail))
}

pub async fn update_policy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(policy_id): Path<Uuid>,
    Json(payload): Json<PolicyUpdateRequest>,
) -> Result<Json<PolicyDetail>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let mut tx = state.pool().begin().await.map_err(map_db_error_tx)?;
    let existing = sqlx::query(
        "SELECT name, version, status, created_by, created_at FROM policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_db_error_tx)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "policy not found")),
        )
    })?;

    let mut name: String = existing.get("name");
    let mut version: String = existing.get("version");
    let mut status: String = existing.get("status");
    if let Some(value) = payload.name.as_ref() {
        name = value.trim().to_string();
    }
    if let Some(value) = payload.version.as_ref() {
        version = value.trim().to_string();
    }
    if let Some(value) = payload.status.as_ref() {
        status = normalize_policy_status(value)?;
    }

    sqlx::query("UPDATE policies SET name = $1, version = $2, status = $3 WHERE id = $4")
        .bind(&name)
        .bind(&version)
        .bind(&status)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .map_err(map_db_error_tx)?;

    let rules = if let Some(rules) = payload.rules.as_ref() {
        let normalized = normalize_rules(rules)?;
        replace_policy_rules(&mut tx, policy_id, &normalized)
            .await
            .map_err(map_db_error_tx)?;
        normalized
    } else {
        load_rules_from_latest_version(&mut tx, policy_id)
            .await
            .map_err(map_db_error_tx)?
    };

    insert_policy_version(
        &mut tx,
        policy_id,
        &version,
        &status,
        Some(&user.actor),
        payload.notes.as_deref(),
        None,
        &rules,
    )
    .await
    .map_err(map_db_error_tx)?;
    tx.commit().await.map_err(map_db_error_tx)?;

    let detail = fetch_policy_detail(state.pool(), policy_id)
        .await
        .map_err(map_db_error_tx)?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "policy not found")),
        ))?;

    state
        .log_policy_event(
            "policy.update",
            Some(user.actor.clone()),
            Some(policy_id.to_string()),
            &detail,
        )
        .await;

    Ok(Json(detail))
}

pub async fn publish_policy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(policy_id): Path<Uuid>,
    Json(payload): Json<PolicyPublishRequest>,
) -> Result<Json<PolicyDetail>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_PUBLISH)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let mut tx = state.pool().begin().await.map_err(map_db_error_tx)?;
    let exists = sqlx::query("SELECT id FROM policies WHERE id = $1")
        .bind(policy_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_db_error_tx)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError::new("NOT_FOUND", "policy not found")),
            )
        })?;
    drop(exists);

    sqlx::query("UPDATE policies SET status = $1 WHERE status = $1 AND id <> $2")
        .bind(POLICY_STATUS_ACTIVE)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .map_err(map_db_error_tx)?;

    let new_version = payload
        .version
        .unwrap_or_else(|| format!("release-{}", Utc::now().format("%Y%m%d%H%M%S")));

    sqlx::query("UPDATE policies SET status = $1, version = $2 WHERE id = $3")
        .bind(POLICY_STATUS_ACTIVE)
        .bind(&new_version)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .map_err(map_db_error_tx)?;

    let rules = load_rules_from_latest_version(&mut tx, policy_id)
        .await
        .map_err(map_db_error_tx)?;
    insert_policy_version(
        &mut tx,
        policy_id,
        &new_version,
        POLICY_STATUS_ACTIVE,
        Some(&user.actor),
        payload.notes.as_deref(),
        Some(Utc::now()),
        &rules,
    )
    .await
    .map_err(map_db_error_tx)?;
    tx.commit().await.map_err(map_db_error_tx)?;

    state.invalidate_policy_cache().await;
    let detail = fetch_policy_detail(state.pool(), policy_id)
        .await
        .map_err(map_db_error_tx)?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "policy not found")),
        ))?;
    state
        .log_policy_event(
            "policy.publish",
            Some(user.actor.clone()),
            Some(policy_id.to_string()),
            &detail,
        )
        .await;
    Ok(Json(detail))
}

pub async fn validate_policy(
    Extension(user): Extension<UserContext>,
    Json(payload): Json<PolicyDraftRequest>,
) -> Result<Json<PolicyValidationResponse>, StatusCode> {
    require_roles(&user, ROLE_POLICY_EDIT)?;
    match normalize_rules(&payload.rules) {
        Ok(_) => Ok(Json(PolicyValidationResponse {
            valid: true,
            errors: Vec::new(),
        })),
        Err((status, Json(err))) if status == StatusCode::BAD_REQUEST => {
            Ok(Json(PolicyValidationResponse {
                valid: false,
                errors: vec![err.message().to_string()],
            }))
        }
        Err((status, _)) => Err(status),
    }
}

fn normalize_rules(
    rules: &[PolicyRulePayload],
) -> Result<Vec<PolicyRule>, (StatusCode, Json<ApiError>)> {
    if rules.is_empty() {
        return Err(crate::validation_error("at least one rule required"));
    }
    let mut seen_priorities = HashSet::new();
    let mut normalized = Vec::with_capacity(rules.len());
    for rule in rules {
        if !seen_priorities.insert(rule.priority) {
            return Err(crate::validation_error(
                "duplicate priorities are not allowed",
            ));
        }
        let action = parse_action(&rule.action)?;
        let id = rule
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        normalized.push(PolicyRule {
            id,
            description: rule.description.clone(),
            priority: rule.priority,
            action,
            conditions: rule.conditions.clone(),
        });
    }
    Ok(normalized)
}

fn parse_action(value: &str) -> Result<PolicyAction, (StatusCode, Json<ApiError>)> {
    match value.trim() {
        "Allow" | "allow" => Ok(PolicyAction::Allow),
        "Block" | "block" => Ok(PolicyAction::Block),
        "Warn" | "warn" => Ok(PolicyAction::Warn),
        "Monitor" | "monitor" => Ok(PolicyAction::Monitor),
        "Review" | "review" => Ok(PolicyAction::Review),
        "RequireApproval" | "requireapproval" | "require-approval" => {
            Ok(PolicyAction::RequireApproval)
        }
        _ => Err(crate::validation_error("unsupported action value")),
    }
}

fn normalize_policy_status(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let lowered = value.trim().to_ascii_lowercase();
    match lowered.as_str() {
        POLICY_STATUS_ACTIVE => Ok(POLICY_STATUS_ACTIVE.to_string()),
        POLICY_STATUS_DRAFT => Ok(POLICY_STATUS_DRAFT.to_string()),
        POLICY_STATUS_ARCHIVED => Ok(POLICY_STATUS_ARCHIVED.to_string()),
        _ => Err(crate::validation_error(
            "status must be active|draft|archived",
        )),
    }
}

async fn build_policy_count_query(
    pool: &PgPool,
    params: &PolicyListParams,
) -> Result<i64, sqlx::Error> {
    let mut qb = QueryBuilder::new("SELECT COUNT(*) as count FROM policies p");
    apply_filters(&mut qb, params);
    qb.build_query_scalar().fetch_one(pool).await
}

async fn build_policy_list_query(
    pool: &PgPool,
    params: &PolicyListParams,
    opts: &PageOptions,
) -> Result<Vec<PolicySummary>, sqlx::Error> {
    let mut qb = QueryBuilder::new(
        "SELECT p.id, p.name, p.version, p.status, p.created_by, p.created_at,
                COALESCE((SELECT COUNT(*) FROM policy_rules pr WHERE pr.policy_id = p.id), 0) as rule_count
         FROM policies p",
    );
    apply_filters(&mut qb, params);
    qb.push(" ORDER BY p.created_at DESC LIMIT ")
        .push_bind(opts.page_size as i64)
        .push(" OFFSET ")
        .push_bind(opts.offset());

    qb.build().fetch_all(pool).await.map(|rows| {
        rows.into_iter()
            .map(|row| PolicySummary {
                id: row.get("id"),
                name: row.get("name"),
                version: row.get("version"),
                status: row.get("status"),
                created_by: row.get("created_by"),
                created_at: row.get("created_at"),
                rule_count: row.get("rule_count"),
            })
            .collect()
    })
}

fn apply_filters(qb: &mut QueryBuilder<Postgres>, params: &PolicyListParams) {
    qb.push(" WHERE 1=1");
    if let Some(status) = params.status.as_ref() {
        qb.push(" AND p.status = ").push_bind(status.clone());
    } else if !params.include_drafts {
        qb.push(" AND p.status <> 'draft' ");
    }
    if let Some(search) = params.search.as_ref() {
        let pattern = format!("%{}%", search.to_ascii_lowercase());
        qb.push(" AND (LOWER(p.name) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR LOWER(p.version) LIKE ")
            .push_bind(pattern)
            .push(" OR p.id::text = ")
            .push_bind(search.trim().to_string());
        qb.push(")");
    }
}

async fn fetch_policy_detail(
    pool: &PgPool,
    policy_id: Uuid,
) -> Result<Option<PolicyDetail>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, name, version, status, created_by, created_at,
                COALESCE((SELECT COUNT(*) FROM policy_rules pr WHERE pr.policy_id = policies.id), 0) AS rule_count
         FROM policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let rules = load_rules_from_versions(pool, policy_id).await?;
    Ok(Some(PolicyDetail {
        id: row.get("id"),
        name: row.get("name"),
        version: row.get("version"),
        status: row.get("status"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        rule_count: row.get("rule_count"),
        rules: rules
            .into_iter()
            .map(|rule| PolicyRuleResponse {
                id: rule.id,
                description: rule.description,
                priority: rule.priority,
                action: rule.action,
                conditions: rule.conditions,
            })
            .collect(),
    }))
}

async fn load_rules_from_versions(
    pool: &PgPool,
    policy_id: Uuid,
) -> Result<Vec<PolicyRule>, sqlx::Error> {
    sqlx::query_scalar::<_, SqlJson<Vec<PolicyRule>>>(
        "SELECT rules FROM policy_versions WHERE policy_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(policy_id)
    .fetch_optional(pool)
    .await
    .map(|opt| opt.map(|SqlJson(rules)| rules).unwrap_or_default())
}

async fn load_rules_from_latest_version(
    tx: &mut Transaction<'_, Postgres>,
    policy_id: Uuid,
) -> Result<Vec<PolicyRule>, sqlx::Error> {
    sqlx::query_scalar::<_, SqlJson<Vec<PolicyRule>>>(
        "SELECT rules FROM policy_versions WHERE policy_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(policy_id)
    .fetch_optional(&mut **tx)
    .await
    .map(|opt| opt.map(|SqlJson(rules)| rules).unwrap_or_default())
}

async fn replace_policy_rules(
    tx: &mut Transaction<'_, Postgres>,
    policy_id: Uuid,
    rules: &[PolicyRule],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM policy_rules WHERE policy_id = $1")
        .bind(policy_id)
        .execute(&mut **tx)
        .await?;
    for rule in rules {
        sqlx::query(
            "INSERT INTO policy_rules (id, policy_id, priority, action, description, conditions)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(policy_id)
        .bind(rule.priority as i32)
        .bind(rule.action.to_string())
        .bind(&rule.description)
        .bind(SqlJson(rule.conditions.clone()))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_policy_version(
    tx: &mut Transaction<'_, Postgres>,
    policy_id: Uuid,
    version: &str,
    status: &str,
    created_by: Option<&str>,
    notes: Option<&str>,
    deployed_at: Option<DateTime<Utc>>,
    rules: &[PolicyRule],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO policy_versions (id, policy_id, version, status, created_by, notes, rules, deployed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(policy_id)
    .bind(version)
    .bind(status)
    .bind(created_by)
    .bind(notes)
    .bind(SqlJson(rules.to_vec()))
    .bind(deployed_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn map_db_error(err: sqlx::Error) -> StatusCode {
    error!(target = "svc-admin", %err, "database error");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn map_db_error_tx(err: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "database error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_validate_unique_priority() {
        let payload = PolicyRulePayload {
            id: Some("r1".into()),
            description: None,
            priority: 10,
            action: "Allow".into(),
            conditions: Conditions::default(),
        };
        assert!(normalize_rules(&[payload.clone(), payload]).is_err());
    }
}
