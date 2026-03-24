use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_EDIT, ROLE_POLICY_VIEW},
    ApiError, AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common_types::PolicyAction;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use tracing::error;

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

    let record =
        persist_manual_classification(state.pool(), &normalized_key, &payload, &user.actor)
            .await
            .map_err(db_error)?;

    if let Err(err) = sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
        .bind(&normalized_key)
        .execute(state.pool())
        .await
    {
        error!(target = "svc-admin", %err, key = %normalized_key, "failed to clear classification request");
    }

    state.invalidate_cache_key(&normalized_key).await;
    Ok(Json(record))
}

async fn persist_manual_classification(
    pool: &PgPool,
    normalized_key: &str,
    payload: &ManualUnblockRequest,
    actor: &str,
) -> Result<ManualClassificationRecord, sqlx::Error> {
    use sqlx::postgres::PgRow;
    let ttl_seconds = 3600;
    let flags = json!({
        "source": "manual-unblock",
        "actor": actor,
        "reason": payload.reason,
    });

    let row: PgRow = sqlx::query(
        r#"
        INSERT INTO classifications (
            id, normalized_key, taxonomy_version, model_version, primary_category,
            subcategory, risk_level, recommended_action, confidence, sfw, flags,
            ttl_seconds, status, next_refresh_at
        ) VALUES ($1, $2, 'manual', 'manual', $3, $4, $5, $6, $7, false, $8, $9, 'active', NOW() + INTERVAL '4 hours')
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
        RETURNING id, primary_category, subcategory, risk_level, recommended_action, confidence, updated_at
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(normalized_key)
    .bind(&payload.primary_category)
    .bind(&payload.subcategory)
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
        "category": payload.primary_category,
        "action": payload.action,
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
