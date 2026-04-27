mod audit;
mod auth;
mod cache;
mod cache_entries_api;
mod classification_requests;
mod classifications;
mod cli_logs;
mod iam;
mod metrics;
mod ops_health;
mod page_contents;
mod pagination;
mod policies;
mod reporting;
mod reporting_es;
mod taxonomy;

use anyhow::{anyhow, Context, Result};
use audit::{AuditEvent, AuditLogger, ElasticExporter};
use auth::{
    enforce_admin, require_roles, validate_settings_for_startup, AdminAuth, AuthMode, AuthSettings,
    UserContext, ROLE_OVERRIDES_DELETE, ROLE_OVERRIDES_VIEW, ROLE_OVERRIDES_WRITE,
};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use iam::IamService;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{self, Value};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    PgPool, Row,
};
use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{error, info, warn, Level};
use uuid::Uuid;

use ::taxonomy::{CanonicalTaxonomy, TaxonomyStore};
use cache::CacheInvalidator;
use common_types::normalizer::{canonical_classification_key_with_policy, CanonicalizationPolicy};
use metrics::ReviewMetrics;
use pagination::{
    cursor_limit, decode_cursor_with_direction, encode_directional_cursor, CursorDirection,
    CursorPaged,
};
use reporting_es::{ElasticReportingClient, ReportingConfig};

fn check_config_mode_enabled() -> bool {
    std::env::args().any(|arg| arg == "--check-config")
}

fn resolve_admin_db_url(cfg: &AdminApiConfig) -> Option<String> {
    if let Some(url) = cfg.database_url.clone() {
        return Some(url);
    }
    config_core::lookup_env("OD_ADMIN_DATABASE_URL", &["DATABASE_URL"]).value
}

fn emit_env_alias_warnings(cfg: &AdminApiConfig) {
    if cfg.database_url.is_none() {
        if let Some(alias) =
            config_core::lookup_env("OD_ADMIN_DATABASE_URL", &["DATABASE_URL"]).deprecated_alias
        {
            eprintln!(
                "warning: admin-api deprecated env alias in use: {} -> OD_ADMIN_DATABASE_URL",
                alias
            );
        }
    }
}

