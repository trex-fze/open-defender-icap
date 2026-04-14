use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_EDIT, ROLE_POLICY_VIEW},
    pagination::{
        cursor_limit, decode_cursor_with_direction, encode_directional_cursor, CursorDirection,
        CursorPaged,
    },
    ApiError, AppState,
};
use ::taxonomy::FallbackReason;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common_types::{PolicyAction, PolicyDecision};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use tracing::{error, warn};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct PendingQuery {
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PendingCursor {
    updated_at: DateTime<Utc>,
    normalized_key: String,
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

#[derive(Debug, Deserialize)]
pub struct MetadataClassifyRequest {
    pub provider_name: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MetadataClassifyResponse {
    pub normalized_key: String,
    pub status: String,
    pub provider_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct ClassificationJobPayload {
    normalized_key: String,
    entity_level: String,
    hostname: String,
    full_url: String,
    trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    idempotency_key: Option<String>,
    requires_content: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_language: Option<String>,
    #[serde(default)]
    requeue_attempt: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_override: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClearAllPendingResponse {
    pub deleted: usize,
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
) -> Result<Json<CursorPaged<PendingClassificationRecord>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let limit = cursor_limit(query.limit);
    let cursor = query
        .cursor
        .as_deref()
        .map(decode_cursor_with_direction::<PendingCursor>)
        .transpose()
        .map_err(|message| {
            (
                StatusCode::BAD_REQUEST,
                Json(ApiError::new("INVALID_CURSOR", message)),
            )
        })?;

    let (cursor_direction, cursor_anchor) = cursor
        .as_ref()
        .map(|(direction, anchor)| (*direction, Some(anchor)))
        .unwrap_or((CursorDirection::Next, None));

    let cursor_updated_at = cursor_anchor.map(|c| c.updated_at);
    let cursor_key = cursor
        .as_ref()
        .map(|(_, c)| c.normalized_key.clone())
        .unwrap_or_default();

    let rows = if cursor_direction == CursorDirection::Prev {
        sqlx::query(
            r#"SELECT normalized_key, status, base_url, last_error, requested_at, updated_at
           FROM classification_requests
           WHERE ($1::text IS NULL OR status = $1)
             AND ($2::timestamptz IS NULL OR (updated_at, normalized_key) > ($2, $3))
           ORDER BY updated_at ASC, normalized_key ASC
           LIMIT $4"#,
        )
        .bind(query.status.as_deref())
        .bind(cursor_updated_at)
        .bind(&cursor_key)
        .bind((limit + 1) as i64)
        .fetch_all(state.pool())
        .await
        .map_err(db_error)?
    } else {
        sqlx::query(
            r#"SELECT normalized_key, status, base_url, last_error, requested_at, updated_at
           FROM classification_requests
           WHERE ($1::text IS NULL OR status = $1)
             AND ($2::timestamptz IS NULL OR (updated_at, normalized_key) < ($2, $3))
           ORDER BY updated_at DESC, normalized_key DESC
           LIMIT $4"#,
        )
        .bind(query.status.as_deref())
        .bind(cursor_updated_at)
        .bind(&cursor_key)
        .bind((limit + 1) as i64)
        .fetch_all(state.pool())
        .await
        .map_err(db_error)?
    };

    let mut records: Vec<PendingClassificationRecord> = rows
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
    let has_more = records.len() > limit as usize;
    if has_more {
        records.truncate(limit as usize);
    }
    if cursor_direction == CursorDirection::Prev {
        records.reverse();
    }
    let next_cursor = if has_more {
        records.last().and_then(|last| {
            encode_directional_cursor(
                CursorDirection::Next,
                &PendingCursor {
                    updated_at: last.updated_at,
                    normalized_key: last.normalized_key.clone(),
                },
            )
            .ok()
        })
    } else {
        None
    };
    let prev_cursor = if query.cursor.is_some() && !records.is_empty() {
        records.first().and_then(|first| {
            encode_directional_cursor(
                CursorDirection::Prev,
                &PendingCursor {
                    updated_at: first.updated_at,
                    normalized_key: first.normalized_key.clone(),
                },
            )
            .ok()
        })
    } else {
        None
    };
    Ok(Json(CursorPaged::new_with_prev(
        records,
        limit,
        has_more,
        next_cursor,
        prev_cursor,
    )))
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

    let applied_key = state.canonicalize_key(&normalized_key, None);

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

    let applied_key = state.canonicalize_key(&normalized_key, None);
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

    let applied_key = state.canonicalize_key(&normalized_key, None);

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

pub async fn metadata_classify(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
    Json(payload): Json<MetadataClassifyRequest>,
) -> Result<Json<MetadataClassifyResponse>, (StatusCode, Json<ApiError>)> {
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

    let applied_key = state.canonicalize_key(&normalized_key, None);
    let (entity_level, hostname) = parse_normalized_key(&applied_key).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "INVALID_NORMALIZED_KEY",
                "normalized_key must start with domain: or subdomain:",
            )),
        )
    })?;

    let provider_name = normalize_provider_name(payload.provider_name.as_deref());

    if let Some(provider) = provider_name.as_deref() {
        let provider_valid = state
            .validate_llm_provider_name(provider)
            .await
            .map_err(|err| {
                error!(target = "svc-admin", %err, provider, "failed to validate llm provider");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(ApiError::new(
                        "LLM_PROVIDER_LOOKUP_FAILED",
                        "failed to validate selected llm provider",
                    )),
                )
            })?;

        if !provider_valid {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError::new(
                    "INVALID_LLM_PROVIDER",
                    "selected llm provider is not available",
                )),
            ));
        }
    }

    let pending = sqlx::query(
        r#"SELECT base_url
           FROM classification_requests
           WHERE normalized_key = $1"#,
    )
    .bind(&applied_key)
    .fetch_optional(state.pool())
    .await
    .map_err(db_error)?;

    let Some(pending_row) = pending else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new(
                "PENDING_NOT_FOUND",
                "pending classification record was not found",
            )),
        ));
    };

    let base_url: Option<String> = pending_row.get("base_url");
    let job = build_metadata_classification_job(
        applied_key.clone(),
        entity_level,
        &hostname,
        base_url,
        provider_name.clone(),
    );

    state.queue_classification_job(&job).await.map_err(|err| {
        error!(
            target = "svc-admin",
            %err,
            key = %applied_key,
            "failed to publish metadata-only classification job"
        );
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                "CLASSIFICATION_QUEUE_UNAVAILABLE",
                "failed to enqueue metadata-only classification job",
            )),
        )
    })?;

    let status = "queued_manual_metadata";
    sqlx::query(
        r#"UPDATE classification_requests
           SET status = $2,
               last_error = NULL,
               updated_at = NOW()
           WHERE normalized_key = $1"#,
    )
    .bind(&applied_key)
    .bind(status)
    .execute(state.pool())
    .await
    .map_err(db_error)?;

    state
        .log_policy_event(
            "pending.metadata_classify",
            Some(user.actor),
            Some(applied_key.clone()),
            json!({
                "provider_name": provider_name,
                "reason": payload.reason,
            }),
        )
        .await;

    Ok(Json(MetadataClassifyResponse {
        normalized_key: applied_key,
        status: status.to_string(),
        provider_name: job.provider_override,
    }))
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

    let applied_key = state.canonicalize_key(&normalized_key, None);

    sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
        .bind(&applied_key)
        .execute(state.pool())
        .await
        .map_err(db_error)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_all_pending(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
) -> Result<Json<ClearAllPendingResponse>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_EDIT)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let deleted = sqlx::query("DELETE FROM classification_requests")
        .execute(state.pool())
        .await
        .map_err(db_error)?
        .rows_affected() as usize;

    state
        .log_policy_event(
            "pending.clear_all",
            Some(user.actor),
            None,
            json!({
                "deleted": deleted,
            }),
        )
        .await;

    Ok(Json(ClearAllPendingResponse { deleted }))
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

