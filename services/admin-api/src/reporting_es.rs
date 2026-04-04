use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;

#[derive(Debug, Clone, Deserialize)]
pub struct ReportingConfig {
    pub elastic_url: Option<String>,
    #[serde(default = "default_index_pattern")]
    pub index_pattern: String,
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default = "default_range")]
    pub default_range: String,
}

impl Default for ReportingConfig {
    fn default() -> Self {
        Self {
            elastic_url: None,
            index_pattern: default_index_pattern(),
            api_key: None,
            username: None,
            password: None,
            default_range: default_range(),
        }
    }
}

impl ReportingConfig {
    pub fn merge_env(mut self) -> Self {
        if let Ok(url) = env::var("OD_REPORTING_ELASTIC_URL") {
            self.elastic_url = Some(url);
        }
        if let Ok(pattern) = env::var("OD_REPORTING_INDEX_PATTERN") {
            self.index_pattern = pattern;
        }
        if let Ok(key) = env::var("OD_REPORTING_ELASTIC_API_KEY") {
            self.api_key = Some(key);
        }
        if let Ok(user) = env::var("OD_REPORTING_ELASTIC_USERNAME") {
            self.username = Some(user);
        }
        if let Ok(pass) = env::var("OD_REPORTING_ELASTIC_PASSWORD") {
            self.password = Some(pass);
        }
        if let Ok(range) = env::var("OD_REPORTING_DEFAULT_RANGE") {
            if !range.trim().is_empty() {
                self.default_range = range.trim().to_string();
            }
        }
        self
    }
}

fn default_index_pattern() -> String {
    "traffic-events-*".into()
}

fn default_range() -> String {
    "24h".into()
}

#[derive(Clone)]
pub struct ElasticReportingClient {
    client: Client,
    base_url: String,
    index_pattern: String,
    auth: ElasticAuth,
    default_range: String,
}

#[derive(Clone)]
enum ElasticAuth {
    None,
    ApiKey(String),
    Basic { username: String, password: String },
}

impl ElasticReportingClient {
    pub fn from_config(cfg: &ReportingConfig) -> Result<Option<Self>> {
        let url = match cfg.elastic_url.as_ref() {
            Some(url) if !url.trim().is_empty() => url.trim_end_matches('/').to_string(),
            _ => return Ok(None),
        };
        let client = Client::builder().build()?;
        let auth = if let Some(key) = cfg.api_key.clone().filter(|v| !v.is_empty()) {
            ElasticAuth::ApiKey(key)
        } else if let (Some(user), Some(pass)) = (cfg.username.clone(), cfg.password.clone()) {
            ElasticAuth::Basic {
                username: user,
                password: pass,
            }
        } else {
            ElasticAuth::None
        };
        Ok(Some(Self {
            client,
            base_url: url,
            index_pattern: cfg.index_pattern.clone(),
            auth,
            default_range: cfg.default_range.clone(),
        }))
    }