fn validate_config(cfg: &AdminApiConfig) -> Result<()> {
    let mut validator = config_core::ConfigValidator::new("admin-api");
    validator.require_non_empty(
        "host",
        Some(cfg.host.as_str()),
        "set host in config/admin-api.json",
    );
    let db_url = resolve_admin_db_url(cfg);
    validator.require_non_empty(
        "database_url",
        db_url.as_deref(),
        "set config/admin-api.json.database_url or OD_ADMIN_DATABASE_URL (fallback: DATABASE_URL)",
    );
    validator.require_auth_url(
        "OD_ADMIN_DATABASE_URL",
        db_url.as_deref(),
        true,
        true,
        12,
        "set OD_ADMIN_DATABASE_URL with non-default username/password credentials",
    );

    let env_admin_token = env::var("OD_ADMIN_TOKEN").ok();
    let admin_token = cfg.admin_token.as_deref().or(env_admin_token.as_deref());
    validator.require_strong_secret(
        "OD_ADMIN_TOKEN",
        admin_token,
        16,
        "set OD_ADMIN_TOKEN to a high-entropy value",
    );

    let env_policy_token = env::var("OD_POLICY_ADMIN_TOKEN").ok();
    let policy_token = cfg
        .policy_engine_admin_token
        .as_deref()
        .or(env_policy_token.as_deref())
        .or(admin_token);
    validator.require_strong_secret(
        "OD_POLICY_ADMIN_TOKEN",
        policy_token,
        16,
        "set OD_POLICY_ADMIN_TOKEN to a unique strong token",
    );

    let default_admin_password = env::var("OD_DEFAULT_ADMIN_PASSWORD").ok();
    validator.validate_optional_secret(
        "OD_DEFAULT_ADMIN_PASSWORD",
        default_admin_password.as_deref(),
        12,
        "set OD_DEFAULT_ADMIN_PASSWORD to a strong temporary bootstrap password",
    );

    let oidc_hs256 = env::var("OD_OIDC_HS256_SECRET")
        .ok()
        .or(cfg.auth.oidc_hs256_secret.clone());
    validator.validate_optional_secret(
        "OD_OIDC_HS256_SECRET",
        oidc_hs256.as_deref(),
        24,
        "set OD_OIDC_HS256_SECRET to a strong random signing secret",
    );

    let reporting = cfg.reporting.clone().merge_env();
    validator.validate_optional_secret(
        "OD_REPORTING_ELASTIC_PASSWORD",
        reporting.password.as_deref(),
        12,
        "use strong Elasticsearch credentials for OD_REPORTING_ELASTIC_PASSWORD",
    );
    validator.validate_optional_secret(
        "OD_REPORTING_ELASTIC_API_KEY",
        reporting.api_key.as_deref(),
        16,
        "set OD_REPORTING_ELASTIC_API_KEY to a non-default API key",
    );

    let audit = cfg.audit.clone().merge_env();
    validator.validate_optional_secret(
        "OD_AUDIT_ELASTIC_API_KEY",
        audit.api_key.as_deref(),
        16,
        "set OD_AUDIT_ELASTIC_API_KEY to a non-default API key",
    );

    validator.finish()?;
    validate_settings_for_startup(cfg.auth.clone())?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct AdminApiConfig {
    pub host: String,
    pub port: u16,
    pub database_url: Option<String>,
    pub admin_token: Option<String>,
    pub redis_url: Option<String>,
    pub cache_channel: Option<String>,
    pub policy_engine_url: Option<String>,
    pub policy_engine_admin_token: Option<String>,
    #[serde(default)]
    pub auth: AuthSettings,
    #[serde(default)]
    pub audit: AuditExportConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub reporting: ReportingConfig,
    #[serde(default)]
    pub canonicalization: CanonicalizationConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CanonicalizationConfig {
    #[serde(default)]
    pub tenant_domain_exceptions: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct AuditExportConfig {
    pub elastic_url: Option<String>,
    pub index: Option<String>,
    pub api_key: Option<String>,
}

impl AuditExportConfig {
    fn merge_env(mut self) -> Self {
        if let Ok(url) = env::var("OD_AUDIT_ELASTIC_URL") {
            self.elastic_url = non_empty_env_value(url);
        }
        if let Ok(index) = env::var("OD_AUDIT_ELASTIC_INDEX") {
            self.index = non_empty_env_value(index);
        }
        if let Ok(key) = env::var("OD_AUDIT_ELASTIC_API_KEY") {
            self.api_key = non_empty_env_value(key);
        }
        self
    }
}

fn non_empty_env_value(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MetricsConfig {
    #[serde(default = "default_review_sla_seconds")]
    pub review_sla_seconds: u64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            review_sla_seconds: default_review_sla_seconds(),
        }
    }
}

impl MetricsConfig {
    fn merge_env(mut self) -> Self {
        if let Ok(value) = env::var("OD_REVIEW_SLA_SECONDS") {
            if let Ok(parsed) = value.parse::<u64>() {
                self.review_sla_seconds = parsed;
            }
        }
        self
    }
}

fn default_review_sla_seconds() -> u64 {
    4 * 60 * 60
}

#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
    admin_auth: Arc<AdminAuth>,
    cache_invalidator: Option<Arc<CacheInvalidator>>,
    audit_logger: AuditLogger,
    metrics: ReviewMetrics,
    reporting_client: Option<ElasticReportingClient>,
    iam: Arc<IamService>,
    canonical_taxonomy: Arc<CanonicalTaxonomy>,
    taxonomy_store: Arc<TaxonomyStore>,
    taxonomy_mutation_enabled: bool,
    policy_engine_url: String,
    policy_engine_admin_token: Option<String>,
    llm_providers_url: String,
    prometheus_url: Option<String>,
    http_client: reqwest::Client,
    classification_job_publisher: Option<Arc<ClassificationJobPublisher>>,
    canonicalization_policy: Arc<CanonicalizationPolicy>,
    redis_url: Option<String>,
    ops_health_config: ops_health::OpsHealthConfig,
    ops_health_cache: Arc<RwLock<Option<ops_health::CachedPlatformHealth>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmProviderSummary {
    name: String,
    #[serde(default)]
    provider_type: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    health_status: Option<String>,
    #[serde(default)]
    health_checked_at_ms: Option<u64>,
    #[serde(default)]
    health_latency_ms: Option<u64>,
    #[serde(default)]
    health_http_status: Option<u16>,
    #[serde(default)]
    health_detail: Option<String>,
}

#[derive(Clone)]
struct ClassificationJobPublisher {
    client: redis::Client,
    stream: String,
}

impl ClassificationJobPublisher {
    fn new(redis_url: String, stream: String) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self { client, stream })
    }

    async fn publish(&self, payload: &str) -> Result<()> {
        let mut conn = self.client.get_async_connection().await?;
        let _: () = redis::cmd("XADD")
            .arg(&self.stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct PolicyEngineRuntimeSummary {
    pub policy_id: Option<String>,
    pub version: String,
}

impl AppState {
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn admin_auth(&self) -> Arc<AdminAuth> {
        self.admin_auth.clone()
    }

    pub fn iam(&self) -> Arc<IamService> {
        self.iam.clone()
    }

    pub fn canonical_taxonomy(&self) -> Arc<CanonicalTaxonomy> {
        self.canonical_taxonomy.clone()
    }

    pub fn taxonomy_store(&self) -> Arc<TaxonomyStore> {
        self.taxonomy_store.clone()
    }

    pub fn taxonomy_mutation_enabled(&self) -> bool {
        self.taxonomy_mutation_enabled
    }

    async fn invalidate_override(&self, scope_type: &str, scope_value: &str) {
        if let Some(cache) = &self.cache_invalidator {
            if let Err(err) = cache.invalidate_override(scope_type, scope_value).await {
                error!(
                    target = "svc-admin",
                    %err,
                    scope_type,
                    scope_value,
                    "failed to invalidate cache for override"
                );
            }
        }
    }

    pub async fn invalidate_policy_cache(&self) {
        if let Some(cache) = &self.cache_invalidator {
            if let Err(err) = cache.invalidate_policy().await {
                warn!(target = "svc-admin", %err, "failed to invalidate caches after policy change");
            }
        }
    }

    pub async fn trigger_policy_reload(&self) -> Result<()> {
        let endpoint = format!("{}/api/v1/policies/reload", self.policy_engine_url);
        let mut request = self.http_client.post(&endpoint);
        if let Some(token) = self.policy_engine_admin_token.as_deref() {
            request = request.header("X-Admin-Token", token);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("failed to call policy-engine reload at {}", endpoint))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "policy-engine reload failed with status {} body={} ",
                status,
                body
            ));
        }
        info!(
            target = "svc-admin",
            endpoint, "policy-engine reload triggered"
        );
        Ok(())
    }

    pub async fn fetch_policy_engine_runtime(&self) -> Result<PolicyEngineRuntimeSummary> {
        let endpoint = format!("{}/api/v1/policies", self.policy_engine_url);
        let mut request = self.http_client.get(&endpoint);
        if let Some(token) = self.policy_engine_admin_token.as_deref() {
            request = request.header("X-Admin-Token", token);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("failed to call policy-engine list at {}", endpoint))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "policy-engine list failed with status {} body={}",
                status,
                body
            ));
        }
        let parsed = response
            .json::<PolicyEngineRuntimeSummary>()
            .await
            .context("failed to decode policy-engine runtime payload")?;
        Ok(parsed)
    }

    pub async fn evaluate_policy_decision<T, R>(&self, payload: &T) -> Result<R>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let endpoint = format!("{}/api/v1/decision", self.policy_engine_url);
        let mut request = self.http_client.post(&endpoint).json(payload);
        if let Some(token) = self.policy_engine_admin_token.as_deref() {
            request = request.header("X-Admin-Token", token);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("failed to call policy-engine decision at {}", endpoint))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "policy-engine decision failed with status {} body={}",
                status,
                body
            ));
        }
        let parsed = response
            .json::<R>()
            .await
            .context("failed to decode policy decision response")?;
        Ok(parsed)
    }

    pub async fn invalidate_cache_key(&self, cache_key: &str) {
        if let Some(cache) = &self.cache_invalidator {
            if let Err(err) = cache.invalidate_key(cache_key).await {
                warn!(
                    target = "svc-admin",
                    %err,
                    cache_key,
                    "failed to purge cache entry"
                );
            }
        }
    }

    async fn log_override_event<T>(
        &self,
        action: &str,
        actor: Option<String>,
        target_id: String,
        payload: T,
    ) where
        T: serde::Serialize,
    {
        let payload = serde_json::to_value(payload).ok();
        self.audit_logger
            .log(AuditEvent {
                actor,
                action: action.to_string(),
                target_type: Some("override".into()),
                target_id: Some(target_id),
                payload,
            })
            .await;
    }

    pub async fn log_policy_event<T>(
        &self,
        action: &str,
        actor: Option<String>,
        target_id: Option<String>,
        payload: T,
    ) where
        T: serde::Serialize,
    {
        let payload = serde_json::to_value(payload).ok();
        self.audit_logger
            .log(AuditEvent {
                actor,
                action: action.to_string(),
                target_type: Some("policy".into()),
                target_id,
                payload,
            })
            .await;
    }

    pub fn reporting_client(&self) -> Option<&ElasticReportingClient> {
        self.reporting_client.as_ref()
    }

    pub async fn queue_classification_job<T>(&self, payload: &T) -> Result<()>
    where
        T: Serialize,
    {
        let publisher = self
            .classification_job_publisher
            .as_ref()
            .ok_or_else(|| anyhow!("classification job publisher is not configured"))?;
        let serialized = serde_json::to_string(payload)?;
        publisher.publish(&serialized).await
    }

    async fn fetch_llm_provider_summaries(&self) -> Result<Vec<LlmProviderSummary>> {
        let response = self
            .http_client
            .get(&self.llm_providers_url)
            .send()
            .await
            .with_context(|| {
                format!(
                    "failed to fetch llm providers from {}",
                    self.llm_providers_url
                )
            })?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "llm providers endpoint returned {}",
                response.status()
            ));
        }

        response
            .json::<Vec<LlmProviderSummary>>()
            .await
            .context("failed to decode llm providers response")
    }

    pub async fn validate_llm_provider_name(&self, provider_name: &str) -> Result<bool> {
        let providers = self.fetch_llm_provider_summaries().await?;
        Ok(providers.iter().any(|item| item.name == provider_name))
    }

    pub fn prometheus_url(&self) -> Option<&str> {
        self.prometheus_url.as_deref()
    }

    pub fn cache_invalidator(&self) -> Option<&CacheInvalidator> {
        self.cache_invalidator.as_deref()
    }

    pub fn policy_engine_url(&self) -> &str {
        &self.policy_engine_url
    }

    pub fn llm_providers_url(&self) -> &str {
        &self.llm_providers_url
    }

    pub fn redis_url(&self) -> Option<&str> {
        self.redis_url.as_deref()
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    pub fn ops_health_config(&self) -> &ops_health::OpsHealthConfig {
        &self.ops_health_config
    }

    pub fn ops_health_cache(&self) -> Arc<RwLock<Option<ops_health::CachedPlatformHealth>>> {
        self.ops_health_cache.clone()
    }

    pub fn canonicalize_key(&self, normalized_key: &str, tenant: Option<&str>) -> String {
        canonical_classification_key_with_policy(
            normalized_key,
            self.canonicalization_policy.as_ref(),
            tenant,
        )
        .unwrap_or_else(|| normalized_key.to_string())
    }

    pub async fn log_iam_event<T>(
        &self,
        action: &str,
        actor: Option<String>,
        target_type: &str,
        target_id: Option<String>,
        payload: T,
    ) where
        T: Serialize,
    {
        let payload_value = serde_json::to_value(&payload).ok();
        self.iam
            .record_iam_event(
                actor.clone(),
                action,
                target_type,
                target_id.clone(),
                payload_value.clone().unwrap_or(Value::Null),
            )
            .await;
        self.audit_logger
            .log(AuditEvent {
                actor,
                action: action.to_string(),
                target_type: Some(target_type.to_string()),
                target_id,
                payload: payload_value,
            })
            .await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: AdminApiConfig = config_core::load_config("config/admin-api.json")?;
    validate_config(&cfg)?;
    emit_env_alias_warnings(&cfg);
    if check_config_mode_enabled() {
        println!("admin-api config check passed");
        return Ok(());
    }
    let db_url = resolve_admin_db_url(&cfg).context(
        "database_url required: set config/admin-api.json or OD_ADMIN_DATABASE_URL / DATABASE_URL",
    )?;
    let admin_token = cfg
        .admin_token
        .clone()
        .or_else(|| env::var("OD_ADMIN_TOKEN").ok());
    let redis_url = cfg
        .redis_url
        .clone()
        .or_else(|| env::var("OD_CACHE_REDIS_URL").ok());
    let cache_channel = cfg
        .cache_channel
        .clone()
        .or_else(|| env::var("OD_CACHE_CHANNEL").ok());
    let policy_engine_url = cfg
        .policy_engine_url
        .clone()
        .or_else(|| env::var("OD_POLICY_ENGINE_URL").ok())
        .unwrap_or_else(|| "http://policy-engine:19010".to_string());
    let policy_engine_admin_token = cfg
        .policy_engine_admin_token
        .clone()
        .or_else(|| env::var("OD_POLICY_ADMIN_TOKEN").ok())
        .or_else(|| admin_token.clone());
    let llm_providers_url = env::var("OD_LLM_PROVIDERS_URL")
        .unwrap_or_else(|_| "http://llm-worker:19015/providers".to_string());
    let prometheus_url = env::var("OD_PROMETHEUS_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| Some("http://prometheus:9090".to_string()));
    let classification_job_stream =
        env::var("OD_CLASSIFICATION_STREAM").unwrap_or_else(|_| "classification-jobs".to_string());
    let cache_invalidator = redis_url
        .as_ref()
        .map(|url| CacheInvalidator::new(url.clone(), cache_channel.clone()))
        .transpose()
        .context("failed to initialize cache invalidator")?
        .map(Arc::new);
    let classification_job_publisher = redis_url
        .as_ref()
        .map(|url| ClassificationJobPublisher::new(url.clone(), classification_job_stream.clone()))
        .transpose()
        .context("failed to initialize classification job publisher")?
        .map(Arc::new);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let iam_service = Arc::new(IamService::new(pool.clone()));

    let merged_auth = cfg.auth.clone().merge_env();
    if matches!(merged_auth.mode, AuthMode::Local | AuthMode::Hybrid) {
        let has_active_admin = iam_service
            .has_active_policy_admin()
            .await
            .context("failed to verify active local policy-admin presence")?;
        if has_active_admin {
            info!(
                target = "svc-admin",
                "skipping local admin bootstrap: active policy-admin already exists"
            );
        } else {
            let default_password = env::var("OD_DEFAULT_ADMIN_PASSWORD").context(
                "OD_DEFAULT_ADMIN_PASSWORD is required only for first local bootstrap when no active policy-admin exists",
            )?;
            iam_service
                .bootstrap_local_admin(&default_password)
                .await
                .context("failed to bootstrap default local admin")?;
        }
    }

    let admin_auth = Arc::new(
        AdminAuth::from_config(admin_token.clone(), merged_auth, iam_service.clone())
            .await
            .context("failed to initialize auth")?,
    );

    let audit_cfg = cfg.audit.clone().merge_env();
    let metrics_cfg = cfg.metrics.clone().merge_env();
    let reporting_cfg = cfg.reporting.clone().merge_env();

    let elastic_exporter = if let (Some(url), Some(index)) =
        (audit_cfg.elastic_url.clone(), audit_cfg.index.clone())
    {
        Some(
            ElasticExporter::new(url, index, audit_cfg.api_key.clone())
                .context("failed to initialize elastic exporter")?,
        )
    } else {
        None
    };

    let review_metrics = ReviewMetrics::new(metrics_cfg.review_sla_seconds);

    if redis_url.is_none() {
        warn!(
            target = "svc-admin",
            "cache invalidation disabled (redis_url not configured)"
        );
    } else if let Some(ref inv) = cache_invalidator {
        info!(
            target = "svc-admin",
            channel = inv.channel_name(),
            "cache invalidation enabled"
        );
    }

    let reporting_client = ElasticReportingClient::from_config(&reporting_cfg)
        .context("failed to initialize reporting client")?;

    let canonical_taxonomy = CanonicalTaxonomy::load_from_env()
        .context("failed to load canonical taxonomy")?
        .into_arc();
    let taxonomy_store = Arc::new(TaxonomyStore::new(canonical_taxonomy.clone()));
    let taxonomy_mutation_enabled = env::var("OD_TAXONOMY_MUTATION_ENABLED")
        .map(|value| matches!(value.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    let state = AppState {
        pool: pool.clone(),
        admin_auth: admin_auth.clone(),
        cache_invalidator,
        audit_logger: AuditLogger::new(pool.clone(), elastic_exporter),
        metrics: review_metrics,
        reporting_client,
        iam: iam_service.clone(),
        canonical_taxonomy,
        taxonomy_store,
        taxonomy_mutation_enabled,
        policy_engine_url,
        policy_engine_admin_token,
        llm_providers_url,
        prometheus_url,
        http_client: reqwest::Client::new(),
        classification_job_publisher,
        canonicalization_policy: Arc::new(CanonicalizationPolicy::from_tenant_exceptions(
            cfg.canonicalization.tenant_domain_exceptions.clone(),
        )),
        redis_url: redis_url.clone(),
        ops_health_config: ops_health::OpsHealthConfig::from_env(),
        ops_health_cache: Arc::new(RwLock::new(None)),
    };

    let auth_layer = {
        let auth = admin_auth.clone();
        middleware::from_fn(move |req, next| {
            let auth = auth.clone();
            async move { enforce_admin(auth, req, next).await }
        })
    };

    let cors_allow_origin = env::var("OD_ADMIN_CORS_ALLOW_ORIGIN")
        .unwrap_or_else(|_| "https://localhost:19001".to_string());
    let cors_layer = {
        let allow_origin = cors_allow_origin.clone();
        middleware::from_fn(move |req, next| {
            let allow_origin = allow_origin.clone();
            async move { cors_middleware(req, next, &allow_origin).await }
        })
    };

    let admin_routes = Router::new()
        .route(
            "/api/v1/overrides",
            get(list_overrides).post(create_override),
        )
        .route("/api/v1/overrides/export", get(export_overrides))
        .route("/api/v1/overrides/import", post(import_overrides))
        .route(
            "/api/v1/overrides/:id",
            delete(delete_override).put(update_override),
        )
        .route(
            "/api/v1/policies",
            get(policies::list_policies).post(policies::create_policy),
        )
        .route(
            "/api/v1/policies/runtime-sync",
            get(policies::policy_runtime_sync),
        )
        .route("/api/v1/policies/validate", post(policies::validate_policy))
        .route(
            "/api/v1/policies/:id",
            get(policies::get_policy)
                .put(policies::update_policy)
                .delete(policies::delete_policy),
        )
        .route(
            "/api/v1/policies/:id/versions",
            get(policies::list_policy_versions),
        )
        .route(
            "/api/v1/policies/:id/publish",
            post(policies::publish_policy),
        )
        .route("/api/v1/taxonomy", get(taxonomy::get_taxonomy))
        .route(
            "/api/v1/taxonomy/activation",
            put(taxonomy::update_taxonomy_activation),
        )
        .route(
            "/api/v1/taxonomy/categories",
            post(taxonomy::block_category_mutation),
        )
        .route(
            "/api/v1/taxonomy/categories/:id",
            put(taxonomy::block_category_mutation).delete(taxonomy::block_category_mutation),
        )
        .route(
            "/api/v1/taxonomy/subcategories",
            post(taxonomy::block_subcategory_mutation),
        )
        .route(
            "/api/v1/taxonomy/subcategories/:id",
            put(taxonomy::block_subcategory_mutation).delete(taxonomy::block_subcategory_mutation),
        )
        .route("/api/v1/reporting/traffic", get(reporting::traffic_summary))
        .route("/api/v1/reporting/status", get(reporting::reporting_status))
        .route(
            "/api/v1/reporting/dashboard",
            get(reporting::dashboard_summary),
        )
        .route("/api/v1/reporting/ops-summary", get(reporting::ops_summary))
        .route(
            "/api/v1/reporting/ops-llm-series",
            get(reporting::ops_llm_series),
        )
        .route(
            "/api/v1/cache-entries/:cache_key",
            get(cache_entries_api::get_entry).delete(cache_entries_api::delete_entry),
        )
        .route("/api/v1/cli-logs", get(cli_logs::list_cli_logs))
        .route(
            "/api/v1/classifications/pending",
            get(classification_requests::list_pending)
                .delete(classification_requests::clear_all_pending),
        )
        .route("/api/v1/classifications", get(classifications::list))
        .route(
            "/api/v1/classifications/export",
            get(classifications::export_bundle),
        )
        .route(
            "/api/v1/classifications/import",
            post(classifications::import_bundle),
        )
        .route(
            "/api/v1/classifications/flush",
            post(classifications::flush),
        )
        .route(
            "/api/v1/classifications/:normalized_key",
            delete(classifications::delete).patch(classifications::update),
        )
        .route(
            "/api/v1/classifications/:normalized_key/unblock",
            post(classification_requests::manual_unblock),
        )
        .route(
            "/api/v1/classifications/:normalized_key/manual-classify",
            post(classification_requests::manual_classify),
        )
        .route(
            "/api/v1/classifications/:normalized_key/metadata-classify",
            post(classification_requests::metadata_classify),
        )
        .route(
            "/api/v1/classifications/:normalized_key/pending",
            post(classification_requests::upsert_pending)
                .delete(classification_requests::clear_pending),
        )
        .route(
            "/api/v1/iam/users",
            get(iam::list_users_route).post(iam::create_user_route),
        )
        .route(
            "/api/v1/iam/users/:id",
            get(iam::get_user_route)
                .put(iam::update_user_route)
                .delete(iam::delete_user_route),
        )
        .route(
            "/api/v1/iam/users/:id/disable",
            post(iam::disable_user_route),
        )
        .route("/api/v1/iam/users/:id/enable", post(iam::enable_user_route))
        .route(
            "/api/v1/iam/users/:id/roles",
            post(iam::assign_user_role_route),
        )
        .route(
            "/api/v1/iam/users/:id/roles/:role",
            delete(iam::revoke_user_role_route),
        )
        .route(
            "/api/v1/iam/users/:id/set-password",
            post(iam::set_user_password_route),
        )
        .route(
            "/api/v1/iam/users/:id/tokens",
            get(iam::list_user_tokens_route).post(iam::create_user_token_route),
        )
        .route(
            "/api/v1/iam/users/:id/tokens/:token_id",
            delete(iam::revoke_user_token_route),
        )
        .route(
            "/api/v1/iam/groups",
            get(iam::list_groups_route).post(iam::create_group_route),
        )
        .route(
            "/api/v1/iam/groups/:id",
            get(iam::get_group_route)
                .put(iam::update_group_route)
                .delete(iam::delete_group_route),
        )
        .route(
            "/api/v1/iam/groups/:id/members",
            get(iam::list_group_members_route).post(iam::add_member_route),
        )
        .route(
            "/api/v1/iam/groups/:id/members/:user_id",
            delete(iam::remove_member_route),
        )
        .route(
            "/api/v1/iam/groups/:id/roles",
            post(iam::assign_group_role_route),
        )
        .route(
            "/api/v1/iam/groups/:id/roles/:role",
            delete(iam::revoke_group_role_route),
        )
        .route("/api/v1/iam/roles", get(iam::list_roles_route))
        .route(
            "/api/v1/iam/service-accounts",
            get(iam::list_service_accounts_route).post(iam::create_service_account_route),
        )
        .route(
            "/api/v1/iam/service-accounts/:id",
            get(iam::get_service_account_route).delete(iam::disable_service_account_route),
        )
        .route(
            "/api/v1/iam/service-accounts/:id/rotate",
            post(iam::rotate_service_account_route),
        )
        .route("/api/v1/iam/whoami", get(iam::whoami_route))
        .route("/api/v1/iam/audit", get(iam::list_audit_route))
        .route(
            "/api/v1/auth/change-password",
            post(auth::change_password_route),
        )
        .route(
            "/api/v1/page-contents/:normalized_key",
            get(page_contents::get_page_content),
        )
        .route(
            "/api/v1/page-contents/:normalized_key/history",
            get(page_contents::list_page_content_history),
        )
        .route("/api/v1/ops/llm/providers", get(list_llm_providers))
        .route(
            "/api/v1/ops/platform-health",
            get(ops_health::platform_health),
        )
        .with_state(state.clone())
        .layer(auth_layer);

    let app = Router::new()
        .route("/health/ready", get(health))
        .route("/health/live", get(health))
        .route("/metrics", get(metrics_endpoint))
        .route("/api/v1/auth/login", post(auth::login_route))
        .route("/api/v1/auth/refresh", post(auth::refresh_route))
        .route("/api/v1/auth/logout", post(auth::logout_route))
        .route("/api/v1/auth/mode", get(auth::auth_mode_route))
        .with_state(state)
        .merge(admin_routes)
        .layer(cors_layer);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(target = "svc-admin", %addr, "admin api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn cors_middleware(
    req: Request<axum::body::Body>,
    next: middleware::Next,
    allow_origin: &str,
) -> Response {
    let origin = req.headers().get(header::ORIGIN).cloned();

    if req.method() == Method::OPTIONS {
        let mut response = Response::new(axum::body::Body::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        apply_cors_headers(response, origin.as_ref(), allow_origin)
    } else {
        let response = next.run(req).await;
        apply_cors_headers(response, origin.as_ref(), allow_origin)
    }
}

fn apply_cors_headers(
    mut response: Response,
    request_origin: Option<&HeaderValue>,
    allow_origin: &str,
) -> Response {
    let allow_origin_header = if allow_origin == "*" {
        Some(HeaderValue::from_static("*"))
    } else {
        request_origin.and_then(|origin| {
            if origin == allow_origin {
                Some(origin.clone())
            } else {
                None
            }
        })
    };

    if let Some(value) = allow_origin_header {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET,POST,PUT,PATCH,DELETE,OPTIONS"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Authorization,Content-Type,X-Admin-Token"),
        );
    }

    response
}

async fn health() -> &'static str {
    "OK"
}

async fn list_llm_providers(
    State(state): State<AppState>,
) -> Result<Json<Vec<LlmProviderSummary>>, StatusCode> {
    let providers = state.fetch_llm_provider_summaries().await.map_err(|err| {
        warn!(
            target = "svc-admin",
            %err,
            url = %state.llm_providers_url,
            "failed to fetch llm providers"
        );
        StatusCode::BAD_GATEWAY
    })?;

    Ok(Json(providers))
}

async fn metrics_endpoint(
    State(state): State<AppState>,
) -> Result<(StatusCode, String), StatusCode> {
    state
        .metrics
        .render()
        .map(|body| (StatusCode::OK, body))
        .map_err(|err| {
            error!(target = "svc-admin", %err, "failed to render metrics");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn list_overrides(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<OverrideListQuery>,
) -> Result<Json<CursorPaged<OverrideRecord>>, StatusCode> {
    require_roles(&user, ROLE_OVERRIDES_VIEW)?;

    let limit = cursor_limit(query.limit);
    let cursor = query
        .cursor
        .as_deref()
        .map(decode_cursor_with_direction::<OverrideCursor>)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (cursor_direction, cursor_anchor) = cursor
        .as_ref()
        .map(|(direction, anchor)| (*direction, Some(anchor)))
        .unwrap_or((CursorDirection::Next, None));
    let cursor_created_at = cursor_anchor.map(|c| c.created_at);
    let cursor_id = cursor_anchor.map(|c| c.id).unwrap_or_else(Uuid::nil);

    let rows = if cursor_direction == CursorDirection::Prev {
        sqlx::query(
            r#"SELECT id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at
        FROM overrides
        WHERE ($1::timestamptz IS NULL OR (created_at, id) > ($1, $2))
        ORDER BY created_at ASC, id ASC
        LIMIT $3"#,
        )
        .bind(cursor_created_at)
        .bind(cursor_id)
        .bind((limit + 1) as i64)
        .fetch_all(&state.pool)
        .await
        .map_err(|err| {
            error!(target = "svc-admin", %err, "list_overrides failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        sqlx::query(
            r#"SELECT id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at
        FROM overrides
        WHERE ($1::timestamptz IS NULL OR (created_at, id) < ($1, $2))
        ORDER BY created_at DESC, id DESC
        LIMIT $3"#,
        )
        .bind(cursor_created_at)
        .bind(cursor_id)
        .bind((limit + 1) as i64)
        .fetch_all(&state.pool)
        .await
        .map_err(|err| {
            error!(target = "svc-admin", %err, "list_overrides failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    };

    let mut overrides = Vec::with_capacity(rows.len());
    for row in rows {
        overrides.push(map_override_row(&row).map_err(|err| {
            error!(target = "svc-admin", %err, "failed to map override row");
            StatusCode::INTERNAL_SERVER_ERROR
        })?);
    }

    let has_more = overrides.len() > limit as usize;
    if has_more {
        overrides.truncate(limit as usize);
    }
    if cursor_direction == CursorDirection::Prev {
        overrides.reverse();
    }
    let next_cursor = if has_more {
        overrides.last().and_then(|row| {
            encode_directional_cursor(
                CursorDirection::Next,
                &OverrideCursor {
                    created_at: row.created_at,
                    id: row.id,
                },
            )
            .ok()
        })
    } else {
        None
    };
    let prev_cursor = if query.cursor.is_some() && !overrides.is_empty() {
        overrides.first().and_then(|row| {
            encode_directional_cursor(
                CursorDirection::Prev,
                &OverrideCursor {
                    created_at: row.created_at,
                    id: row.id,
                },
            )
            .ok()
        })
    } else {
        None
    };

    Ok(Json(CursorPaged::new_with_prev(
        overrides,
        limit,
        has_more,
        next_cursor,
        prev_cursor,
    )))
}

async fn export_overrides(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<OverrideExportQuery>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_OVERRIDES_VIEW)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let action = normalize_action(&query.action)?;

    let rows = sqlx::query(
        r#"SELECT scope_value
           FROM overrides
           WHERE scope_type = 'domain'
             AND action = $1
             AND status = 'active'
           ORDER BY scope_value ASC"#,
    )
    .bind(&action)
    .fetch_all(&state.pool)
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    let mut lines = Vec::with_capacity(rows.len());
    for row in rows {
        let scope_value: String = row.try_get("scope_value").map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;
        lines.push(scope_value);
    }
    lines.sort();
    lines.dedup();

    let body = lines.join("\n");
    let filename = format!("overrides-{}-exchange.txt", action);
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename))
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("EXPORT_ERROR", err.to_string())),
            )
        })?;

    state
        .log_override_event(
            &format!("override.export.{}", action),
            Some(user.actor.clone()),
            action.clone(),
            serde_json::json!({
                "count": lines.len(),
            }),
        )
        .await;

    let response = (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            ),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        body,
    )
        .into_response();
    Ok(response)
}

async fn import_overrides(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(payload): Json<OverrideImportRequest>,
) -> Result<Json<OverrideImportResponse>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_OVERRIDES_WRITE)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;

    let action = normalize_action(&payload.action)?;
    let mode = normalize_override_import_mode(payload.mode.as_deref())?;
    let dry_run = payload.dry_run.unwrap_or(true);
    let parsed = parse_override_exchange_lines(&payload.content);

    let existing_rows = sqlx::query(
        r#"SELECT id, scope_value, action, status
           FROM overrides
           WHERE scope_type = 'domain'
           ORDER BY scope_value ASC, updated_at DESC, created_at DESC, id DESC"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    let mut active_by_scope: HashMap<String, (Uuid, String)> = HashMap::new();
    let mut latest_by_scope_action: HashMap<(String, String), Uuid> = HashMap::new();
    let mut active_scope_values_by_action: HashMap<String, HashSet<String>> = HashMap::new();
    let mut duplicate_active_ids = Vec::new();

    for row in existing_rows {
        let id: Uuid = row.try_get("id").map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;
        let scope_value: String = row.try_get("scope_value").map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;
        let row_action: String = row.try_get("action").map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;
        let status: String = row.try_get("status").map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;

        latest_by_scope_action
            .entry((scope_value.clone(), row_action.clone()))
            .or_insert(id);

        if status.eq_ignore_ascii_case("active") {
            if active_by_scope.contains_key(&scope_value) {
                duplicate_active_ids.push(id);
                continue;
            }
            active_scope_values_by_action
                .entry(row_action.clone())
                .or_default()
                .insert(scope_value.clone());
            active_by_scope.insert(scope_value, (id, row_action));
        }
    }

    let mut imported_would = 0_u32;
    let mut updated_would = 0_u32;
    let skipped_would = 0_u32;
    for scope_value in &parsed.scopes {
        if active_by_scope.contains_key(scope_value)
            || latest_by_scope_action.contains_key(&(scope_value.clone(), action.clone()))
        {
            updated_would += 1;
        } else {
            imported_would += 1;
        }
    }

    let imported_scope_set: HashSet<&str> =
        parsed.scopes.iter().map(|scope| scope.as_str()).collect();
    let deleted_would = if matches!(mode, OverrideImportMode::Replace) {
        active_scope_values_by_action
            .get(&action)
            .into_iter()
            .flatten()
            .filter(|scope| !imported_scope_set.contains(scope.as_str()))
            .count() as u32
    } else {
        0
    };

    let mut imported = imported_would;
    let mut updated = updated_would;
    let mut skipped = skipped_would;
    let mut deleted = deleted_would;

    if !dry_run {
        let mut tx = state.pool.begin().await.map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;

        imported = 0;
        updated = 0;
        skipped = 0;

        if !duplicate_active_ids.is_empty() {
            sqlx::query(
                r#"UPDATE overrides
                   SET status = 'revoked',
                       updated_at = NOW()
                   WHERE id = ANY($1::uuid[])"#,
            )
            .bind(&duplicate_active_ids)
            .execute(&mut *tx)
            .await
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new("DB_ERROR", err.to_string())),
                )
            })?;
        }

        for scope_value in &parsed.scopes {
            if let Some((active_id, _active_action)) = active_by_scope.get(scope_value).cloned() {
                sqlx::query(
                    r#"UPDATE overrides
                       SET action = $1,
                           reason = $2,
                           created_by = $3,
                           expires_at = NULL,
                           status = 'active',
                           updated_at = NOW()
                       WHERE id = $4"#,
                )
                .bind(&action)
                .bind("Imported via Allow / Deny Exchange")
                .bind(&user.actor)
                .bind(active_id)
                .execute(&mut *tx)
                .await
                .map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError::new("DB_ERROR", err.to_string())),
                    )
                })?;
                updated += 1;
                active_by_scope.insert(scope_value.clone(), (active_id, action.clone()));
            } else if let Some(existing_id) = latest_by_scope_action
                .get(&(scope_value.clone(), action.clone()))
                .copied()
            {
                sqlx::query(
                    r#"UPDATE overrides
                       SET action = $1,
                           reason = $2,
                           created_by = $3,
                           expires_at = NULL,
                           status = 'active',
                           updated_at = NOW()
                       WHERE id = $4"#,
                )
                .bind(&action)
                .bind("Imported via Allow / Deny Exchange")
                .bind(&user.actor)
                .bind(existing_id)
                .execute(&mut *tx)
                .await
                .map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError::new("DB_ERROR", err.to_string())),
                    )
                })?;
                updated += 1;
                active_by_scope.insert(scope_value.clone(), (existing_id, action.clone()));
            } else {
                let new_id = Uuid::new_v4();
                sqlx::query(
                    r#"INSERT INTO overrides (id, scope_type, scope_value, action, reason, created_by, status)
                       VALUES ($1, 'domain', $2, $3, $4, $5, 'active')"#,
                )
                .bind(new_id)
                .bind(scope_value)
                .bind(&action)
                .bind("Imported via Allow / Deny Exchange")
                .bind(&user.actor)
                .execute(&mut *tx)
                .await
                .map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError::new("DB_ERROR", err.to_string())),
                    )
                })?;
                imported += 1;
                latest_by_scope_action.insert((scope_value.clone(), action.clone()), new_id);
                active_by_scope.insert(scope_value.clone(), (new_id, action.clone()));
            }

            if let Some((id, _)) = active_by_scope.get(scope_value).cloned() {
                latest_by_scope_action.insert((scope_value.clone(), action.clone()), id);
            }

            state.invalidate_override("domain", scope_value).await;
        }

        deleted = 0;
        if matches!(mode, OverrideImportMode::Replace) {
            let removed_rows = if parsed.scopes.is_empty() {
                sqlx::query(
                    r#"DELETE FROM overrides
                       WHERE scope_type = 'domain'
                         AND action = $1
                         AND status = 'active'
                       RETURNING scope_type, scope_value"#,
                )
                .bind(&action)
                .fetch_all(&mut *tx)
                .await
            } else {
                sqlx::query(
                    r#"DELETE FROM overrides
                       WHERE scope_type = 'domain'
                         AND action = $1
                         AND status = 'active'
                         AND NOT (scope_value = ANY($2::text[]))
                       RETURNING scope_type, scope_value"#,
                )
                .bind(&action)
                .bind(&parsed.scopes)
                .fetch_all(&mut *tx)
                .await
            }
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new("DB_ERROR", err.to_string())),
                )
            })?;

            deleted = removed_rows.len() as u32;
            for row in removed_rows {
                let removed_scope_type: String = row.try_get("scope_type").map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError::new("DB_ERROR", err.to_string())),
                    )
                })?;
                let removed_scope_value: String = row.try_get("scope_value").map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError::new("DB_ERROR", err.to_string())),
                    )
                })?;
                state
                    .invalidate_override(&removed_scope_type, &removed_scope_value)
                    .await;
            }
        }

        tx.commit().await.map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;
    }

    let response = OverrideImportResponse {
        action: action.clone(),
        mode: mode.as_str().to_string(),
        dry_run,
        total_lines: parsed.total_lines,
        parsed: parsed.scopes.len() as u32,
        duplicates: parsed.duplicates,
        imported,
        updated,
        deleted,
        skipped,
        invalid: parsed.invalid_lines.len() as u32,
        invalid_lines: parsed.invalid_lines.clone(),
    };

    state
        .log_override_event(
            &format!("override.import.{}", action),
            Some(user.actor.clone()),
            action.clone(),
            &response,
        )
        .await;

    Ok(Json(response))
}

