use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use tracing::error;

use crate::{
    auth::{require_roles, UserContext, ROLE_CACHE_ADMIN, ROLE_POLICY_VIEW},
    ApiError, AppState,
};

#[derive(Debug, Serialize)]
pub struct CacheEntryRecord {
    pub cache_key: String,
    pub value: Value,
    pub expires_at: DateTime<Utc>,
    pub source: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn get_entry(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(cache_key): Path<String>,
) -> Result<Json<CacheEntryRecord>, StatusCode> {
    require_roles(&user, ROLE_POLICY_VIEW)?;
    let row = sqlx::query(
        "SELECT cache_key, value_json, expires_at, source, created_at FROM cache_entries WHERE cache_key = $1",
    )
    .bind(&cache_key)
    .fetch_optional(state.pool())
    .await
    .map_err(map_db_error)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(CacheEntryRecord {
        cache_key: row.get("cache_key"),
        value: row.get("value_json"),
        expires_at: row.get("expires_at"),
        source: row.get("source"),
        created_at: row.get("created_at"),
    }))
}

pub async fn delete_entry(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(cache_key): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_CACHE_ADMIN)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let result = sqlx::query("DELETE FROM cache_entries WHERE cache_key = $1")
        .bind(&cache_key)
        .execute(state.pool())
        .await
        .map_err(map_db_error_api)?;
    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "cache entry not found")),
        ));
    }
    state.invalidate_cache_key(&cache_key).await;
    Ok(StatusCode::NO_CONTENT)
}

fn map_db_error(err: sqlx::Error) -> StatusCode {
    error!(target = "svc-admin", %err, "cache entry lookup failed");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn map_db_error_api(err: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "cache entry mutation failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    )
}
