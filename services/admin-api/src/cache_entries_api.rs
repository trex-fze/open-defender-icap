use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use std::collections::HashSet;
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

    let candidates = cache_key_candidates(&cache_key);

    if let Some(cache) = state.cache_invalidator() {
        for candidate in &candidates {
            match cache.inspect_key(candidate).await {
                Ok(Some((value, expires_at))) => {
                    let now = Utc::now();
                    return Ok(Json(CacheEntryRecord {
                        cache_key: candidate.clone(),
                        value,
                        expires_at: expires_at.unwrap_or(now),
                        source: Some("redis".to_string()),
                        created_at: now,
                    }));
                }
                Ok(None) => {}
                Err(err) => {
                    error!(target = "svc-admin", %err, candidate, "redis cache lookup failed; falling back to cache_entries table");
                }
            }
        }
    }

    for candidate in &candidates {
        let row = sqlx::query(
            "SELECT cache_key, value_json, expires_at, source, created_at FROM cache_entries WHERE cache_key = $1",
        )
        .bind(candidate)
        .fetch_optional(state.pool())
        .await
        .map_err(map_db_error)?;

        if let Some(row) = row {
            return Ok(Json(CacheEntryRecord {
                cache_key: row.get("cache_key"),
                value: row.get("value_json"),
                expires_at: row.get("expires_at"),
                source: row.get("source"),
                created_at: row.get("created_at"),
            }));
        }
    }

    Err(StatusCode::NOT_FOUND)
}

fn cache_key_candidates(raw: &str) -> Vec<String> {
    let key = raw.trim().to_ascii_lowercase();
    if key.is_empty() {
        return Vec::new();
    }

    let mut ordered = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |candidate: String| {
        if !candidate.is_empty() && seen.insert(candidate.clone()) {
            ordered.push(candidate);
        }
    };

    push(key.clone());

    if let Some(host) = key.strip_prefix("domain:") {
        push(format!("subdomain:www.{}", host));
        push(format!("subdomain:{}", host));
        return ordered;
    }

    if let Some(host) = key.strip_prefix("subdomain:") {
        if let Some(rest) = host.strip_prefix("www.") {
            push(format!("domain:{}", rest));
        } else {
            push(format!("domain:{}", host));
            push(format!("subdomain:www.{}", host));
        }
        return ordered;
    }

    if key.contains('.') {
        push(format!("subdomain:{}", key));
        push(format!("subdomain:www.{}", key));
        push(format!("domain:{}", key));
    }

    ordered
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