async fn create_override(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Json(mut payload): Json<OverrideUpsertRequest>,
) -> Result<Json<OverrideRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_OVERRIDES_WRITE)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    if payload.created_by.is_none() {
        payload.created_by = Some(user.actor.clone());
    }
    let validated = validate_override_payload(payload)?;
    let ValidatedOverridePayload {
        scope_type,
        scope_value,
        action,
        reason,
        created_by,
        expires_at,
        status,
    } = validated;

    let status_value = status.unwrap_or_else(|| "active".to_string());
    let mut tx = state.pool.begin().await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    let row = if status_value.eq_ignore_ascii_case("active") {
        let active_rows = sqlx::query(
            r#"SELECT id
               FROM overrides
               WHERE scope_type = $1
                 AND scope_value = $2
                 AND status = 'active'
               ORDER BY updated_at DESC, created_at DESC, id DESC"#,
        )
        .bind(&scope_type)
        .bind(&scope_value)
        .fetch_all(&mut *tx)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;

        let primary_active_id = active_rows
            .first()
            .and_then(|row| row.try_get::<Uuid, _>("id").ok());
        if active_rows.len() > 1 {
            let mut duplicate_ids = Vec::with_capacity(active_rows.len() - 1);
            for row in active_rows.iter().skip(1) {
                if let Ok(duplicate_id) = row.try_get::<Uuid, _>("id") {
                    duplicate_ids.push(duplicate_id);
                }
            }
            if !duplicate_ids.is_empty() {
                sqlx::query(
                    r#"UPDATE overrides
                       SET status = 'revoked',
                           updated_at = NOW()
                       WHERE id = ANY($1::uuid[])"#,
                )
                .bind(&duplicate_ids)
                .execute(&mut *tx)
                .await
                .map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError::new("DB_ERROR", err.to_string())),
                    )
                })?;
            }
        }

        if let Some(active_id) = primary_active_id {
            sqlx::query(
                r#"UPDATE overrides
                   SET action = $1,
                       reason = $2,
                       created_by = $3,
                       expires_at = $4,
                       status = $5,
                       updated_at = NOW()
                   WHERE id = $6
                   RETURNING id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at"#,
            )
            .bind(&action)
            .bind(&reason)
            .bind(&created_by)
            .bind(expires_at)
            .bind(&status_value)
            .bind(active_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new("DB_ERROR", err.to_string())),
                )
            })?
        } else {
            sqlx::query(
                r#"INSERT INTO overrides (id, scope_type, scope_value, action, reason, created_by, expires_at, status)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                   RETURNING id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at"#,
            )
            .bind(Uuid::new_v4())
            .bind(&scope_type)
            .bind(&scope_value)
            .bind(&action)
            .bind(&reason)
            .bind(&created_by)
            .bind(expires_at)
            .bind(&status_value)
            .fetch_one(&mut *tx)
            .await
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new("DB_ERROR", err.to_string())),
                )
            })?
        }
    } else {
        sqlx::query(
            r#"INSERT INTO overrides (id, scope_type, scope_value, action, reason, created_by, expires_at, status)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at"#,
        )
        .bind(Uuid::new_v4())
        .bind(&scope_type)
        .bind(&scope_value)
        .bind(&action)
        .bind(&reason)
        .bind(&created_by)
        .bind(expires_at)
        .bind(&status_value)
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?
    };

    tx.commit().await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    let mapped = map_override_row(&row).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    state
        .invalidate_override(&mapped.scope_type, &mapped.scope_value)
        .await;
    state
        .log_override_event(
            "override.create",
            mapped
                .created_by
                .clone()
                .or_else(|| Some(user.actor.clone())),
            mapped.id.to_string(),
            &mapped,
        )
        .await;

    Ok(Json(mapped))
}