fn normalize_provider_name(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(ToOwned::to_owned)
}

fn metadata_job_idempotency_key(normalized_key: &str, provider_name: Option<&str>) -> String {
    format!(
        "manual-meta:{}:{}",
        normalized_key,
        provider_name.unwrap_or("default")
    )
}

fn build_metadata_classification_job(
    normalized_key: String,
    entity_level: &str,
    hostname: &str,
    base_url: Option<String>,
    provider_name: Option<String>,
) -> ClassificationJobPayload {
    let full_url = base_url
        .clone()
        .unwrap_or_else(|| format!("https://{}", hostname));
    let trace_id = format!("manual-meta-{}", Uuid::new_v4());
    let idempotency_key = metadata_job_idempotency_key(&normalized_key, provider_name.as_deref());

    ClassificationJobPayload {
        normalized_key,
        entity_level: entity_level.to_string(),
        hostname: hostname.to_string(),
        full_url,
        trace_id,
        idempotency_key: Some(idempotency_key),
        requires_content: false,
        base_url,
        content_excerpt: None,
        content_hash: None,
        content_version: None,
        content_language: None,
        requeue_attempt: 0,
        provider_override: provider_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_provider_name_trims_and_omits_empty() {
        assert_eq!(
            normalize_provider_name(Some(" openai-fallback ")),
            Some("openai-fallback".to_string())
        );
        assert_eq!(normalize_provider_name(Some("   ")), None);
        assert_eq!(normalize_provider_name(None), None);
    }

    #[test]
    fn metadata_idempotency_key_defaults_when_provider_missing() {
        assert_eq!(
            metadata_job_idempotency_key("domain:example.com", None),
            "manual-meta:domain:example.com:default"
        );
        assert_eq!(
            metadata_job_idempotency_key("domain:example.com", Some("local-lmstudio")),
            "manual-meta:domain:example.com:local-lmstudio"
        );
    }

    #[test]
    fn build_metadata_job_uses_base_url_and_provider_override() {
        let job = build_metadata_classification_job(
            "domain:example.com".to_string(),
            "domain",
            "example.com",
            Some("https://portal.example.com".to_string()),
            Some("openai-fallback".to_string()),
        );

        assert_eq!(job.normalized_key, "domain:example.com");
        assert_eq!(job.entity_level, "domain");
        assert_eq!(job.hostname, "example.com");
        assert_eq!(job.full_url, "https://portal.example.com");
        assert!(!job.requires_content);
        assert_eq!(job.provider_override.as_deref(), Some("openai-fallback"));
        assert_eq!(
            job.idempotency_key.as_deref(),
            Some("manual-meta:domain:example.com:openai-fallback")
        );
        assert!(job.trace_id.starts_with("manual-meta-"));
    }
}
