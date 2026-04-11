use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use common_types::normalizer::canonical_classification_key;
use serde::Deserialize;
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
            let canonical = canonical_classification_key(&raw_key).unwrap_or(raw_key);
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
    top_categories.sort_by(|a, b| b.doc_count.cmp(&a.doc_count).then_with(|| a.key.cmp(&b.key)));
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
        let category_by_key = HashMap::from([(
            "domain:example.com".to_string(),
            "news-media".to_string(),
        )]);

        let (categories, mapped_docs) =
            build_mapped_categories(&top_domains, &canonical_by_domain, &category_by_key, 20);

        assert_eq!(mapped_docs, 50);
        assert_eq!(categories[0].key, "news-media");
        assert_eq!(categories[0].doc_count, 50);
        assert_eq!(categories[1].key, "unknown-unclassified");
        assert_eq!(categories[1].doc_count, 5);
    }
}