async fn delete_override(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_OVERRIDES_DELETE)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    let deleted =
        sqlx::query("DELETE FROM overrides WHERE id = $1 RETURNING scope_type, scope_value")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new("DB_ERROR", err.to_string())),
                )
            })?;

    let Some(row) = deleted else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "override not found")),
        ));
    };

    let scope_type: String = row.try_get("scope_type").map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;
    let scope_value: String = row.try_get("scope_value").map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    state.invalidate_override(&scope_type, &scope_value).await;
    state
        .log_override_event(
            "override.delete",
            Some(user.actor.clone()),
            id.to_string(),
            serde_json::json!({
                "scope_type": scope_type,
                "scope_value": scope_value
            }),
        )
        .await;

    Ok(())
}

async fn update_override(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(mut payload): Json<OverrideUpsertRequest>,
) -> Result<Json<OverrideRecord>, (StatusCode, Json<ApiError>)> {
    require_roles(&user, ROLE_OVERRIDES_WRITE)
        .map_err(|status| (status, Json(ApiError::forbidden())))?;
    if payload.created_by.is_none() {
        payload.created_by = Some(user.actor.clone());
    }
    let validated = validate_override_payload(payload)?;
    let ValidatedOverridePayload {
        scope_type,
        scope_value,
        action,
        reason,
        created_by,
        expires_at,
        status,
    } = validated;

    let mut tx = state.pool.begin().await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    let existing =
        sqlx::query("SELECT scope_type, scope_value, status FROM overrides WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError::new("DB_ERROR", err.to_string())),
                )
            })?;

    let Some(existing_row) = existing else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "override not found")),
        ));
    };

    let previous_scope_type: String = existing_row.try_get("scope_type").map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;
    let previous_scope_value: String = existing_row.try_get("scope_value").map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;
    let existing_status: String = existing_row.try_get("status").map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;
    let next_status = status.unwrap_or(existing_status);

    if next_status.eq_ignore_ascii_case("active") {
        sqlx::query(
            r#"UPDATE overrides
               SET status = 'revoked',
                   updated_at = NOW()
               WHERE scope_type = $1
                 AND scope_value = $2
                 AND status = 'active'
                 AND id <> $3"#,
        )
        .bind(&scope_type)
        .bind(&scope_value)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;
    }

    let row = sqlx::query(
        r#"UPDATE overrides
            SET scope_type = $1,
                scope_value = $2,
                action = $3,
                reason = $4,
                created_by = $5,
                expires_at = $6,
                status = $7,
                updated_at = NOW()
          WHERE id = $8
          RETURNING id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at"#,
    )
    .bind(&scope_type)
    .bind(&scope_value)
    .bind(&action)
    .bind(&reason)
    .bind(&created_by)
    .bind(expires_at)
    .bind(&next_status)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|err| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    ))?;

    let Some(row) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "override not found")),
        ));
    };

    tx.commit().await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    let mapped = map_override_row(&row).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    for (invalidate_scope_type, invalidate_scope_value) in override_invalidation_scopes(
        &previous_scope_type,
        &previous_scope_value,
        &mapped.scope_type,
        &mapped.scope_value,
    ) {
        state
            .invalidate_override(&invalidate_scope_type, &invalidate_scope_value)
            .await;
    }
    state
        .log_override_event(
            "override.update",
            mapped
                .created_by
                .clone()
                .or_else(|| Some(user.actor.clone())),
            mapped.id.to_string(),
            &mapped,
        )
        .await;

    Ok(Json(mapped))
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    error_code: &'static str,
    message: String,
}

