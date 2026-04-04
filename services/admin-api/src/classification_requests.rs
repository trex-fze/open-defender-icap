use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_EDIT, ROLE_POLICY_VIEW},
    ApiError, AppState,
};
use ::taxonomy::FallbackReason;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common_types::{normalizer::canonical_classification_key, PolicyAction, PolicyDecision};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use tracing::{error, warn};

#[derive(Debug, Deserialize)]
pub struct PendingQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PendingClassificationRecord {
    pub normalized_key: String,
    pub status: String,
    pub base_url: Option<String>,
    pub last_error: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ManualUnblockRequest {
    pub action: PolicyAction,
    pub primary_category: String,
    pub subcategory: String,
    pub risk_level: String,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ManualClassificationRecord {
    pub normalized_key: String,
    pub primary_category: String,
    pub subcategory: String,
    pub risk_level: String,
    pub recommended_action: PolicyAction,
    pub confidence: f32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ManualClassifyRequest {
    pub primary_category: String,
    pub subcategory: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpsertPendingRequest {
    pub status: Option<String>,
    pub base_url: Option<String>,
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

const fn default_confidence() -> f32 {
    0.9
}

pub async fn list_pending(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<PendingQuery>,
) -> Result<Json<Vec<PendingClassificationRecord>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let mut sql = String::from(
        "SELECT normalized_key, status, base_url, last_error, requested_at, updated_at \
         FROM classification_requests",
    );
    if query.status.is_some() {
        sql.push_str(" WHERE status = $1 ORDER BY updated_at DESC LIMIT $2");
    } else {
        sql.push_str(" ORDER BY updated_at DESC LIMIT $1");
    }

    let rows = if let Some(status) = &query.status {
        sqlx::query(&sql)
            .bind(status)
            .bind(limit)
            .fetch_all(state.pool())
            .await
    } else {
        sqlx::query(&sql).bind(limit).fetch_all(state.pool()).await
    };

    let rows = rows.map_err(db_error)?;
    let records = rows
        .into_iter()
        .map(|row| PendingClassificationRecord {
            normalized_key: row.get("normalized_key"),
            status: row.get("status"),
            base_url: row.get("base_url"),
            last_error: row.get("last_error"),
            requested_at: row.get("requested_at"),
            updated_at: row.get("updated_at"),
        })
        .collect();
    Ok(Json(records))
}

pub async fn manual_unblock(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
    Json(payload): Json<ManualUnblockRequest>,
) -> Result<Json<ManualClassificationRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    if matches!(payload.action, PolicyAction::ContentPending) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "INVALID_ACTION",
                "ContentPending is not a valid manual action",
            )),
        ));
    }

    let applied_key =
        canonical_classification_key(&normalized_key).unwrap_or_else(|| normalized_key.clone());

    let taxonomy_store = state.taxonomy_store();
    let sub_input = if payload.subcategory.trim().is_empty() {
        None
    } else {
        Some(payload.subcategory.as_str())
    };
    let validated = taxonomy_store.validate_labels(&payload.primary_category, sub_input);
    let fallback_reason = validated.fallback_reason;
    if let Some(reason) = fallback_reason {
        warn!(
            target = "svc-admin",
            reason = reason.as_str(),
            actor = %user.actor,
            original_category = %payload.primary_category,
            original_subcategory = %payload.subcategory,
            canonical_category = %validated.category.id,
            canonical_subcategory = %validated.subcategory.id,
            "manual classification normalized to canonical taxonomy"
        );
    }

    let taxonomy_version = taxonomy_store.taxonomy().version.clone();

    let record = persist_manual_classification(
        state.pool(),
        &applied_key,
        &payload,
        &user.actor,
        &validated.category.id,
        &validated.subcategory.id,
        &taxonomy_version,
        fallback_reason,
        "manual-unblock",
        None,
    )
    .await
    .map_err(db_error)?;

    if let Err(err) = sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
        .bind(&applied_key)
        .execute(state.pool())
        .await
    {
        error!(target = "svc-admin", %err, key = %applied_key, "failed to clear classification request");
    }

    state.invalidate_cache_key(&applied_key).await;
    if applied_key != normalized_key {
        state.invalidate_cache_key(&normalized_key).await;
    }
    Ok(Json(record))
}

