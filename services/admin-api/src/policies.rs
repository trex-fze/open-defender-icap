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
use taxonomy::FallbackReason;
use tracing::error;
use uuid::Uuid;

use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_EDIT, ROLE_POLICY_PUBLISH, ROLE_POLICY_VIEW},
    pagination::{
        cursor_limit, decode_cursor_with_direction, encode_directional_cursor, CursorDirection,
        CursorPaged,
    },
    ApiError, AppState, PolicyEngineRuntimeSummary,
};

const POLICY_STATUS_ACTIVE: &str = "active";
const POLICY_STATUS_DRAFT: &str = "draft";
const POLICY_STATUS_ARCHIVED: &str = "archived";

#[derive(Debug, Deserialize)]
pub struct PolicyListParams {
    limit: Option<u32>,
    cursor: Option<String>,
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    page_size: Option<u32>,
    status: Option<String>,
    search: Option<String>,
    #[serde(default)]
    include_drafts: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct PolicyCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
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

#[derive(Debug, Serialize)]
pub struct PolicyVersionSummary {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub version: String,
    pub status: String,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub deployed_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub rule_count: i64,
}

#[derive(Debug, Serialize)]
pub struct PolicyRuntimeSnapshot {
    pub policy_id: Option<String>,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct PolicyRuntimeSyncStatus {
    pub control_plane: Option<PolicyRuntimeSnapshot>,
    pub runtime: PolicyRuntimeSnapshot,
    pub in_sync: bool,
    pub drift_reason: Option<String>,
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
) -> Result<Json<CursorPaged<PolicySummary>>, StatusCode> {
    require_roles(&user, ROLE_POLICY_VIEW)?;
    let limit = cursor_limit(params.limit.or(params.page_size));
    let legacy_page = params.page.unwrap_or(1).max(1);
    let cursor = params
        .cursor
        .as_deref()
        .map(decode_cursor_with_direction::<PolicyCursor>)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (cursor_direction, cursor_anchor) = cursor
        .as_ref()
        .map(|(direction, anchor)| (*direction, Some(anchor)))
        .unwrap_or((CursorDirection::Next, None));
    let cursor_created_at = cursor_anchor.map(|c| c.created_at);
    let cursor_id = cursor_anchor.map(|c| c.id).unwrap_or_else(Uuid::nil);

    let mut qb = QueryBuilder::new(
        "SELECT p.id, p.name, p.version, p.status, p.created_by, p.created_at,
                COALESCE((SELECT COUNT(*) FROM policy_rules pr WHERE pr.policy_id = p.id), 0) as rule_count
         FROM policies p",
    );
    apply_filters(&mut qb, &params);
    qb.push(" AND (")
        .push_bind(cursor_created_at)
        .push(" IS NULL OR (p.created_at, p.id) ");
    if cursor_direction == CursorDirection::Prev {
        qb.push(">");
    } else {
        qb.push("<");
    }
    qb.push(" (")
        .push_bind(cursor_created_at)
        .push(", ")
        .push_bind(cursor_id)
        .push("))")
        .push(" ORDER BY p.created_at ");
    if cursor_direction == CursorDirection::Prev {
        qb.push("ASC, p.id ASC LIMIT ");
    } else {
        qb.push("DESC, p.id DESC LIMIT ");
    }
    qb.push_bind((limit + 1) as i64);

    if params.cursor.is_none() && legacy_page > 1 {
        qb.push(" OFFSET ")
            .push_bind(((legacy_page - 1) * limit) as i64);
    }

    let rows = qb
        .build()
        .fetch_all(state.pool())
        .await
        .map_err(map_db_error)?;
    let has_more = rows.len() > limit as usize;
    let mut data: Vec<PolicySummary> = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| PolicySummary {
            id: row.get("id"),
            name: row.get("name"),
            version: row.get("version"),
            status: row.get("status"),
            created_by: row.get("created_by"),
            created_at: row.get("created_at"),
            rule_count: row.get("rule_count"),
        })
        .collect();
    if cursor_direction == CursorDirection::Prev {
        data.reverse();
    }