    pub async fn traffic_report(
        &self,
        range: Option<&str>,
        top_n: u32,
        bucket: Option<&str>,
    ) -> Result<TrafficReportResponse> {
        let range = range
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.default_range);
        let bucket_interval = bucket
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| select_interval(range));
        let size = top_n.clamp(1, 50);
        let body = json!({
            "size": 0,
            "track_total_hits": true,
            "runtime_mappings": {
                "effective_action": {
                    "type": "keyword",
                    "script": {
                        "source": "if (doc.containsKey('recommended_action.keyword') && !doc['recommended_action.keyword'].empty) { emit(doc['recommended_action.keyword'].value); } else if (doc.containsKey('recommended_action_inferred.keyword') && !doc['recommended_action_inferred.keyword'].empty) { emit(doc['recommended_action_inferred.keyword'].value); } else if (doc.containsKey('http.response.status_code') && !doc['http.response.status_code'].empty && doc['http.response.status_code'].value >= 400) { emit('block'); } else { emit('allow'); }"
                    }
                },
                "effective_domain": {
                    "type": "keyword",
                    "script": {
                        "source": "if (doc.containsKey('destination.domain.keyword') && !doc['destination.domain.keyword'].empty) { emit(doc['destination.domain.keyword'].value); } else if (doc.containsKey('url.full.keyword') && !doc['url.full.keyword'].empty) { String u = doc['url.full.keyword'].value; int start = u.indexOf('://'); int from = start >= 0 ? start + 3 : 0; int end = u.indexOf('/', from); String host = end > from ? u.substring(from, end) : u.substring(from); int colon = host.indexOf(':'); if (colon > 0) { host = host.substring(0, colon); } if (host.length() > 0) emit(host); }"
                    }
                },
                "effective_category": {
                    "type": "keyword",
                    "script": {
                        "source": "if (doc.containsKey('category.keyword') && !doc['category.keyword'].empty) { emit(doc['category.keyword'].value); } else { emit('unknown-unclassified'); }"
                    }
                }
            },
            "query": {
                "range": {
                    "@timestamp": {
                        "gte": format!("now-{}", range),
                        "lte": "now"
                    }
                }
            },
            "aggs": {
                "actions": {
                    "terms": { "field": "effective_action", "size": 5 },
                    "aggs": {
                        "per_interval": {
                            "date_histogram": {
                                "field": "@timestamp",
                                "fixed_interval": bucket_interval.as_str(),
                                "min_doc_count": 0
                            }
                        }
                    }
                },
                "top_blocked": {
                    "filter": { "term": { "effective_action": "block" } },
                    "aggs": {
                        "urls": { "terms": { "field": "effective_domain", "size": size } }
                    }
                },
                "top_categories": {
                    "terms": { "field": "effective_category", "size": size }
                }
            }
        });

        let url = format!("{}/{}/_search", self.base_url, self.index_pattern);
        let mut req = self.client.post(&url).json(&body);
        req = self.attach_auth(req);
        let response = req.send().await?.error_for_status()?;
        let payload: Value = response.json().await?;
        let aggregations = payload
            .get("aggregations")
            .ok_or_else(|| anyhow!("missing aggregations"))?;

        let allow_block_trend = parse_trend(aggregations);
        let top_blocked_domains = parse_top_entries(aggregations, &["top_blocked", "urls"]);
        let top_categories = parse_top_entries(aggregations, &["top_categories"]);

        Ok(TrafficReportResponse {
            range: range.to_string(),
            bucket_interval,
            allow_block_trend,
            top_blocked_domains,
            top_categories,
        })
    }

    pub async fn coverage_status(&self, range: Option<&str>) -> Result<ReportingCoverageStatus> {
        let range = range
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.default_range);
        let body = json!({
            "size": 0,
            "track_total_hits": true,
            "query": {
                "range": {
                    "@timestamp": {
                        "gte": format!("now-{}", range),
                        "lte": "now"
                    }
                }
            },
            "aggs": {
                "has_action": {
                    "filter": {
                        "bool": {
                            "should": [
                                { "exists": { "field": "recommended_action" } },
                                { "exists": { "field": "recommended_action_inferred" } }
                            ],
                            "minimum_should_match": 1
                        }
                    }
                },
                "has_category": { "filter": { "exists": { "field": "category" } } },
                "has_domain": {
                    "filter": {
                        "bool": {
                            "should": [
                                { "exists": { "field": "destination.domain" } },
                                { "exists": { "field": "url.full" } }
                            ],
                            "minimum_should_match": 1
                        }
                    }
                }
            }
        });

        let url = format!("/{}/_search", self.index_pattern);
        let full_url = format!("{}{}", self.base_url, url);
        let mut req = self.client.post(&full_url).json(&body);
        req = self.attach_auth(req);
        let response = req.send().await?.error_for_status()?;
        let payload: Value = response.json().await?;

        let total = payload
            .pointer("/hits/total/value")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let action_docs = payload
            .pointer("/aggregations/has_action/doc_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let category_docs = payload
            .pointer("/aggregations/has_category/doc_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let domain_docs = payload
            .pointer("/aggregations/has_domain/doc_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);

        Ok(ReportingCoverageStatus {
            range: range.to_string(),
            total_docs: total,
            action_docs,
            category_docs,
            domain_docs,
        })
    }

    fn attach_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            ElasticAuth::None => req,
            ElasticAuth::ApiKey(key) => req.header("Authorization", format!("ApiKey {}", key)),
            ElasticAuth::Basic { username, password } => req.basic_auth(username, Some(password)),
        }
    }
}

