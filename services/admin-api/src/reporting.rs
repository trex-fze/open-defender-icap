use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use sqlx::Row;
use std::collections::HashMap;
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
const PROM_RANGE_STEP_SECONDS: u64 = 15;
const LLM_SERIES_SHORT_WINDOW_SECONDS: i64 = 60;

#[derive(Debug, Serialize)]
pub struct OpsSummaryResponse {
    pub range: String,
    pub source: String,
    pub queue: OpsQueueMetrics,
    pub auth: OpsAuthMetrics,
    pub providers: Vec<OpsProviderMetric>,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct OpsQueueMetrics {
    pub pending_age_p95_seconds: Option<f64>,
    pub llm_jobs_started_per_sec_10m: Option<f64>,
    pub llm_jobs_completed_per_sec_10m: Option<f64>,
    pub llm_dlq_growth_10m: Option<f64>,
    pub page_fetch_dlq_growth_10m: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
pub struct OpsAuthMetrics {
    pub login_failures_10m: Option<f64>,
    pub lockouts_10m: Option<f64>,
    pub refresh_failures_10m: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct OpsProviderMetric {
    pub provider: String,
    pub failures_5m: f64,
    pub timeouts_5m: f64,
    pub latency_p95_seconds: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct OpsSeriesPoint {
    pub ts_ms: i64,
    pub value: f64,
}

#[derive(Debug, Serialize)]
pub struct OpsLlmProviderSeries {
    pub provider: String,
    pub success: Vec<OpsSeriesPoint>,
    pub failures: Vec<OpsSeriesPoint>,
    pub timeouts: Vec<OpsSeriesPoint>,
    pub non_retryable_400: Vec<OpsSeriesPoint>,
}

#[derive(Debug, Serialize)]
pub struct OpsLlmSeriesResponse {
    pub range: String,
    pub source: String,
    pub step_seconds: u64,
    pub providers: Vec<OpsLlmProviderSeries>,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PrometheusResponse {
    status: String,
    data: PrometheusData,
}

#[derive(Debug, Deserialize)]
struct PrometheusData {
    result: Vec<PrometheusVectorSample>,
}

#[derive(Debug, Deserialize)]
struct PrometheusVectorSample {
    metric: HashMap<String, String>,
    value: (f64, String),
}

#[derive(Debug, Deserialize)]
struct PrometheusRangeResponse {
    status: String,
    data: PrometheusRangeData,
}

#[derive(Debug, Deserialize)]
struct PrometheusRangeData {
    result: Vec<PrometheusRangeSample>,
}

#[derive(Debug, Deserialize)]
struct PrometheusRangeSample {
    metric: HashMap<String, String>,
    values: Vec<(f64, String)>,
}

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

    let mut report = report;
    let event_categories = report.top_categories.clone();
    match hydrate_mapped_categories(&state, &report.top_domains, top_n).await {
        Ok((mapped_categories, mapped_domain_docs)) => {
            report.top_categories_event = event_categories;
            report.top_categories = mapped_categories;
            report.coverage.category_mapped_domain_docs = mapped_domain_docs;
            report.coverage.category_mapped_ratio = if report.coverage.total_docs > 0 {
                mapped_domain_docs as f64 / report.coverage.total_docs as f64
            } else {
                0.0
            };
        }
        Err(err) => {
            error!(target = "svc-admin", %err, "failed to map dashboard categories from classifications; using event categories");
            report.top_categories_event = Vec::new();
            report.coverage.category_mapped_domain_docs = 0;
            report.coverage.category_mapped_ratio = 0.0;
        }
    }

    Ok(Json(report))
}

pub async fn ops_summary(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<TrafficReportQuery>,
) -> Result<Json<OpsSummaryResponse>, StatusCode> {
    require_roles(&user, ROLE_REPORTING_VIEW)?;
    let range = query
        .range
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("24h")
        .to_string();

    let Some(prometheus_base) = state.prometheus_url() else {
        return Ok(Json(OpsSummaryResponse {
            range,
            source: "unavailable".to_string(),
            queue: OpsQueueMetrics::default(),
            auth: OpsAuthMetrics::default(),
            providers: Vec::new(),
            errors: vec!["prometheus endpoint is not configured".to_string()],
        }));
    };

    let mut errors = Vec::new();
    let mut queue = OpsQueueMetrics::default();
    let mut auth = OpsAuthMetrics::default();

    queue.pending_age_p95_seconds = query_prometheus_scalar(
        &state,
        prometheus_base,
        "histogram_quantile(0.95, sum(rate(llm_pending_age_seconds_bucket[10m])) by (le))",
        "pending_age_p95_seconds",
        &mut errors,
    )
    .await;
    queue.llm_jobs_started_per_sec_10m = query_prometheus_scalar(
        &state,
        prometheus_base,
        "rate(llm_jobs_started_total[10m])",
        "llm_jobs_started_per_sec_10m",
        &mut errors,
    )
    .await;
    queue.llm_jobs_completed_per_sec_10m = query_prometheus_scalar(
        &state,
        prometheus_base,
        "rate(llm_jobs_completed_total[10m])",
        "llm_jobs_completed_per_sec_10m",
        &mut errors,
    )
    .await;
    queue.llm_dlq_growth_10m = query_prometheus_scalar(
        &state,
        prometheus_base,
        "increase(llm_dlq_published_total[10m])",
        "llm_dlq_growth_10m",
        &mut errors,
    )
    .await;
    queue.page_fetch_dlq_growth_10m = query_prometheus_scalar(
        &state,
        prometheus_base,
        "increase(page_fetch_dlq_published_total[10m])",
        "page_fetch_dlq_growth_10m",
        &mut errors,
    )
    .await;

    auth.login_failures_10m = query_prometheus_scalar(
        &state,
        prometheus_base,
        "increase(admin_auth_login_failure_total[10m])",
        "login_failures_10m",
        &mut errors,
    )
    .await;
    auth.lockouts_10m = query_prometheus_scalar(
        &state,
        prometheus_base,
        "increase(admin_auth_lockout_total[10m])",
        "lockouts_10m",
        &mut errors,
    )
    .await;
    auth.refresh_failures_10m = query_prometheus_scalar(
        &state,
        prometheus_base,
        "increase(admin_auth_refresh_failure_total[10m])",
        "refresh_failures_10m",
        &mut errors,
    )
    .await;

    let failure_rows = query_prometheus_vector(
        &state,
        prometheus_base,
        "sum by (provider) (increase(llm_provider_failures_total[5m]))",
        "llm_provider_failures_5m",
        &mut errors,
    )
    .await
    .unwrap_or_default();
    let timeout_rows = query_prometheus_vector(
        &state,
        prometheus_base,
        "sum by (provider) (increase(llm_provider_timeouts_total[5m]))",
        "llm_provider_timeouts_5m",
        &mut errors,
    )
    .await
    .unwrap_or_default();
    let latency_rows = query_prometheus_vector(
        &state,
        prometheus_base,
        "histogram_quantile(0.95, sum(rate(llm_provider_request_duration_seconds_bucket[5m])) by (provider, le))",
        "llm_provider_latency_p95_seconds",
        &mut errors,
    )
    .await
    .unwrap_or_default();

    let mut providers_by_name: HashMap<String, OpsProviderMetric> = HashMap::new();
    for sample in &failure_rows {
        if let Some(name) = sample.metric.get("provider") {
            providers_by_name
                .entry(name.clone())
                .or_insert(OpsProviderMetric {
                    provider: name.clone(),
                    failures_5m: 0.0,
                    timeouts_5m: 0.0,
                    latency_p95_seconds: None,
                });
            if let Some(item) = providers_by_name.get_mut(name) {
                item.failures_5m = parse_sample_value(sample);
            }
        }
    }
    for sample in &timeout_rows {
        if let Some(name) = sample.metric.get("provider") {
            providers_by_name
                .entry(name.clone())
                .or_insert(OpsProviderMetric {
                    provider: name.clone(),
                    failures_5m: 0.0,
                    timeouts_5m: 0.0,
                    latency_p95_seconds: None,
                });
            if let Some(item) = providers_by_name.get_mut(name) {
                item.timeouts_5m = parse_sample_value(sample);
            }
        }
    }
    for sample in &latency_rows {
        if let Some(name) = sample.metric.get("provider") {
            providers_by_name
                .entry(name.clone())
                .or_insert(OpsProviderMetric {
                    provider: name.clone(),
                    failures_5m: 0.0,
                    timeouts_5m: 0.0,
                    latency_p95_seconds: None,
                });
            if let Some(item) = providers_by_name.get_mut(name) {
                item.latency_p95_seconds = Some(parse_sample_value(sample));
            }
        }
    }
    let mut providers = providers_by_name.into_values().collect::<Vec<_>>();
    providers.sort_by(|a, b| a.provider.cmp(&b.provider));

    let source = if errors.is_empty() {
        "live"
    } else if queue.pending_age_p95_seconds.is_some()
        || queue.llm_jobs_started_per_sec_10m.is_some()
        || queue.llm_jobs_completed_per_sec_10m.is_some()
        || auth.login_failures_10m.is_some()
        || !providers.is_empty()
    {
        "partial"
    } else {
        "unavailable"
    };

    Ok(Json(OpsSummaryResponse {
        range,
        source: source.to_string(),
        queue,
        auth,
        providers,
        errors,
    }))
}

pub async fn ops_llm_series(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<TrafficReportQuery>,
) -> Result<Json<OpsLlmSeriesResponse>, StatusCode> {
    require_roles(&user, ROLE_REPORTING_VIEW)?;
    let range = query
        .range
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("24h")
        .to_string();

    let Some(prometheus_base) = state.prometheus_url() else {
        return Ok(Json(OpsLlmSeriesResponse {
            range,
            source: "unavailable".to_string(),
            step_seconds: PROM_RANGE_STEP_SECONDS,
            providers: Vec::new(),
            errors: vec!["prometheus endpoint is not configured".to_string()],
        }));
    };

    let range_seconds = parse_prometheus_range_seconds(&range).unwrap_or(24 * 60 * 60);
    let end_ts = Utc::now().timestamp();
    let start_ts = end_ts - range_seconds;
    let lookback_window = llm_series_lookback_window(&range);

    let mut errors = Vec::new();
    let success_rows = query_prometheus_matrix(
        &state,
        prometheus_base,
        &format!(
            "sum by (provider) (increase(llm_provider_success_total[{}]))",
            lookback_window
        ),
        "llm_provider_success_series",
        start_ts,
        end_ts,
        PROM_RANGE_STEP_SECONDS,
        &mut errors,
    )
    .await
    .unwrap_or_default();
    let failure_rows = query_prometheus_matrix(
        &state,
        prometheus_base,
        &format!(
            "sum by (provider) (increase(llm_provider_failures_total[{}]))",
            lookback_window
        ),
        "llm_provider_failure_series",
        start_ts,
        end_ts,
        PROM_RANGE_STEP_SECONDS,
        &mut errors,
    )
    .await
    .unwrap_or_default();
    let timeout_rows = query_prometheus_matrix(
        &state,
        prometheus_base,
        &format!(
            "sum by (provider) (increase(llm_provider_timeouts_total[{}]))",
            lookback_window
        ),
        "llm_provider_timeout_series",
        start_ts,
        end_ts,
        PROM_RANGE_STEP_SECONDS,
        &mut errors,
    )
    .await
    .unwrap_or_default();
    let non_retryable_400_rows = query_prometheus_matrix(
        &state,
        prometheus_base,
        &format!(
            "sum by (provider) (increase(llm_provider_failure_class_total{{class=\"non_retryable\",status_code=\"400\"}}[{}]))",
            lookback_window
        ),
        "llm_provider_non_retryable_400_series",
        start_ts,
        end_ts,
        PROM_RANGE_STEP_SECONDS,
        &mut errors,
    )
    .await
    .unwrap_or_default();

    let mut providers_by_name: HashMap<String, OpsLlmProviderSeries> = HashMap::new();
    merge_provider_series(&mut providers_by_name, success_rows, |row| &mut row.success);
    merge_provider_series(&mut providers_by_name, failure_rows, |row| {
        &mut row.failures
    });
    merge_provider_series(&mut providers_by_name, timeout_rows, |row| {
        &mut row.timeouts
    });
    merge_provider_series(&mut providers_by_name, non_retryable_400_rows, |row| {
        &mut row.non_retryable_400
    });

    let mut providers = providers_by_name.into_values().collect::<Vec<_>>();
    providers.sort_by(|a, b| a.provider.cmp(&b.provider));

    let source = if errors.is_empty() {
        "live"
    } else if !providers.is_empty() {
        "partial"
    } else {
        "unavailable"
    };

    Ok(Json(OpsLlmSeriesResponse {
        range,
        source: source.to_string(),
        step_seconds: PROM_RANGE_STEP_SECONDS,
        providers,
        errors,
    }))
}

fn parse_sample_value(sample: &PrometheusVectorSample) -> f64 {
    sample.value.1.parse::<f64>().unwrap_or(0.0)
}

async fn query_prometheus_scalar(
    state: &AppState,
    base: &str,
    query: &str,
    metric_name: &str,
    errors: &mut Vec<String>,
) -> Option<f64> {
    match query_prometheus_vector(state, base, query, metric_name, errors).await {
        Some(rows) => rows.first().map(parse_sample_value),
        None => None,
    }
}

async fn query_prometheus_vector(
    state: &AppState,
    base: &str,
    query: &str,
    metric_name: &str,
    errors: &mut Vec<String>,
) -> Option<Vec<PrometheusVectorSample>> {
    let url = format!("{}/api/v1/query", base.trim_end_matches('/'));
    let response = state
        .http_client
        .get(&url)
        .query(&[("query", query)])
        .send()
        .await;
    let response = match response {
        Ok(resp) => resp,
        Err(err) => {
            errors.push(format!("{} query failed: {}", metric_name, err));
            return None;
        }
    };

    if !response.status().is_success() {
        errors.push(format!(
            "{} query returned {}",
            metric_name,
            response.status()
        ));
        return None;
    }

    let payload = response.json::<PrometheusResponse>().await;
    match payload {
        Ok(body) => {
            if body.status != "success" {
                errors.push(format!(
                    "{} query response status was {}",
                    metric_name, body.status
                ));
                None
            } else {
                Some(body.data.result)
            }
        }
        Err(err) => {
            errors.push(format!("{} query decode failed: {}", metric_name, err));
            None
        }
    }
}

async fn query_prometheus_matrix(
    state: &AppState,
    base: &str,
    query: &str,
    metric_name: &str,
    start_ts: i64,
    end_ts: i64,
    step_seconds: u64,
    errors: &mut Vec<String>,
) -> Option<Vec<PrometheusRangeSample>> {
    let url = format!("{}/api/v1/query_range", base.trim_end_matches('/'));
    let response = state
        .http_client
        .get(&url)
        .query(&[
            ("query", query.to_string()),
            ("start", start_ts.to_string()),
            ("end", end_ts.to_string()),
            ("step", format!("{}s", step_seconds)),
        ])
        .send()
        .await;
    let response = match response {
        Ok(resp) => resp,
        Err(err) => {
            errors.push(format!("{} query failed: {}", metric_name, err));
            return None;
        }
    };

    if !response.status().is_success() {
        errors.push(format!(
            "{} query returned {}",
            metric_name,
            response.status()
        ));
        return None;
    }

    match response.json::<PrometheusRangeResponse>().await {
        Ok(body) => {
            if body.status != "success" {
                errors.push(format!(
                    "{} query response status was {}",
                    metric_name, body.status
                ));
                None
            } else {
                Some(body.data.result)
            }
        }
        Err(err) => {
            errors.push(format!("{} query decode failed: {}", metric_name, err));
            None
        }
    }
}

fn parse_prometheus_range_seconds(range: &str) -> Option<i64> {
    let trimmed = range.trim();
    if trimmed.len() < 2 {
        return None;
    }
    let unit = trimmed.chars().last()?.to_ascii_lowercase();
    let value: i64 = trimmed[..trimmed.len() - 1].parse().ok()?;
    if value <= 0 {
        return None;
    }
    match unit {
        'm' => Some(value * 60),
        'h' => Some(value * 60 * 60),
        'd' => Some(value * 24 * 60 * 60),
        _ => None,
    }
}

fn llm_series_lookback_window(range: &str) -> &'static str {
    match parse_prometheus_range_seconds(range) {
        Some(seconds) if seconds <= LLM_SERIES_SHORT_WINDOW_SECONDS => "1m",
        _ => "5m",
    }
}

fn parse_matrix_points(values: Vec<(f64, String)>) -> Vec<OpsSeriesPoint> {
    values
        .into_iter()
        .map(|(ts_secs, value)| OpsSeriesPoint {
            ts_ms: (ts_secs * 1000.0) as i64,
            value: value.parse::<f64>().unwrap_or(0.0),
        })
        .collect()
}

fn merge_provider_series<F>(
    providers_by_name: &mut HashMap<String, OpsLlmProviderSeries>,
    rows: Vec<PrometheusRangeSample>,
    target: F,
) where
    F: Fn(&mut OpsLlmProviderSeries) -> &mut Vec<OpsSeriesPoint>,
{
    for sample in rows {
        let Some(name) = sample.metric.get("provider").cloned() else {
            continue;
        };
        let item = providers_by_name
            .entry(name.clone())
            .or_insert_with(|| OpsLlmProviderSeries {
                provider: name,
                success: Vec::new(),
                failures: Vec::new(),
                timeouts: Vec::new(),
                non_retryable_400: Vec::new(),
            });
        *target(item) = parse_matrix_points(sample.values);
    }
}

async fn hydrate_mapped_categories(
    state: &AppState,
    top_domains: &[crate::reporting_es::TopEntry],
    top_n: u32,
) -> Result<(Vec<crate::reporting_es::TopEntry>, i64), sqlx::Error> {
    if top_domains.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let canonical_by_domain = top_domains
        .iter()
        .map(|entry| {
            let host = entry.key.trim().to_ascii_lowercase();
            let raw_key = format!("domain:{}", host);
            let canonical = state.canonicalize_key(&raw_key, None);
            (entry.key.clone(), canonical)
        })
        .collect::<HashMap<_, _>>();

    let mut canonical_keys = canonical_by_domain.values().cloned().collect::<Vec<_>>();
    canonical_keys.sort();
    canonical_keys.dedup();

    let rows = sqlx::query(
        r#"SELECT normalized_key, primary_category
           FROM classifications
           WHERE status = 'active'
             AND normalized_key = ANY($1::text[])"#,
    )
    .bind(&canonical_keys)
    .fetch_all(state.pool())
    .await?;

    let category_by_key = rows
        .into_iter()
        .filter_map(|row| {
            let key: String = row.get("normalized_key");
            let category: Option<String> = row.get("primary_category");
            category.map(|value| (key, value))
        })
        .collect::<HashMap<_, _>>();

    Ok(build_mapped_categories(
        top_domains,
        &canonical_by_domain,
        &category_by_key,
        top_n,
    ))
}

fn build_mapped_categories(
    top_domains: &[crate::reporting_es::TopEntry],
    canonical_by_domain: &HashMap<String, String>,
    category_by_key: &HashMap<String, String>,
    top_n: u32,
) -> (Vec<crate::reporting_es::TopEntry>, i64) {
    let mut by_category: HashMap<String, i64> = HashMap::new();
    let mut mapped_domain_docs = 0_i64;

    for domain in top_domains {
        let canonical_key = canonical_by_domain
            .get(&domain.key)
            .cloned()
            .unwrap_or_else(|| format!("domain:{}", domain.key.trim().to_ascii_lowercase()));
        let category = if let Some(found) = category_by_key.get(&canonical_key) {
            mapped_domain_docs += domain.doc_count;
            found.clone()
        } else {
            "unknown-unclassified".to_string()
        };
        *by_category.entry(category).or_insert(0) += domain.doc_count;
    }

    let mut top_categories = by_category
        .into_iter()
        .map(|(key, doc_count)| crate::reporting_es::TopEntry { key, doc_count })
        .collect::<Vec<_>>();
    top_categories.sort_by(|a, b| {
        b.doc_count
            .cmp(&a.doc_count)
            .then_with(|| a.key.cmp(&b.key))
    });
    top_categories.truncate(top_n.clamp(1, 50) as usize);

    (top_categories, mapped_domain_docs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_categories_aggregate_domain_counts() {
        let top_domains = vec![
            crate::reporting_es::TopEntry {
                key: "news.example.com".to_string(),
                doc_count: 40,
            },
            crate::reporting_es::TopEntry {
                key: "ads.example.com".to_string(),
                doc_count: 10,
            },
            crate::reporting_es::TopEntry {
                key: "unknown.test".to_string(),
                doc_count: 5,
            },
        ];

        let canonical_by_domain = HashMap::from([
            (
                "news.example.com".to_string(),
                "domain:example.com".to_string(),
            ),
            (
                "ads.example.com".to_string(),
                "domain:example.com".to_string(),
            ),
            (
                "unknown.test".to_string(),
                "domain:unknown.test".to_string(),
            ),
        ]);
        let category_by_key =
            HashMap::from([("domain:example.com".to_string(), "news-media".to_string())]);

        let (categories, mapped_docs) =
            build_mapped_categories(&top_domains, &canonical_by_domain, &category_by_key, 20);

        assert_eq!(mapped_docs, 50);
        assert_eq!(categories[0].key, "news-media");
        assert_eq!(categories[0].doc_count, 50);
        assert_eq!(categories[1].key, "unknown-unclassified");
        assert_eq!(categories[1].doc_count, 5);
    }

    #[test]
    fn parses_prometheus_range_seconds() {
        assert_eq!(parse_prometheus_range_seconds("1m"), Some(60));
        assert_eq!(parse_prometheus_range_seconds("15m"), Some(900));
        assert_eq!(parse_prometheus_range_seconds("1h"), Some(3600));
        assert_eq!(parse_prometheus_range_seconds("7d"), Some(604800));
        assert_eq!(parse_prometheus_range_seconds("0m"), None);
        assert_eq!(parse_prometheus_range_seconds("abc"), None);
    }

    #[test]
    fn matrix_points_convert_to_epoch_millis() {
        let points = parse_matrix_points(vec![(1712707200.0, "3.5".to_string())]);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].ts_ms, 1712707200000);
        assert!((points[0].value - 3.5).abs() < f64::EPSILON);
    }

    #[test]
    fn llm_series_lookback_uses_short_window_for_1m() {
        assert_eq!(llm_series_lookback_window("1m"), "1m");
    }

    #[test]
    fn llm_series_lookback_defaults_to_5m_for_longer_or_invalid_ranges() {
        assert_eq!(llm_series_lookback_window("5m"), "5m");
        assert_eq!(llm_series_lookback_window("15m"), "5m");
        assert_eq!(llm_series_lookback_window("1h"), "5m");
        assert_eq!(llm_series_lookback_window("24h"), "5m");
        assert_eq!(llm_series_lookback_window("bogus"), "5m");
    }
}