    let next_cursor = if has_more {
        data.last().and_then(|item| {
            encode_directional_cursor(
                CursorDirection::Next,
                &PolicyCursor {
                    created_at: item.created_at,
                    id: item.id,
                },
            )
            .ok()
        })
    } else {
        None
    };
    let prev_cursor = if params.cursor.is_some() && !data.is_empty() {
        data.first().and_then(|item| {
            encode_directional_cursor(
                CursorDirection::Prev,
                &PolicyCursor {
                    created_at: item.created_at,
                    id: item.id,
                },
            )
            .ok()
        })
    } else {
        None
    };

    Ok(Json(CursorPaged::new_with_prev(
        std::mem::take(&mut data),
        limit,
        has_more,
        next_cursor,
        prev_cursor,
    )))
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

pub async fn list_policy_versions(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(policy_id): Path<Uuid>,
) -> Result<Json<Vec<PolicyVersionSummary>>, StatusCode> {
    require_roles(&user, ROLE_POLICY_VIEW)?;
    let versions = sqlx::query(
        r#"SELECT id, policy_id, version, status, created_by, created_at, deployed_at, notes,
                  COALESCE(jsonb_array_length(rules), 0)::bigint AS rule_count
           FROM policy_versions
           WHERE policy_id = $1
           ORDER BY created_at DESC"#,
    )
    .bind(policy_id)
    .fetch_all(state.pool())
    .await
    .map_err(map_db_error)?;

    let mapped = versions
        .into_iter()
        .map(|row| {
            Ok(PolicyVersionSummary {
                id: row.try_get("id").map_err(map_policy_version_row_error)?,
                policy_id: row
                    .try_get("policy_id")
                    .map_err(map_policy_version_row_error)?,
                version: row
                    .try_get("version")
                    .map_err(map_policy_version_row_error)?,
                status: row
                    .try_get("status")
                    .map_err(map_policy_version_row_error)?,
                created_by: row
                    .try_get("created_by")
                    .map_err(map_policy_version_row_error)?,
                created_at: row
                    .try_get("created_at")
                    .map_err(map_policy_version_row_error)?,
                deployed_at: row
                    .try_get("deployed_at")
                    .map_err(map_policy_version_row_error)?,
                notes: row.try_get("notes").map_err(map_policy_version_row_error)?,
                rule_count: row
                    .try_get("rule_count")
                    .map_err(map_policy_version_row_error)?,
            })
        })
        .collect::<Result<Vec<_>, StatusCode>>()?;

    Ok(Json(mapped))
}

fn map_policy_version_row_error(err: sqlx::Error) -> StatusCode {
    error!(target = "svc-admin", %err, "failed to map policy version history row");
    StatusCode::INTERNAL_SERVER_ERROR
}

pub async fn policy_runtime_sync(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<Json<PolicyRuntimeSyncStatus>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let active = sqlx::query(
        r#"SELECT id::text AS id, version
           FROM policies
           WHERE status = $1
           ORDER BY created_at DESC
           LIMIT 1"#,
    )
    .bind(POLICY_STATUS_ACTIVE)
    .fetch_optional(state.pool())
    .await
    .map_err(map_db_error_tx)?
    .map(|row| PolicyRuntimeSnapshot {
        policy_id: row.get::<Option<String>, _>("id"),
        version: row.get("version"),
    });

    let runtime = state.fetch_policy_engine_runtime().await.map_err(|err| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new(
                "POLICY_RUNTIME_UNAVAILABLE",
                &format!("failed to fetch policy-engine runtime: {err}"),
            )),
        )
    })?;
    let runtime_snapshot = policy_runtime_snapshot(runtime);

    let (in_sync, drift_reason) = match active.as_ref() {
        None => (false, Some("no active control-plane policy".to_string())),
        Some(control) => {
            let id_matches = control.policy_id == runtime_snapshot.policy_id;
            let version_matches = control.version == runtime_snapshot.version;
            if id_matches && version_matches {
                (true, None)
            } else {
                let reason = format!(
                    "control-plane id/version ({:?}/{}) differs from runtime ({:?}/{})",
                    control.policy_id,
                    control.version,
                    runtime_snapshot.policy_id,
                    runtime_snapshot.version
                );
                (false, Some(reason))
            }
        }
    };

    Ok(Json(PolicyRuntimeSyncStatus {
        control_plane: active,
        runtime: runtime_snapshot,
        in_sync,
        drift_reason,
    }))
}

