use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_EDIT, ROLE_POLICY_VIEW},
    pagination::{cursor_limit, decode_cursor, encode_cursor, CursorPaged},
    ApiError, AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common_types::{PolicyAction, PolicyDecision};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use tracing::{error, warn};

#[derive(Debug, Deserialize)]
pub struct ClassificationListQuery {
    pub state: Option<String>,
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ClassificationCursor {
    updated_at: DateTime<Utc>,
    normalized_key: String,
}

#[derive(Debug, Serialize)]
pub struct ClassificationListRecord {
    pub normalized_key: String,
    pub state: String,
    pub primary_category: Option<String>,
    pub subcategory: Option<String>,
    pub risk_level: Option<String>,
    pub recommended_action: Option<PolicyAction>,
    pub effective_action: Option<PolicyAction>,
    pub effective_decision_source: Option<String>,
    pub confidence: Option<f32>,
    pub status: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateClassificationRequest {
    pub primary_category: String,
    pub subcategory: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClassificationMutationRecord {
    pub normalized_key: String,
    pub primary_category: String,
    pub subcategory: String,
    pub risk_level: String,
    pub recommended_action: PolicyAction,
    pub confidence: f32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct PolicyDecisionRequestPayload {
    normalized_key: String,
    entity_level: String,
    source_ip: String,
    user_id: Option<String>,
    group_ids: Option<Vec<String>>,
    category_hint: Option<String>,
    subcategory_hint: Option<String>,
    risk_hint: Option<String>,
    confidence_hint: Option<f32>,
}

pub async fn list(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<ClassificationListQuery>,
) -> Result<Json<CursorPaged<ClassificationListRecord>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let limit = cursor_limit(query.limit);
    let state_filter = query
        .state
        .as_deref()
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "all".to_string());
    let q = query.q.unwrap_or_default();
    let q_like = format!("%{}%", q.trim().to_ascii_lowercase());

    let cursor = query
        .cursor
        .as_deref()
        .map(decode_cursor::<ClassificationCursor>)
        .transpose()
        .map_err(|message| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::new("INVALID_CURSOR", message)),
            )
        })?;

    let cursor_updated_at = cursor.as_ref().map(|c| c.updated_at);
    let cursor_key = cursor
        .as_ref()
        .map(|c| c.normalized_key.clone())
        .unwrap_or_default();

    let rows = sqlx::query(
        r#"WITH combined AS (
               SELECT normalized_key,
                      'classified'::text AS state,
                      primary_category,
                      subcategory,
                      risk_level,
                      recommended_action,
                      confidence::float8 AS confidence,
                      status,
                      updated_at
               FROM classifications
               WHERE status = 'active'
                 AND LOWER(normalized_key) LIKE $1
                 AND ($2 = 'all' OR $2 = 'classified')
               UNION ALL
               SELECT cr.normalized_key,
                      'unclassified'::text AS state,
                      NULL::text AS primary_category,
                      NULL::text AS subcategory,
                      NULL::text AS risk_level,
                      NULL::text AS recommended_action,
                      NULL::float8 AS confidence,
                      cr.status,
                      cr.updated_at
               FROM classification_requests cr
               LEFT JOIN classifications c
                 ON c.normalized_key = cr.normalized_key
                AND c.status = 'active'
               WHERE c.normalized_key IS NULL
                 AND LOWER(cr.normalized_key) LIKE $1
                 AND ($2 = 'all' OR $2 = 'unclassified')
           )
           SELECT normalized_key,
                  state,
                  primary_category,
                  subcategory,
                  risk_level,
                  recommended_action,
                  confidence,
                  status,
                  updated_at
           FROM combined
           WHERE ($3::timestamptz IS NULL OR (updated_at, normalized_key) < ($3, $4))
           ORDER BY updated_at DESC, normalized_key DESC
           LIMIT $5"#,
    )
    .bind(&q_like)
    .bind(&state_filter)
    .bind(cursor_updated_at)
    .bind(&cursor_key)
    .bind((limit + 1) as i64)
    .fetch_all(state.pool())
    .await
    .map_err(db_error)?;

    let mut out: Vec<ClassificationListRecord> = rows
        .into_iter()
        .map(|row| ClassificationListRecord {
            normalized_key: row.get("normalized_key"),
            state: row.get("state"),
            primary_category: row.get("primary_category"),
            subcategory: row.get("subcategory"),
            risk_level: row.get("risk_level"),
            recommended_action: row
                .try_get::<Option<String>, _>("recommended_action")
                .ok()
                .flatten()
                .as_deref()
                .and_then(parse_policy_action),
            effective_action: None,
            effective_decision_source: None,
            confidence: row
                .try_get::<Option<f64>, _>("confidence")
                .ok()
                .flatten()
                .map(|v| v as f32),
            status: row.get("status"),
            updated_at: row.get("updated_at"),
        })
        .collect();

    let has_more = out.len() > limit as usize;
    if has_more {
        out.truncate(limit as usize);
    }
    enrich_effective_decisions(&state, &mut out).await;
    let next_cursor = if has_more {
        out.last().and_then(|last| {
            encode_cursor(&ClassificationCursor {
                updated_at: last.updated_at,
                normalized_key: last.normalized_key.clone(),
            })
            .ok()
        })
    } else {
        None
    };
    Ok(Json(CursorPaged::new(out, limit, has_more, next_cursor)))
}

