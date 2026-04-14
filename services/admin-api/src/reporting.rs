use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
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
}