fn policy_runtime_snapshot(input: PolicyEngineRuntimeSummary) -> PolicyRuntimeSnapshot {
    PolicyRuntimeSnapshot {
        policy_id: input.policy_id,
        version: input.version,
    }
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
    validate_rules_against_taxonomy(&state, &rules)?;
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

    propagate_policy_runtime_change(&state, Some(user.actor.as_str()), policy_id, "create").await?;

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
    let existing_status: String = existing.get("status");
    let mut status: String = existing_status.clone();
    if let Some(value) = payload.name.as_ref() {
        name = value.trim().to_string();
    }
    if let Some(value) = payload.version.as_ref() {
        version = value.trim().to_string();
    }
    if let Some(value) = payload.status.as_ref() {
        status = normalize_policy_status(value)?;
        if status == POLICY_STATUS_ACTIVE {
            return Err(crate::validation_error(
                "setting status=active via update is not allowed; use publish endpoint",
            ));
        }
        ensure_can_disable_status(existing_status.as_str(), status.as_str())?;
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
        validate_rules_against_taxonomy(&state, &normalized)?;
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

    propagate_policy_runtime_change(&state, Some(user.actor.as_str()), policy_id, "update").await?;

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

    sqlx::query("UPDATE policies SET status = $1 WHERE status = $2 AND id <> $3")
        .bind(POLICY_STATUS_ARCHIVED)
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

    propagate_policy_runtime_change(&state, Some(user.actor.as_str()), policy_id, "publish")
        .await?;

    Ok(Json(detail))
}

pub async fn delete_policy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(policy_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_PUBLISH)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let mut tx = state.pool().begin().await.map_err(map_db_error_tx)?;
    let row = sqlx::query("SELECT id, name, version, status FROM policies WHERE id = $1")
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

    let status: String = row.get("status");
    ensure_can_delete_status(status.as_str())?;

    let name: String = row.get("name");
    let version: String = row.get("version");

    sqlx::query("DELETE FROM policies WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .map_err(map_db_error_tx)?;
    tx.commit().await.map_err(map_db_error_tx)?;

    state
        .log_policy_event(
            "policy.delete",
            Some(user.actor.clone()),
            Some(policy_id.to_string()),
            serde_json::json!({
                "name": name,
                "version": version,
                "status": status,
            }),
        )
        .await;

    propagate_policy_runtime_change(&state, Some(user.actor.as_str()), policy_id, "delete").await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn propagate_policy_runtime_change(
    state: &AppState,
    actor: Option<&str>,
    policy_id: Uuid,
    operation: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    state.invalidate_policy_cache().await;
    if let Err(err) = state.trigger_policy_reload().await {
        let payload = serde_json::json!({
            "operation": operation,
            "result": "failed",
            "error": err.to_string(),
            "next_step": "retry /api/v1/policies/reload",
        });
        state
            .log_policy_event(
                "policy.propagate_runtime_change",
                actor.map(std::string::ToString::to_string),
                Some(policy_id.to_string()),
                payload,
            )
            .await;
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new(
                "POLICY_RELOAD_FAILED",
                "policy persisted, cache invalidated, but policy-engine reload failed; run /api/v1/policies/reload",
            )),
        ));
    }

    state
        .log_policy_event(
            "policy.propagate_runtime_change",
            actor.map(std::string::ToString::to_string),
            Some(policy_id.to_string()),
            serde_json::json!({
                "operation": operation,
                "result": "ok",
            }),
        )
        .await;
    Ok(())
}