async fn enrich_effective_decisions(state: &AppState, out: &mut [ClassificationListRecord]) {
    for record in out.iter_mut() {
        if record.state != "classified" {
            continue;
        }

        let Some((entity_level, _)) = parse_normalized_key(&record.normalized_key) else {
            continue;
        };

        let payload = PolicyDecisionRequestPayload {
            normalized_key: record.normalized_key.clone(),
            entity_level: entity_level.to_string(),
            source_ip: "127.0.0.1".to_string(),
            user_id: None,
            group_ids: None,
            category_hint: record.primary_category.clone(),
            subcategory_hint: record.subcategory.clone(),
            risk_hint: record.risk_level.clone(),
            confidence_hint: record.confidence,
        };

        match state
            .evaluate_policy_decision::<_, PolicyDecision>(&payload)
            .await
        {
            Ok(decision) => {
                record.effective_action = Some(decision.action);
                record.effective_decision_source = decision.decision_source;
            }
            Err(err) => {
                warn!(
                    target = "svc-admin",
                    %err,
                    normalized_key = %record.normalized_key,
                    "failed to compute effective decision for classifications list"
                );
            }
        }
    }
}

pub async fn update(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
    Json(payload): Json<UpdateClassificationRequest>,
) -> Result<Json<ClassificationMutationRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let taxonomy_store = state.taxonomy_store();
    let sub_input = if payload.subcategory.trim().is_empty() {
        None
    } else {
        Some(payload.subcategory.as_str())
    };
    let validated = taxonomy_store.validate_labels(&payload.primary_category, sub_input);
    if let Some(reason) = validated.fallback_reason {
        warn!(
            target = "svc-admin",
            reason = reason.as_str(),
            actor = %user.actor,
            original_category = %payload.primary_category,
            original_subcategory = %payload.subcategory,
            canonical_category = %validated.category.id,
            canonical_subcategory = %validated.subcategory.id,
            "classification update normalized to canonical taxonomy"
        );
    }

    let (entity_level, hostname) = parse_normalized_key(&normalized_key).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "INVALID_NORMALIZED_KEY",
                "normalized_key must start with domain: or subdomain:",
            )),
        )
    })?;

    let decision_payload = PolicyDecisionRequestPayload {
        normalized_key: normalized_key.clone(),
        entity_level: entity_level.to_string(),
        source_ip: "127.0.0.1".to_string(),
        user_id: None,
        group_ids: None,
        category_hint: Some(validated.category.id.clone()),
        subcategory_hint: Some(validated.subcategory.id.clone()),
        risk_hint: None,
        confidence_hint: None,
    };

    let decision = state
        .evaluate_policy_decision::<_, PolicyDecision>(&decision_payload)
        .await
        .map_err(policy_error)?;

    let risk_level = decision
        .verdict
        .as_ref()
        .map(|v| v.risk_level.clone())
        .unwrap_or_else(|| "medium".to_string());
    let confidence = decision
        .verdict
        .as_ref()
        .map(|v| v.confidence)
        .unwrap_or(0.9);

    let taxonomy_version = taxonomy_store.taxonomy().version.clone();
    let flags = json!({
        "source": "classification-edit",
        "actor": user.actor,
        "reason": payload.reason,
        "hostname": hostname,
    });

    let row = sqlx::query(
        r#"INSERT INTO classifications (
               id, normalized_key, taxonomy_version, model_version, primary_category,
               subcategory, risk_level, recommended_action, confidence, sfw, flags,
               ttl_seconds, status, next_refresh_at
            ) VALUES ($1, $2, $3, 'manual', $4, $5, $6, $7, $8, false, $9, 3600, 'active', NOW() + INTERVAL '4 hours')
            ON CONFLICT (normalized_key)
            DO UPDATE SET
               primary_category = EXCLUDED.primary_category,
               subcategory = EXCLUDED.subcategory,
               risk_level = EXCLUDED.risk_level,
               recommended_action = EXCLUDED.recommended_action,
               confidence = EXCLUDED.confidence,
               flags = EXCLUDED.flags,
               updated_at = NOW(),
               ttl_seconds = EXCLUDED.ttl_seconds,
               next_refresh_at = NOW() + INTERVAL '4 hours'
            RETURNING id, primary_category, subcategory, risk_level, recommended_action, confidence::float8 AS confidence, updated_at"#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&normalized_key)
    .bind(&taxonomy_version)
    .bind(&validated.category.id)
    .bind(&validated.subcategory.id)
    .bind(&risk_level)
    .bind(decision.action.to_string())
    .bind(confidence as f64)
    .bind(flags)
    .fetch_one(state.pool())
    .await
    .map_err(db_error)?;

    let classification_id: uuid::Uuid = row.get("id");
    let next_version: i64 = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT MAX(version) FROM classification_versions WHERE classification_id = $1",
    )
    .bind(classification_id)
    .fetch_one(state.pool())
    .await
    .map_err(db_error)?
    .unwrap_or(0) as i64
        + 1;

    sqlx::query(
        "INSERT INTO classification_versions (id, classification_id, version, changed_by, reason, payload)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(classification_id)
    .bind(next_version)
    .bind(Some(user.actor.clone()))
    .bind(payload.reason.clone())
    .bind(json!({
        "normalized_key": normalized_key,
        "category": validated.category.id,
        "subcategory": validated.subcategory.id,
        "source": "classification-edit",
    }))
    .execute(state.pool())
    .await
    .map_err(db_error)?;

    let _ = sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
        .bind(&normalized_key)
        .execute(state.pool())
        .await;

    state.invalidate_cache_key(&normalized_key).await;

    Ok(Json(ClassificationMutationRecord {
        normalized_key,
        primary_category: row.get("primary_category"),
        subcategory: row.get("subcategory"),
        risk_level: row.get("risk_level"),
        recommended_action: parse_policy_action(
            row.get::<String, _>("recommended_action").as_str(),
        )
        .unwrap_or(PolicyAction::Monitor),
        confidence: row.get::<f64, _>("confidence") as f32,
        updated_at: row.get("updated_at"),
    }))
}