impl ApiError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            error_code: code,
            message: message.into(),
        }
    }

    pub fn forbidden() -> Self {
        Self::new("FORBIDDEN", "insufficient privileges")
    }

    pub fn code(&self) -> &str {
        self.error_code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Deserialize)]
struct OverrideUpsertRequest {
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OverrideListQuery {
    limit: Option<u32>,
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OverrideExportQuery {
    action: String,
}

#[derive(Debug, Deserialize)]
struct OverrideImportRequest {
    action: String,
    mode: Option<String>,
    dry_run: Option<bool>,
    content: String,
}

#[derive(Debug, Serialize, Clone)]
struct OverrideImportInvalidLine {
    line_number: u32,
    value: String,
    error: String,
}

#[derive(Debug, Serialize)]
struct OverrideImportResponse {
    action: String,
    mode: String,
    dry_run: bool,
    total_lines: u32,
    parsed: u32,
    duplicates: u32,
    imported: u32,
    updated: u32,
    deleted: u32,
    skipped: u32,
    invalid: u32,
    invalid_lines: Vec<OverrideImportInvalidLine>,
}

#[derive(Debug)]
struct ParsedOverrideImport {
    total_lines: u32,
    scopes: Vec<String>,
    duplicates: u32,
    invalid_lines: Vec<OverrideImportInvalidLine>,
}

#[derive(Debug, Clone, Copy)]
enum OverrideImportMode {
    Merge,
    Replace,
}

impl OverrideImportMode {
    fn as_str(self) -> &'static str {
        match self {
            OverrideImportMode::Merge => "merge",
            OverrideImportMode::Replace => "replace",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct OverrideCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Debug, Serialize)]
struct OverrideRecord {
    id: Uuid,
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug)]
struct ValidatedOverridePayload {
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    status: Option<String>,
}

const ALLOWED_SCOPE_TYPES: &[&str] = &["domain"];
const ALLOWED_ACTIONS: &[&str] = &["allow", "block"];
const ALLOWED_STATUSES: &[&str] = &["active", "inactive", "expired", "revoked"];

fn validate_override_payload(
    payload: OverrideUpsertRequest,
) -> Result<ValidatedOverridePayload, (StatusCode, Json<ApiError>)> {
    let scope_type = normalize_scope_type(&payload.scope_type)?;
    let scope_value = normalize_scope_value(&scope_type, &payload.scope_value)?;
    let action = normalize_action(&payload.action)?;
    let reason = normalize_optional_field(payload.reason);
    let created_by = normalize_optional_field(payload.created_by);
    let expires_at = payload.expires_at;
    let status = match payload.status {
        Some(value) => Some(normalize_status(&value)?),
        None => None,
    };

    Ok(ValidatedOverridePayload {
        scope_type,
        scope_value,
        action,
        reason,
        created_by,
        expires_at,
        status,
    })
}

fn normalize_scope_type(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(validation_error("scope_type required"));
    }
    if ALLOWED_SCOPE_TYPES.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(validation_error("scope_type must be domain"))
    }
}

