use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, Row};
use tracing::error;

use crate::{
    auth::{require_roles, UserContext, ROLE_POLICY_VIEW},
    ApiError, AppState,
};

const DEFAULT_MAX_EXCERPT: usize = 1_200;
const MAX_ALLOWED_EXCERPT: usize = 8_000;
const MIN_EXCERPT: usize = 64;
const DEFAULT_HISTORY_LIMIT: i64 = 5;
const MAX_HISTORY_LIMIT: i64 = 50;

#[derive(Debug, Deserialize)]
pub struct PageContentQuery {
    pub version: Option<i64>,
    pub max_excerpt: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct PageContentHistoryQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PageContentRecord {
    pub normalized_key: String,
    pub fetch_version: i64,
    pub content_type: Option<String>,
    pub content_hash: Option<String>,
    pub char_count: Option<i32>,
    pub byte_count: Option<i32>,
    pub fetch_status: String,
    pub fetch_reason: Option<String>,
    pub ttl_seconds: i32,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub excerpt: Option<String>,
    pub excerpt_truncated: bool,
    pub excerpt_format: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PageContentSummary {
    pub fetch_version: i64,
    pub fetch_status: String,
    pub fetch_reason: Option<String>,
    pub ttl_seconds: i32,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub char_count: Option<i32>,
    pub byte_count: Option<i32>,
    pub content_hash: Option<String>,
}

pub async fn get_page_content(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
    Query(params): Query<PageContentQuery>,
) -> Result<Json<PageContentRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let excerpt_limit = clamp_excerpt(params.max_excerpt);
    let row = if let Some(version) = params.version {
        sqlx::query(
            r#"SELECT normalized_key, fetch_version::bigint AS fetch_version, content_type, content_hash, char_count,
                byte_count, fetch_status, fetch_reason, ttl_seconds, fetched_at, expires_at,
                text_excerpt
            FROM page_contents
            WHERE normalized_key = $1 AND fetch_version = $2
            LIMIT 1"#,
        )
        .bind(&normalized_key)
        .bind(version)
        .fetch_optional(state.pool())
        .await
    } else {
        sqlx::query(
            r#"SELECT normalized_key, fetch_version::bigint AS fetch_version, content_type, content_hash, char_count,
                byte_count, fetch_status, fetch_reason, ttl_seconds, fetched_at, expires_at,
                text_excerpt
            FROM page_contents
            WHERE normalized_key = $1
            ORDER BY fetch_version DESC
            LIMIT 1"#,
        )
        .bind(&normalized_key)
        .fetch_optional(state.pool())
        .await
    }
    .map_err(map_db_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError::new(
                "NOT_FOUND",
                "no page content available for key",
            )),
        )
    })?;

    let record = map_content_row(row, excerpt_limit).map_err(map_db_error)?;
    Ok(Json(record))
}

pub async fn list_page_content_history(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(normalized_key): Path<String>,
    Query(params): Query<PageContentHistoryQuery>,
) -> Result<Json<Vec<PageContentSummary>>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_POLICY_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let limit = clamp_history_limit(params.limit);
    let rows = sqlx::query(
        r#"SELECT fetch_version::bigint AS fetch_version, fetch_status, fetch_reason, ttl_seconds,
                fetched_at, expires_at, char_count, byte_count, content_hash
            FROM page_contents
            WHERE normalized_key = $1
            ORDER BY fetch_version DESC
            LIMIT $2"#,
    )
    .bind(&normalized_key)
    .bind(limit)
    .fetch_all(state.pool())
    .await
    .map_err(map_db_error)?;

    let history = rows
        .into_iter()
        .map(map_summary_row)
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_error)?;

    Ok(Json(history))
}

fn map_content_row(row: PgRow, excerpt_limit: usize) -> Result<PageContentRecord, sqlx::Error> {
    let text_excerpt: Option<String> = row.try_get("text_excerpt")?;
    let (excerpt, excerpt_truncated) = text_excerpt
        .as_deref()
        .and_then(|text| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trim_excerpt(trimmed, excerpt_limit))
            }
        })
        .map(|(value, truncated)| (Some(value), truncated))
        .unwrap_or((None, false));

    let excerpt_format = if excerpt.is_some() {
        Some("markdown".to_string())
    } else {
        None
    };

    Ok(PageContentRecord {
        normalized_key: row.try_get("normalized_key")?,
        fetch_version: row.try_get("fetch_version")?,
        content_type: row.try_get::<Option<String>, _>("content_type")?,
        content_hash: row.try_get::<Option<String>, _>("content_hash")?,
        char_count: row.try_get::<Option<i32>, _>("char_count")?,
        byte_count: row.try_get::<Option<i32>, _>("byte_count")?,
        fetch_status: row.try_get("fetch_status")?,
        fetch_reason: row.try_get::<Option<String>, _>("fetch_reason")?,
        ttl_seconds: row.try_get("ttl_seconds")?,
        fetched_at: row.try_get("fetched_at")?,
        expires_at: row.try_get("expires_at")?,
        excerpt,
        excerpt_truncated,
        excerpt_format,
    })
}

fn map_summary_row(row: PgRow) -> Result<PageContentSummary, sqlx::Error> {
    Ok(PageContentSummary {
        fetch_version: row.try_get("fetch_version")?,
        fetch_status: row.try_get("fetch_status")?,
        fetch_reason: row.try_get::<Option<String>, _>("fetch_reason")?,
        ttl_seconds: row.try_get("ttl_seconds")?,
        fetched_at: row.try_get("fetched_at")?,
        expires_at: row.try_get("expires_at")?,
        char_count: row.try_get::<Option<i32>, _>("char_count")?,
        byte_count: row.try_get::<Option<i32>, _>("byte_count")?,
        content_hash: row.try_get::<Option<String>, _>("content_hash")?,
    })
}

fn trim_excerpt(text: &str, limit: usize) -> (String, bool) {
    let mut buffer = String::new();
    for ch in text.chars().take(limit) {
        buffer.push(ch);
    }
    let truncated = text.chars().count() > buffer.chars().count();
    if truncated {
        buffer.push('…');
    }
    (buffer, truncated)
}

fn clamp_excerpt(limit: Option<usize>) -> usize {
    limit
        .map(|value| value.clamp(MIN_EXCERPT, MAX_ALLOWED_EXCERPT))
        .unwrap_or(DEFAULT_MAX_EXCERPT)
}

fn clamp_history_limit(limit: Option<i64>) -> i64 {
    limit
        .map(|value| value.clamp(1, MAX_HISTORY_LIMIT))
        .unwrap_or(DEFAULT_HISTORY_LIMIT)
}

fn map_db_error(err: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(target = "svc-admin", %err, "page content query failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    )
}