pub async fn manual_classify(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
    Json(payload): Json<ManualClassifyRequest>,
) -> Result<Json<ManualClassificationRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let taxonomy_store = state.taxonomy_store();
    let sub_input = if payload.subcategory.trim().is_empty() {
        None
    } else {
        Some(payload.subcategory.as_str())
    };
    let validated = taxonomy_store.validate_labels(&payload.primary_category, sub_input);
    let fallback_reason = validated.fallback_reason;
    if let Some(reason) = fallback_reason {
        warn!(
            target = "svc-admin",
            reason = reason.as_str(),
            actor = %user.actor,
            original_category = %payload.primary_category,
            original_subcategory = %payload.subcategory,
            canonical_category = %validated.category.id,
            canonical_subcategory = %validated.subcategory.id,
            "manual classification normalized to canonical taxonomy"
        );
    }

    let (_, _) = parse_normalized_key(&normalized_key).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "INVALID_NORMALIZED_KEY",
                "normalized_key must start with domain: or subdomain:",
            )),
        )
    })?;

    let applied_key = canonical_classification_key(&normalized_key).unwrap_or_else(|| normalized_key.clone());
    let (entity_level, hostname) = parse_normalized_key(&applied_key).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "INVALID_NORMALIZED_KEY",
                "normalized_key must start with domain: or subdomain:",
            )),
        )
    })?;

    let decision_payload = PolicyDecisionRequestPayload {
        normalized_key: applied_key.clone(),
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
        .map_err(|err| {
            error!(target = "svc-admin", %err, key = %normalized_key, "policy decision failed for manual classify");
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError::new(
                    "POLICY_DECISION_FAILED",
                    "failed to compute policy decision for manual classification",
                )),
            )
        })?;

    let derived_risk = decision
        .verdict
        .as_ref()
        .map(|v| v.risk_level.clone())
        .unwrap_or_else(|| "medium".to_string());
    let derived_confidence = decision
        .verdict
        .as_ref()
        .map(|v| v.confidence)
        .unwrap_or(default_confidence());

    let manual_payload = ManualUnblockRequest {
        action: decision.action.clone(),
        primary_category: validated.category.id.clone(),
        subcategory: validated.subcategory.id.clone(),
        risk_level: derived_risk,
        confidence: derived_confidence,
        reason: payload.reason.clone(),
    };

    let taxonomy_version = taxonomy_store.taxonomy().version.clone();
    let record = persist_manual_classification(
        state.pool(),
        &applied_key,
        &manual_payload,
        &user.actor,
        &validated.category.id,
        &validated.subcategory.id,
        &taxonomy_version,
        fallback_reason,
        "manual-classify",
        Some(&hostname),
    )
    .await
    .map_err(db_error)?;

    if let Err(err) = sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
        .bind(&applied_key)
        .execute(state.pool())
        .await
    {
        error!(target = "svc-admin", %err, key = %applied_key, "failed to clear classification request");
    }

    state.invalidate_cache_key(&applied_key).await;
    if applied_key != normalized_key {
        state.invalidate_cache_key(&normalized_key).await;
    }
    Ok(Json(record))
}

pub async fn upsert_pending(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
    Json(payload): Json<UpsertPendingRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    if parse_normalized_key(&normalized_key).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "INVALID_NORMALIZED_KEY",
                "normalized_key must start with domain: or subdomain:",
            )),
        ));
    }

    let status = payload
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("waiting_content");

    let applied_key = canonical_classification_key(&normalized_key).unwrap_or_else(|| normalized_key.clone());

    sqlx::query(
        r#"
        INSERT INTO classification_requests (normalized_key, status, base_url, last_error)
        VALUES ($1, $2, $3, NULL)
        ON CONFLICT (normalized_key)
        DO UPDATE SET
            status = EXCLUDED.status,
            base_url = COALESCE(EXCLUDED.base_url, classification_requests.base_url),
            last_error = NULL,
            updated_at = NOW()
        "#,
    )
    .bind(&applied_key)
    .bind(status)
    .bind(payload.base_url)
    .execute(state.pool())
    .await
    .map_err(db_error)?;

    Ok(StatusCode::ACCEPTED)
}