fn normalize_scope_value(
    scope_type: &str,
    value: &str,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    match scope_type {
        "domain" => normalize_domain_scope(value),
        _ => Err(validation_error("unsupported scope_type")),
    }
}

fn normalize_domain_scope(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let lowered = value.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return Err(validation_error(
            "scope_value required for domain overrides",
        ));
    }

    let (wildcard_prefix, domain_part) = if let Some(rest) = lowered.strip_prefix("*.") {
        ("*.", rest)
    } else {
        ("", lowered.as_str())
    };

    if domain_part.is_empty() || !domain_part.contains('.') {
        return Err(validation_error(
            "domain scope must include a valid hostname",
        ));
    }

    if domain_part.len() > 253 {
        return Err(validation_error("domain scope exceeds maximum length"));
    }

    if !domain_part
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        return Err(validation_error(
            "domain scope may only include alphanumeric, '-', '.' characters",
        ));
    }

    Ok(format!("{}{}", wildcard_prefix, domain_part))
}

fn normalize_action(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(validation_error("action required"));
    }
    if ALLOWED_ACTIONS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(validation_error("action must be one of allow|block"))
    }
}

fn normalize_status(value: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(validation_error("status cannot be empty"));
    }
    if ALLOWED_STATUSES.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        Err(validation_error(
            "status must be one of active|inactive|expired|revoked",
        ))
    }
}

