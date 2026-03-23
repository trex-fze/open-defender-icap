use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use tracing::error;
use uuid::Uuid;

use crate::{
    auth::{require_roles, UserContext, ROLE_REPORTING_VIEW},
    pagination::{PageOptions, Paged},
    reporting_es::TrafficReportResponse,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct ReportingQuery {
    pub dimension: String,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ReportingAggregate {
    pub id: Uuid,
    pub dimension: String,
    pub period_start: DateTime<Utc>,
    pub metrics: Value,
    pub created_at: DateTime<Utc>,
}

pub async fn list_aggregates(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<ReportingQuery>,
) -> Result<Json<Paged<ReportingAggregate>>, StatusCode> {
    require_roles(&user, ROLE_REPORTING_VIEW)?;
    let opts = PageOptions::new(query.page, query.page_size);
    let dimension = query.dimension.trim();
    if dimension.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reporting_aggregates WHERE dimension = $1")
            .bind(dimension)
            .fetch_one(state.pool())
            .await
            .map_err(map_db_error)?;
    if total == 0 {
        return Ok(Json(Paged::new(Vec::new(), 0, opts)));
    }
    let rows = sqlx::query(
        "SELECT id, dimension, period_start, metrics, created_at
            FROM reporting_aggregates
            WHERE dimension = $1
            ORDER BY period_start DESC
            LIMIT $2 OFFSET $3",
    )
    .bind(dimension)
    .bind(opts.page_size as i64)
    .bind(opts.offset())
    .fetch_all(state.pool())
    .await
    .map_err(map_db_error)?;

    let data = rows
        .into_iter()
        .map(|row| ReportingAggregate {
            id: row.get("id"),
            dimension: row.get("dimension"),
            period_start: row.get("period_start"),
            metrics: row.get("metrics"),
            created_at: row.get("created_at"),
        })
        .collect();
    Ok(Json(Paged::new(data, total, opts)))
}

#[derive(Debug, Deserialize)]
pub struct TrafficReportQuery {
    pub range: Option<String>,
    pub top_n: Option<u32>,
    pub bucket: Option<String>,
}

const DEFAULT_TOP_N: u32 = 10;

pub async fn traffic_summary(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<TrafficReportQuery>,
) -> Result<Json<TrafficReportResponse>, StatusCode> {
    require_roles(&user, ROLE_REPORTING_VIEW)?;
    let client = state
        .reporting_client()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;
    let range = query.range.as_deref();
    let bucket = query.bucket.as_deref();
    let top_n = query.top_n.unwrap_or(DEFAULT_TOP_N).max(1);
    let report = client
        .traffic_report(range, top_n, bucket)
        .await
        .map_err(|err| {
            error!(target = "svc-admin", %err, "failed to fetch traffic report from elastic");
            StatusCode::BAD_GATEWAY
        })?;
    Ok(Json(report))
}

fn map_db_error(err: sqlx::Error) -> StatusCode {
    error!(target = "svc-admin", %err, "reporting query failed");
    StatusCode::INTERNAL_SERVER_ERROR
}
