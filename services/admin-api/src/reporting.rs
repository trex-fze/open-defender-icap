use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use tracing::error;

use crate::{
    auth::{require_roles, UserContext, ROLE_REPORTING_VIEW},
    reporting_es::{DashboardReportResponse, ReportingCoverageStatus, TrafficReportResponse},
    AppState,
};

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

pub async fn reporting_status(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<TrafficReportQuery>,
) -> Result<Json<ReportingCoverageStatus>, StatusCode> {
    require_roles(&user, ROLE_REPORTING_VIEW)?;
    let client = state
        .reporting_client()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;
    let range = query.range.as_deref();
    let status = client.coverage_status(range).await.map_err(|err| {
        error!(target = "svc-admin", %err, "failed to fetch reporting coverage status");
        StatusCode::BAD_GATEWAY
    })?;
    Ok(Json(status))
}

pub async fn dashboard_summary(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<TrafficReportQuery>,
) -> Result<Json<DashboardReportResponse>, StatusCode> {
    require_roles(&user, ROLE_REPORTING_VIEW)?;
    let client = state
        .reporting_client()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;
    let range = query.range.as_deref();
    let bucket = query.bucket.as_deref();
    let top_n = query.top_n.unwrap_or(DEFAULT_TOP_N).max(1);
    let report = client
        .dashboard_report(range, top_n, bucket)
        .await
        .map_err(|err| {
            error!(target = "svc-admin", %err, "failed to fetch dashboard report from elastic");
            StatusCode::BAD_GATEWAY
        })?;
    Ok(Json(report))
}
