use std::env;
use std::time::{Duration, Instant};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{require_roles, UserContext, ROLE_REPORTING_VIEW},
    metrics, AppState,
};

const DEFAULT_TTL_SECS: u64 = 10;
const DEFAULT_TIMEOUT_MS: u64 = 2_000;

#[derive(Debug, Clone)]
enum ElasticsearchProbeAuth {
    None,
    ApiKey(String),
    Basic { username: String, password: String },
}

impl ElasticsearchProbeAuth {
    fn from_env() -> Self {
        if let Ok(api_key) = env::var("OD_REPORTING_ELASTIC_API_KEY") {
            let trimmed = api_key.trim();
            if !trimmed.is_empty() {
                return Self::ApiKey(trimmed.to_string());
            }
        }

        let username = env::var("OD_REPORTING_ELASTIC_USERNAME")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let password = env::var("OD_REPORTING_ELASTIC_PASSWORD")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        match (username, password) {
            (Some(username), Some(password)) => Self::Basic { username, password },
            _ => Self::None,
        }
    }

    fn has_credentials(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone)]
pub struct OpsHealthConfig {
    pub enabled: bool,
    pub ttl: Duration,
    pub timeout: Duration,
}

impl OpsHealthConfig {
    pub fn from_env() -> Self {
        let enabled = env::var("OD_OPS_HEALTH_ENABLED")
            .ok()
            .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(true);
        let ttl_secs = env::var("OD_OPS_HEALTH_TTL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_TTL_SECS);
        let timeout_ms = env::var("OD_OPS_HEALTH_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        Self {
            enabled,
            ttl: Duration::from_secs(ttl_secs),
            timeout: Duration::from_millis(timeout_ms),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CachedPlatformHealth {
    pub generated_at: Instant,
    pub snapshot: PlatformHealthResponse,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Healthy,
    Degraded,
    Unreachable,
    Misconfigured,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformHealthComponent {
    pub name: String,
    pub category: String,
    pub status: HealthState,
    pub checked_at_ms: u64,
    pub latency_ms: u64,
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformHealthSummary {
    pub total: usize,
    pub healthy: usize,
    pub degraded: usize,
    pub unreachable: usize,
    pub misconfigured: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformHealthResponse {
    pub source: String,
    pub checked_at_ms: u64,
    pub overall_status: HealthState,
    pub summary: PlatformHealthSummary,
    pub components: Vec<PlatformHealthComponent>,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlatformHealthQuery {
    pub refresh: Option<bool>,
}

pub async fn platform_health(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<PlatformHealthQuery>,
) -> Result<Json<PlatformHealthResponse>, StatusCode> {
    require_roles(&user, ROLE_REPORTING_VIEW)?;

    let cfg = state.ops_health_config().clone();
    if !cfg.enabled {
        return Ok(Json(PlatformHealthResponse {
            source: "disabled".to_string(),
            checked_at_ms: unix_timestamp_ms(),
            overall_status: HealthState::Unknown,
            summary: PlatformHealthSummary {
                total: 0,
                healthy: 0,
                degraded: 0,
                unreachable: 0,
                misconfigured: 0,
                unknown: 0,
            },
            components: Vec::new(),
            errors: vec!["ops platform health is disabled by OD_OPS_HEALTH_ENABLED".to_string()],
        }));
    }

    let cache = state.ops_health_cache();
    let refresh = query.refresh.unwrap_or(false);
    if !refresh {
        if let Some(cached) = cache.read().await.as_ref() {
            if cached.generated_at.elapsed() < cfg.ttl {
                metrics::record_ops_health_cache_hit();
                let mut snapshot = cached.snapshot.clone();
                snapshot.source = "cached".to_string();
                return Ok(Json(snapshot));
            }
        }
    }

    let snapshot = collect_platform_health(&state, &cfg).await;
    *cache.write().await = Some(CachedPlatformHealth {
        generated_at: Instant::now(),
        snapshot: snapshot.clone(),
    });
    Ok(Json(snapshot))
}

async fn collect_platform_health(
    state: &AppState,
    cfg: &OpsHealthConfig,
) -> PlatformHealthResponse {
    let mut components = Vec::new();
    let mut errors = Vec::new();

    components.push(local_component(
        "admin-api",
        "control_plane",
        HealthState::Healthy,
        "self process running",
    ));

    components.push(
        probe_http(
            state,
            cfg,
            "policy-engine",
            "control_plane",
            &format!(
                "{}/health/ready",
                state.policy_engine_url().trim_end_matches('/')
            ),
        )
        .await,
    );
    components.push(
        probe_http(
            state,
            cfg,
            "event-ingester",
            "pipeline",
            &env_or_default(
                "OD_EVENT_INGESTER_URL",
                "http://event-ingester:19100/health/ready",
            ),
        )
        .await,
    );
    components.push(
        probe_http(
            state,
            cfg,
            "llm-worker",
            "workers",
            state.llm_providers_url(),
        )
        .await,
    );
    components.push(
        probe_http(
            state,
            cfg,
            "page-fetcher",
            "workers",
            &env_or_default(
                "OD_PAGE_FETCH_METRICS_URL",
                "http://page-fetcher:19025/metrics",
            ),
        )
        .await,
    );
    components.push(
        probe_http(
            state,
            cfg,
            "reclass-worker",
            "workers",
            &env_or_default(
                "OD_RECLASS_METRICS_URL",
                "http://reclass-worker:19016/metrics",
            ),
        )
        .await,
    );
    components.push(
        probe_http(
            state,
            cfg,
            "icap-adaptor",
            "ingress",
            &env_or_default("OD_ICAP_METRICS_URL", "http://icap-adaptor:19005/metrics"),
        )
        .await,
    );
    components.push(
        probe_http(
            state,
            cfg,
            "crawl4ai",
            "workers",
            &env_or_default("OD_CRAWL4AI_HEALTH_URL", "http://crawl4ai:8085/healthz"),
        )
        .await,
    );

    components.push(probe_postgres(state, cfg).await);
    components.push(probe_redis(state, cfg).await);

    components.push(probe_elasticsearch(state, cfg).await);
    components.push(
        probe_http(
            state,
            cfg,
            "kibana",
            "observability",
            &env_or_default("OD_KIBANA_STATUS_URL", "http://kibana:5601/status"),
        )
        .await,
    );

    if let Some(prometheus_base) = state.prometheus_url() {
        components.push(
            probe_http(
                state,
                cfg,
                "prometheus",
                "observability",
                &format!("{}/-/ready", prometheus_base.trim_end_matches('/')),
            )
            .await,
        );
    } else {
        components.push(PlatformHealthComponent {
            name: "prometheus".to_string(),
            category: "observability".to_string(),
            status: HealthState::Misconfigured,
            checked_at_ms: unix_timestamp_ms(),
            latency_ms: 0,
            endpoint: None,
            http_status: None,
            detail: Some("OD_PROMETHEUS_URL is not configured".to_string()),
            source: "active_probe".to_string(),
        });
    }

    for component in &components {
        metrics::record_ops_health_probe(&component.name, health_state_label(&component.status));
        metrics::observe_ops_health_probe_duration(
            &component.name,
            component.latency_ms as f64 / 1000.0,
        );
        if !matches!(component.status, HealthState::Healthy) {
            errors.push(format!(
                "{} reported {:?}: {}",
                component.name,
                component.status,
                component
                    .detail
                    .clone()
                    .unwrap_or_else(|| "no details".to_string())
            ));
        }
    }

    let summary = build_summary(&components);
    let overall_status = aggregate_status(&components);
    PlatformHealthResponse {
        source: "live".to_string(),
        checked_at_ms: unix_timestamp_ms(),
        overall_status,
        summary,
        components,
        errors,
    }
}

async fn probe_elasticsearch(state: &AppState, cfg: &OpsHealthConfig) -> PlatformHealthComponent {
    let checked_at_ms = unix_timestamp_ms();
    let start = Instant::now();
    let elastic_url = env_or_default("OD_REPORTING_ELASTIC_URL", "http://elasticsearch:9200");
    let endpoint = format!("{}/_cluster/health", elastic_url.trim_end_matches('/'));
    let sanitized_endpoint = sanitize_endpoint(&endpoint);
    let auth = ElasticsearchProbeAuth::from_env();

    let mut request = state.http_client().get(&endpoint).timeout(cfg.timeout);
    request = match &auth {
        ElasticsearchProbeAuth::ApiKey(api_key) => {
            request.header("Authorization", format!("ApiKey {}", api_key))
        }
        ElasticsearchProbeAuth::Basic { username, password } => {
            request.basic_auth(username, Some(password))
        }
        ElasticsearchProbeAuth::None => request,
    };

    match request.send().await {
        Ok(response) => {
            let http_status = response.status().as_u16();
            let (status, detail) =
                map_elasticsearch_probe_result(http_status, auth.has_credentials());
            PlatformHealthComponent {
                name: "elasticsearch".to_string(),
                category: "observability".to_string(),
                status,
                checked_at_ms,
                latency_ms: start.elapsed().as_millis() as u64,
                endpoint: Some(sanitized_endpoint),
                http_status: Some(http_status),
                detail,
                source: "active_probe".to_string(),
            }
        }
        Err(err) => PlatformHealthComponent {
            name: "elasticsearch".to_string(),
            category: "observability".to_string(),
            status: if err.is_connect() || err.is_timeout() {
                HealthState::Unreachable
            } else {
                HealthState::Degraded
            },
            checked_at_ms,
            latency_ms: start.elapsed().as_millis() as u64,
            endpoint: Some(sanitized_endpoint),
            http_status: None,
            detail: Some(err.to_string()),
            source: "active_probe".to_string(),
        },
    }
}

async fn probe_http(
    state: &AppState,
    cfg: &OpsHealthConfig,
    name: &str,
    category: &str,
    endpoint: &str,
) -> PlatformHealthComponent {
    let checked_at_ms = unix_timestamp_ms();
    let start = Instant::now();
    let sanitized_endpoint = sanitize_endpoint(endpoint);
    let request = state.http_client().get(endpoint).timeout(cfg.timeout);
    match request.send().await {
        Ok(response) => {
            let http_status = response.status().as_u16();
            let status = if response.status().is_success() {
                HealthState::Healthy
            } else if http_status == 401 || http_status == 403 {
                HealthState::Misconfigured
            } else if http_status == 408 || http_status == 429 || http_status >= 500 {
                HealthState::Degraded
            } else {
                HealthState::Unknown
            };
            PlatformHealthComponent {
                name: name.to_string(),
                category: category.to_string(),
                status,
                checked_at_ms,
                latency_ms: start.elapsed().as_millis() as u64,
                endpoint: Some(sanitized_endpoint),
                http_status: Some(http_status),
                detail: if response.status().is_success() {
                    None
                } else {
                    Some(format!("HTTP {}", http_status))
                },
                source: "active_probe".to_string(),
            }
        }
        Err(err) => PlatformHealthComponent {
            name: name.to_string(),
            category: category.to_string(),
            status: if err.is_connect() || err.is_timeout() {
                HealthState::Unreachable
            } else {
                HealthState::Degraded
            },
            checked_at_ms,
            latency_ms: start.elapsed().as_millis() as u64,
            endpoint: Some(sanitized_endpoint),
            http_status: None,
            detail: Some(err.to_string()),
            source: "active_probe".to_string(),
        },
    }
}

async fn probe_postgres(state: &AppState, cfg: &OpsHealthConfig) -> PlatformHealthComponent {
    let checked_at_ms = unix_timestamp_ms();
    let start = Instant::now();
    let probe =
        tokio::time::timeout(cfg.timeout, sqlx::query("SELECT 1").fetch_one(state.pool())).await;

    match probe {
        Ok(Ok(_)) => PlatformHealthComponent {
            name: "postgres".to_string(),
            category: "datastore".to_string(),
            status: HealthState::Healthy,
            checked_at_ms,
            latency_ms: start.elapsed().as_millis() as u64,
            endpoint: None,
            http_status: None,
            detail: None,
            source: "active_probe".to_string(),
        },
        Ok(Err(err)) => PlatformHealthComponent {
            name: "postgres".to_string(),
            category: "datastore".to_string(),
            status: HealthState::Unreachable,
            checked_at_ms,
            latency_ms: start.elapsed().as_millis() as u64,
            endpoint: None,
            http_status: None,
            detail: Some(err.to_string()),
            source: "active_probe".to_string(),
        },
        Err(_) => PlatformHealthComponent {
            name: "postgres".to_string(),
            category: "datastore".to_string(),
            status: HealthState::Unreachable,
            checked_at_ms,
            latency_ms: start.elapsed().as_millis() as u64,
            endpoint: None,
            http_status: None,
            detail: Some("probe timed out".to_string()),
            source: "active_probe".to_string(),
        },
    }
}

async fn probe_redis(state: &AppState, cfg: &OpsHealthConfig) -> PlatformHealthComponent {
    let checked_at_ms = unix_timestamp_ms();
    let start = Instant::now();
    let Some(redis_url) = state.redis_url() else {
        return PlatformHealthComponent {
            name: "redis".to_string(),
            category: "datastore".to_string(),
            status: HealthState::Misconfigured,
            checked_at_ms,
            latency_ms: 0,
            endpoint: None,
            http_status: None,
            detail: Some("OD_CACHE_REDIS_URL is not configured".to_string()),
            source: "active_probe".to_string(),
        };
    };

    let client = match redis::Client::open(redis_url.to_string()) {
        Ok(client) => client,
        Err(err) => {
            return PlatformHealthComponent {
                name: "redis".to_string(),
                category: "datastore".to_string(),
                status: HealthState::Misconfigured,
                checked_at_ms,
                latency_ms: start.elapsed().as_millis() as u64,
                endpoint: Some(sanitize_endpoint(redis_url)),
                http_status: None,
                detail: Some(err.to_string()),
                source: "active_probe".to_string(),
            }
        }
    };

    let ping_result = tokio::time::timeout(cfg.timeout, async {
        let mut conn = client.get_async_connection().await?;
        let pong: String = redis::cmd("PING").query_async(&mut conn).await?;
        Ok::<String, redis::RedisError>(pong)
    })
    .await;

    match ping_result {
        Ok(Ok(_)) => PlatformHealthComponent {
            name: "redis".to_string(),
            category: "datastore".to_string(),
            status: HealthState::Healthy,
            checked_at_ms,
            latency_ms: start.elapsed().as_millis() as u64,
            endpoint: Some(sanitize_endpoint(redis_url)),
            http_status: None,
            detail: None,
            source: "active_probe".to_string(),
        },
        Ok(Err(err)) => PlatformHealthComponent {
            name: "redis".to_string(),
            category: "datastore".to_string(),
            status: HealthState::Unreachable,
            checked_at_ms,
            latency_ms: start.elapsed().as_millis() as u64,
            endpoint: Some(sanitize_endpoint(redis_url)),
            http_status: None,
            detail: Some(err.to_string()),
            source: "active_probe".to_string(),
        },
        Err(_) => PlatformHealthComponent {
            name: "redis".to_string(),
            category: "datastore".to_string(),
            status: HealthState::Unreachable,
            checked_at_ms,
            latency_ms: start.elapsed().as_millis() as u64,
            endpoint: Some(sanitize_endpoint(redis_url)),
            http_status: None,
            detail: Some("probe timed out".to_string()),
            source: "active_probe".to_string(),
        },
    }
}

fn local_component(
    name: &str,
    category: &str,
    status: HealthState,
    detail: &str,
) -> PlatformHealthComponent {
    PlatformHealthComponent {
        name: name.to_string(),
        category: category.to_string(),
        status,
        checked_at_ms: unix_timestamp_ms(),
        latency_ms: 0,
        endpoint: None,
        http_status: None,
        detail: Some(detail.to_string()),
        source: "local".to_string(),
    }
}

fn build_summary(components: &[PlatformHealthComponent]) -> PlatformHealthSummary {
    let mut summary = PlatformHealthSummary {
        total: components.len(),
        healthy: 0,
        degraded: 0,
        unreachable: 0,
        misconfigured: 0,
        unknown: 0,
    };
    for component in components {
        match component.status {
            HealthState::Healthy => summary.healthy += 1,
            HealthState::Degraded => summary.degraded += 1,
            HealthState::Unreachable => summary.unreachable += 1,
            HealthState::Misconfigured => summary.misconfigured += 1,
            HealthState::Unknown => summary.unknown += 1,
        }
    }
    summary
}

fn aggregate_status(components: &[PlatformHealthComponent]) -> HealthState {
    if components
        .iter()
        .any(|item| matches!(item.status, HealthState::Unreachable))
    {
        return HealthState::Unreachable;
    }
    if components
        .iter()
        .any(|item| matches!(item.status, HealthState::Misconfigured))
    {
        return HealthState::Misconfigured;
    }
    if components
        .iter()
        .any(|item| matches!(item.status, HealthState::Degraded))
    {
        return HealthState::Degraded;
    }
    if components
        .iter()
        .any(|item| matches!(item.status, HealthState::Unknown))
    {
        return HealthState::Unknown;
    }
    HealthState::Healthy
}

fn env_or_default(key: &str, fallback: &str) -> String {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn sanitize_endpoint(raw: &str) -> String {
    if let Ok(mut parsed) = reqwest::Url::parse(raw) {
        if !parsed.username().is_empty() {
            let _ = parsed.set_username("redacted");
        }
        if parsed.password().is_some() {
            let _ = parsed.set_password(Some("redacted"));
        }
        parsed.set_query(None);
        return parsed.to_string();
    }

    if let Some((scheme, rest)) = raw.split_once("://") {
        if let Some((creds, suffix)) = rest.split_once('@') {
            if creds.contains(':') {
                return format!("{}://redacted:redacted@{}", scheme, suffix);
            }
            return format!("{}://redacted@{}", scheme, suffix);
        }
    }
    raw.to_string()
}

fn health_state_label(state: &HealthState) -> &'static str {
    match state {
        HealthState::Healthy => "healthy",
        HealthState::Degraded => "degraded",
        HealthState::Unreachable => "unreachable",
        HealthState::Misconfigured => "misconfigured",
        HealthState::Unknown => "unknown",
    }
}

fn map_elasticsearch_probe_result(
    status_code: u16,
    has_credentials: bool,
) -> (HealthState, Option<String>) {
    if (200..=299).contains(&status_code) {
        return (HealthState::Healthy, None);
    }
    if status_code == 401 || status_code == 403 {
        let detail = if has_credentials {
            "HTTP 401/403 (invalid credentials or insufficient Elasticsearch privileges)"
                .to_string()
        } else {
            "HTTP 401/403 (credentials missing for secured Elasticsearch cluster)".to_string()
        };
        return (HealthState::Misconfigured, Some(detail));
    }
    if status_code == 408 || status_code == 429 || status_code >= 500 {
        return (HealthState::Degraded, Some(format!("HTTP {}", status_code)));
    }
    (HealthState::Unknown, Some(format!("HTTP {}", status_code)))
}

fn unix_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_prefers_unreachable_over_other_states() {
        let components = vec![
            local_component("a", "x", HealthState::Healthy, "ok"),
            local_component("b", "x", HealthState::Degraded, "warn"),
            local_component("c", "x", HealthState::Unreachable, "down"),
        ];
        assert!(matches!(
            aggregate_status(&components),
            HealthState::Unreachable
        ));
    }

    #[test]
    fn sanitize_endpoint_redacts_credentials() {
        let sanitized = sanitize_endpoint("https://user:pass@example.com/v1/models?key=abc");
        assert!(sanitized.contains("redacted"));
        assert!(!sanitized.contains("pass"));
        assert!(!sanitized.contains("key=abc"));
    }

    #[test]
    fn elasticsearch_auth_prefers_api_key_over_basic() {
        unsafe {
            env::set_var("OD_REPORTING_ELASTIC_API_KEY", "api-key");
            env::set_var("OD_REPORTING_ELASTIC_USERNAME", "elastic");
            env::set_var("OD_REPORTING_ELASTIC_PASSWORD", "secret");
        }
        let auth = ElasticsearchProbeAuth::from_env();
        assert!(matches!(auth, ElasticsearchProbeAuth::ApiKey(_)));
        unsafe {
            env::remove_var("OD_REPORTING_ELASTIC_API_KEY");
            env::remove_var("OD_REPORTING_ELASTIC_USERNAME");
            env::remove_var("OD_REPORTING_ELASTIC_PASSWORD");
        }
    }

    #[test]
    fn elasticsearch_401_reason_differs_by_credentials_presence() {
        let (_status_without_creds, detail_without_creds) =
            map_elasticsearch_probe_result(401, false);
        assert!(detail_without_creds
            .unwrap_or_default()
            .contains("credentials missing"));

        let (_status_with_creds, detail_with_creds) = map_elasticsearch_probe_result(401, true);
        assert!(detail_with_creds
            .unwrap_or_default()
            .contains("invalid credentials"));
    }
}