fn select_interval(range: &str) -> String {
    let hours = parse_range_hours(range).unwrap_or(24.0);
    if hours <= 6.0 {
        "5m".into()
    } else if hours <= 24.0 {
        "1h".into()
    } else if hours <= 24.0 * 7.0 {
        "3h".into()
    } else if hours <= 24.0 * 30.0 {
        "12h".into()
    } else {
        "1d".into()
    }
}

fn parse_range_hours(input: &str) -> Option<f64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let last = trimmed.chars().last()?;
    let value = &trimmed[..trimmed.len() - 1];
    let number: f64 = value.parse().ok()?;
    match last {
        'h' | 'H' => Some(number),
        'd' | 'D' => Some(number * 24.0),
        'm' | 'M' => Some(number / 60.0),
        _ => None,
    }
}

fn parse_trend(aggregations: &Value) -> Vec<ActionSeries> {
    aggregations
        .get("actions")
        .and_then(|a| a.get("buckets"))
        .and_then(Value::as_array)
        .map(|buckets| {
            buckets
                .iter()
                .map(|bucket| {
                    let action = bucket
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string();
                    let series = bucket
                        .get("per_interval")
                        .and_then(|pi| pi.get("buckets"))
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .map(|entry| TimeBucket {
                                    key_as_string: entry
                                        .get("key_as_string")
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                    doc_count: entry
                                        .get("doc_count")
                                        .and_then(Value::as_i64)
                                        .unwrap_or(0),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    ActionSeries {
                        action,
                        buckets: series,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_top_entries(aggregations: &Value, path: &[&str]) -> Vec<TopEntry> {
    let mut current = aggregations;
    for segment in path {
        if let Some(next) = current.get(segment) {
            current = next;
        } else {
            return Vec::new();
        }
    }
    current
        .get("buckets")
        .and_then(Value::as_array)
        .map(|buckets| {
            buckets
                .iter()
                .map(|bucket| TopEntry {
                    key: bucket
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or("-unknown-")
                        .to_string(),
                    doc_count: bucket.get("doc_count").and_then(Value::as_i64).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Serialize)]
pub struct TrafficReportResponse {
    pub range: String,
    pub bucket_interval: String,
    pub allow_block_trend: Vec<ActionSeries>,
    pub top_blocked_domains: Vec<TopEntry>,
    pub top_categories: Vec<TopEntry>,
}

#[derive(Debug, Serialize)]
pub struct ReportingCoverageStatus {
    pub range: String,
    pub total_docs: i64,
    pub action_docs: i64,
    pub category_docs: i64,
    pub domain_docs: i64,
}

#[derive(Debug, Serialize)]
pub struct ActionSeries {
    pub action: String,
    pub buckets: Vec<TimeBucket>,
}

#[derive(Debug, Serialize)]
pub struct TimeBucket {
    pub key_as_string: String,
    pub doc_count: i64,
}

#[derive(Debug, Serialize)]
pub struct TopEntry {
    pub key: String,
    pub doc_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_selection() {
        assert_eq!(select_interval("6h"), "5m");
        assert_eq!(select_interval("12h"), "1h");
        assert_eq!(select_interval("48h"), "3h");
        assert_eq!(select_interval("10d"), "12h");
        assert_eq!(select_interval("90d"), "1d");
    }
}