pub async fn clear_pending(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    if parse_normalized_key(&normalized_key).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "INVALID_NORMALIZED_KEY",
                "normalized_key must start with domain: or subdomain:",
            )),
        ));
    }

    let applied_key = canonical_classification_key(&normalized_key).unwrap_or_else(|| normalized_key.clone());

    sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
        .bind(&applied_key)
        .execute(state.pool())
        .await
        .map_err(db_error)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn persist_manual_classification(
    pool: &PgPool,
    normalized_key: &str,
    payload: &ManualUnblockRequest,
    actor: &str,
    canonical_category: &str,
    canonical_subcategory: &str,
    taxonomy_version: &str,
    fallback_reason: Option<FallbackReason>,
    source: &str,
    hostname_hint: Option<&str>,
) -> Result<ManualClassificationRecord, sqlx::Error> {
    use sqlx::postgres::PgRow;
    let ttl_seconds = 3600;
    let mut flags = json!({
        "source": source,
        "actor": actor,
        "reason": payload.reason,
    });
    if let Some(hostname) = hostname_hint {
        if let Some(obj) = flags.as_object_mut() {
            obj.insert("hostname".into(), json!(hostname));
        }
    }
    if let Some(reason) = fallback_reason {
        if let Some(obj) = flags.as_object_mut() {
            obj.insert("taxonomy_fallback_reason".into(), json!(reason.as_str()));
        }
    }

    let row: PgRow = sqlx::query(
        r#"
        INSERT INTO classifications (
            id, normalized_key, taxonomy_version, model_version, primary_category,
            subcategory, risk_level, recommended_action, confidence, sfw, flags,
            ttl_seconds, status, next_refresh_at
        ) VALUES ($1, $2, $3, 'manual', $4, $5, $6, $7, $8, false, $9, $10, 'active', NOW() + INTERVAL '4 hours')
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
        RETURNING id, primary_category, subcategory, risk_level, recommended_action, confidence::float8 AS confidence, updated_at
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(normalized_key)
    .bind(taxonomy_version)
    .bind(canonical_category)
    .bind(canonical_subcategory)
    .bind(&payload.risk_level)
    .bind(payload.action.to_string())
    .bind(payload.confidence as f64)
    .bind(flags)
    .bind(ttl_seconds)
    .fetch_one(pool)
    .await?;

    let classification_id: uuid::Uuid = row.get("id");
    let version: i64 = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT MAX(version) FROM classification_versions WHERE classification_id = $1",
    )
    .bind(classification_id)
    .fetch_one(pool)
    .await?
    .unwrap_or(0) as i64
        + 1;

    sqlx::query(
        "INSERT INTO classification_versions (id, classification_id, version, changed_by, reason, payload)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(classification_id)
    .bind(version)
    .bind(Some(actor.to_string()))
    .bind(payload.reason.clone())
    .bind(json!({
        "normalized_key": normalized_key,
        "category": canonical_category,
        "action": payload.action,
        "source": source,
    }))
    .execute(pool)
    .await?;

    Ok(ManualClassificationRecord {
        normalized_key: normalized_key.to_string(),
        primary_category: row.get("primary_category"),
        subcategory: row.get("subcategory"),
        risk_level: row.get("risk_level"),
        recommended_action: payload.action.clone(),
        confidence: row.get::<f64, _>("confidence") as f32,
        updated_at: row.get("updated_at"),
    })
}

fn db_error(err: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "classification request query failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    )
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