fn normalize_override_import_mode(
    value: Option<&str>,
) -> Result<OverrideImportMode, (StatusCode, Json<ApiError>)> {
    let normalized = value.unwrap_or("merge").trim().to_ascii_lowercase();
    match normalized.as_str() {
        "merge" => Ok(OverrideImportMode::Merge),
        "replace" => Ok(OverrideImportMode::Replace),
        _ => Err(validation_error("mode must be merge|replace")),
    }
}

fn parse_override_exchange_lines(content: &str) -> ParsedOverrideImport {
    let mut seen = HashSet::new();
    let mut scopes = Vec::new();
    let mut invalid_lines = Vec::new();
    let mut duplicates = 0_u32;
    let mut total_lines = 0_u32;

    for (index, raw_line) in content.lines().enumerate() {
        total_lines += 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        match normalize_domain_scope(trimmed) {
            Ok(scope_value) => {
                if seen.insert(scope_value.clone()) {
                    scopes.push(scope_value);
                } else {
                    duplicates += 1;
                }
            }
            Err((_, err)) => {
                invalid_lines.push(OverrideImportInvalidLine {
                    line_number: (index + 1) as u32,
                    value: trimmed.to_string(),
                    error: err.0.message().to_string(),
                });
            }
        }
    }

    ParsedOverrideImport {
        total_lines,
        scopes,
        duplicates,
        invalid_lines,
    }
}