pub async fn delete(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    sqlx::query("DELETE FROM classifications WHERE normalized_key = $1")
        .bind(&normalized_key)
        .execute(state.pool())
        .await
        .map_err(db_error)?;
    let _ = sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
        .bind(&normalized_key)
        .execute(state.pool())
        .await;
    let _ = sqlx::query("DELETE FROM page_contents WHERE normalized_key = $1")
        .bind(&normalized_key)
        .execute(state.pool())
        .await;

    state.invalidate_cache_key(&normalized_key).await;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_policy_action(value: &str) -> Option<PolicyAction> {
    match value {
        "Allow" => Some(PolicyAction::Allow),
        "Block" => Some(PolicyAction::Block),
        "Warn" => Some(PolicyAction::Warn),
        "Monitor" => Some(PolicyAction::Monitor),
        "Review" => Some(PolicyAction::Review),
        "RequireApproval" => Some(PolicyAction::RequireApproval),
        "ContentPending" => Some(PolicyAction::ContentPending),
        _ => None,
    }
}

fn parse_normalized_key(normalized_key: &str) -> Option<(&'static str, String)> {
    if let Some(host) = normalized_key.strip_prefix("domain:") {
        let host = host.trim();
        if host.is_empty() {
            return None;
        }
        return Some(("domain", host.to_string()));
    }
    if let Some(host) = normalized_key.strip_prefix("subdomain:") {
        let host = host.trim();
        if host.is_empty() {
            return None;
        }
        return Some(("subdomain", host.to_string()));
    }
    None
}

fn policy_error(err: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "policy decision call failed");
    (
        StatusCode::BAD_GATEWAY,
        Json(ApiError::new(
            "POLICY_DECISION_FAILED",
            "failed to compute policy decision for classification update",
        )),
    )
}

fn db_error(err: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "classification query failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_policy_actions() {
        assert_eq!(parse_policy_action("Allow"), Some(PolicyAction::Allow));
        assert_eq!(parse_policy_action("Block"), Some(PolicyAction::Block));
        assert_eq!(parse_policy_action("Warn"), Some(PolicyAction::Warn));
        assert_eq!(
            parse_policy_action("ContentPending"),
            Some(PolicyAction::ContentPending)
        );
        assert_eq!(parse_policy_action("nope"), None);
    }

    #[test]
    fn parses_domain_and_subdomain_keys() {
        assert_eq!(
            parse_normalized_key("domain:example.com"),
            Some(("domain", "example.com".to_string()))
        );
        assert_eq!(
            parse_normalized_key("subdomain:www.example.com"),
            Some(("subdomain", "www.example.com".to_string()))
        );
        assert_eq!(parse_normalized_key("url:https://example.com"), None);
    }
}