pub async fn validate_policy(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(payload): Json<PolicyDraftRequest>,
) -> Result<Json<PolicyValidationResponse>, StatusCode> {
    require_roles(&user, ROLE_POLICY_EDIT)?;
    match normalize_rules(&payload.rules).and_then(|rules| {
        validate_rules_against_taxonomy(&state, &rules)?;
        Ok(rules)
    }) {
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
    let mut seen_ids = HashSet::new();
    let mut normalized = Vec::with_capacity(rules.len());
    for rule in rules {
        if !seen_priorities.insert(rule.priority) {
            return Err(crate::validation_error(
                "duplicate priorities are not allowed",
            ));
        }
        let action = parse_action(&rule.action)?;
        validate_domain_patterns(&rule.conditions)?;
        let id = rule
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let id = id.trim().to_string();
        if id.is_empty() {
            return Err(crate::validation_error("rule id cannot be empty"));
        }
        if !seen_ids.insert(id.clone()) {
            return Err(crate::validation_error(
                "duplicate rule ids are not allowed",
            ));
        }
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

fn validate_domain_patterns(conditions: &Conditions) -> Result<(), (StatusCode, Json<ApiError>)> {
    let Some(domains) = conditions.domains.as_ref() else {
        return Ok(());
    };
    for domain in domains {
        let candidate = domain.trim().to_ascii_lowercase();
        if candidate.is_empty() {
            return Err(crate::validation_error(
                "domain conditions must not contain empty values",
            ));
        }
        if candidate.contains("://") || candidate.contains('/') {
            return Err(crate::validation_error(
                "domain conditions must use host patterns only (no scheme/path)",
            ));
        }
        let stripped = candidate.strip_prefix("*.").unwrap_or(candidate.as_str());
        let labels: Vec<&str> = stripped.split('.').collect();
        if labels.len() < 2 {
            return Err(crate::validation_error(
                "domain conditions must contain a valid host with at least one dot",
            ));
        }
        for label in labels {
            if label.is_empty() || !is_valid_domain_label(label) {
                return Err(crate::validation_error(
                    "domain conditions contain an invalid hostname label",
                ));
            }
        }
    }
    Ok(())
}

fn is_valid_domain_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    if bytes.is_empty() || bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    bytes
        .iter()
        .all(|ch| ch.is_ascii_alphanumeric() || *ch == b'-')
}

fn validate_rules_against_taxonomy(
    state: &AppState,
    rules: &[PolicyRule],
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let taxonomy = state.taxonomy_store();
    for rule in rules {
        let Some(categories) = rule.conditions.categories.as_ref() else {
            continue;
        };
        for category in categories {
            let validated = taxonomy.validate_category(category);
            if matches!(
                validated.fallback_reason,
                Some(FallbackReason::MissingCategory | FallbackReason::UnknownCategory)
            ) {
                return Err(crate::validation_error(&format!(
                    "rule '{}' references invalid category '{}'",
                    rule.id, category
                )));
            }
        }
    }
    Ok(())
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
        "ContentPending" | "contentpending" | "content-pending" => Ok(PolicyAction::ContentPending),
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

fn ensure_can_disable_status(
    existing_status: &str,
    next_status: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if existing_status == POLICY_STATUS_ACTIVE && next_status == POLICY_STATUS_ARCHIVED {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "ACTIVE_POLICY_GUARD",
                "active policy cannot be disabled directly; activate another policy first",
            )),
        ));
    }
    Ok(())
}

fn ensure_can_delete_status(status: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    if status == POLICY_STATUS_ACTIVE {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::new(
                "ACTIVE_POLICY_GUARD",
                "active policy cannot be deleted; activate another policy first",
            )),
        ));
    }
    Ok(())
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

    #[test]
    fn disable_guard_blocks_archiving_active_policy() {
        let result = ensure_can_disable_status(POLICY_STATUS_ACTIVE, POLICY_STATUS_ARCHIVED);
        assert!(result.is_err());
    }

    #[test]
    fn disable_guard_allows_non_active_transitions() {
        assert!(ensure_can_disable_status(POLICY_STATUS_DRAFT, POLICY_STATUS_ARCHIVED).is_ok());
        assert!(ensure_can_disable_status(POLICY_STATUS_ARCHIVED, POLICY_STATUS_DRAFT).is_ok());
    }

    #[test]
    fn delete_guard_blocks_active_policy() {
        let result = ensure_can_delete_status(POLICY_STATUS_ACTIVE);
        assert!(result.is_err());
    }
}