fn normalize_optional_field(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn override_invalidation_scopes(
    previous_scope_type: &str,
    previous_scope_value: &str,
    updated_scope_type: &str,
    updated_scope_value: &str,
) -> Vec<(String, String)> {
    let mut scopes = Vec::with_capacity(2);
    scopes.push((
        previous_scope_type.to_string(),
        previous_scope_value.to_string(),
    ));
    if previous_scope_type != updated_scope_type || previous_scope_value != updated_scope_value {
        scopes.push((
            updated_scope_type.to_string(),
            updated_scope_value.to_string(),
        ));
    }
    scopes
}

pub fn validation_error(message: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError::new("VALIDATION_ERROR", message.to_string())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_admin_cfg() -> AdminApiConfig {
        serde_json::from_value(json!({
            "host": "0.0.0.0",
            "port": 19000,
            "database_url": "postgres://svc_admin:prod-db-password-123@db.example:5432/defender_admin",
            "admin_token": "prod-admin-token-0123456789",
            "policy_engine_admin_token": "prod-policy-token-0123456789",
            "auth": {
              "mode": "local",
              "local_jwt_secret": "prod-local-jwt-secret-0123456789abcdef"
            },
            "reporting": {
              "elastic_url": "http://elasticsearch:9200",
              "index_pattern": "traffic-events-*",
              "username": "elastic",
              "password": "prod-elastic-password-abcdef"
            }
        }))
        .expect("valid admin config")
    }

    fn base_request() -> OverrideUpsertRequest {
        OverrideUpsertRequest {
            scope_type: "domain".into(),
            scope_value: "example.com".into(),
            action: "block".into(),
            reason: None,
            created_by: Some("tester".into()),
            expires_at: None,
            status: None,
        }
    }

    #[test]
    fn validates_domain_override() {
        let payload = base_request();
        let result = validate_override_payload(payload).unwrap();
        assert_eq!(result.scope_type, "domain");
        assert_eq!(result.scope_value, "example.com");
        assert_eq!(result.action, "block");
    }

    #[test]
    fn rejects_unknown_scope_type() {
        let mut payload = base_request();
        payload.scope_type = "device".into();
        let err = validate_override_payload(payload).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.message.contains("scope_type"));
    }

    #[test]
    fn normalizes_wildcard_domain() {
        let mut payload = base_request();
        payload.scope_value = "*.Example.com".into();
        let result = validate_override_payload(payload).unwrap();
        assert_eq!(result.scope_value, "*.example.com");
    }

    #[test]
    fn invalidates_both_previous_and_updated_scope_on_change() {
        let scopes = override_invalidation_scopes(
            "domain",
            "mozilla.org",
            "domain",
            "incoming.telemetry.mozilla.org",
        );
        assert_eq!(
            scopes,
            vec![
                ("domain".to_string(), "mozilla.org".to_string()),
                (
                    "domain".to_string(),
                    "incoming.telemetry.mozilla.org".to_string()
                )
            ]
        );
    }

    #[test]
    fn invalidates_scope_once_when_scope_unchanged() {
        let scopes = override_invalidation_scopes(
            "domain",
            "incoming.telemetry.mozilla.org",
            "domain",
            "incoming.telemetry.mozilla.org",
        );
        assert_eq!(
            scopes,
            vec![(
                "domain".to_string(),
                "incoming.telemetry.mozilla.org".to_string()
            )]
        );
    }

    #[test]
    fn parses_override_exchange_lines_with_comments_and_duplicates() {
        let parsed = parse_override_exchange_lines(
            "# allow list\nexample.com\n\n*.Example.org\nexample.com\ninvalid host\n",
        );
        assert_eq!(parsed.total_lines, 6);
        assert_eq!(parsed.scopes, vec!["example.com", "*.example.org"]);
        assert_eq!(parsed.duplicates, 1);
        assert_eq!(parsed.invalid_lines.len(), 1);
        assert_eq!(parsed.invalid_lines[0].line_number, 6);
    }

    #[test]
    fn normalize_import_mode_defaults_to_merge() {
        assert!(matches!(
            normalize_override_import_mode(None).unwrap(),
            OverrideImportMode::Merge
        ));
        assert!(matches!(
            normalize_override_import_mode(Some("replace")).unwrap(),
            OverrideImportMode::Replace
        ));
        assert!(normalize_override_import_mode(Some("all")).is_err());
    }

    #[test]
    fn config_rejects_default_secret_values_when_dev_mode_disabled() {
        std::env::remove_var("OD_ALLOW_INSECURE_DEV_SECRETS");
        std::env::set_var("OD_DEFAULT_ADMIN_PASSWORD", "changeme-local-admin-password");
        let mut cfg = base_admin_cfg();
        cfg.admin_token = Some("changeme-admin".into());
        let err = validate_config(&cfg).expect_err("default secrets must fail");
        let rendered = format!("{err:#}");
        assert!(rendered.contains("OD_ADMIN_TOKEN"));
        std::env::remove_var("OD_DEFAULT_ADMIN_PASSWORD");
    }

    #[test]
    fn config_accepts_strong_secrets_when_dev_mode_disabled() {
        std::env::remove_var("OD_ALLOW_INSECURE_DEV_SECRETS");
        std::env::set_var("OD_DEFAULT_ADMIN_PASSWORD", "strong-bootstrap-password-123");
        let cfg = base_admin_cfg();
        assert!(validate_config(&cfg).is_ok());
        std::env::remove_var("OD_DEFAULT_ADMIN_PASSWORD");
    }

    #[test]
    fn config_allows_default_secrets_only_with_explicit_dev_override() {
        std::env::set_var("OD_ALLOW_INSECURE_DEV_SECRETS", "true");
        std::env::set_var("OD_DEFAULT_ADMIN_PASSWORD", "changeme-local-admin-password");
        let mut cfg = base_admin_cfg();
        cfg.admin_token = Some("changeme-admin".into());
        assert!(validate_config(&cfg).is_ok());
        std::env::remove_var("OD_DEFAULT_ADMIN_PASSWORD");
        std::env::remove_var("OD_ALLOW_INSECURE_DEV_SECRETS");
    }
}

fn map_override_row(row: &PgRow) -> sqlx::Result<OverrideRecord> {
    Ok(OverrideRecord {
        id: row.try_get("id")?,
        scope_type: row.try_get("scope_type")?,
        scope_value: row.try_get("scope_value")?,
        action: row.try_get("action")?,
        reason: row.try_get("reason")?,
        created_by: row.try_get("created_by")?,
        expires_at: row.try_get("expires_at")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
