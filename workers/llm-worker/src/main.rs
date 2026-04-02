mod metrics;
mod schema;

use anyhow::{anyhow, Context, Result};
use common_types::{ClassificationVerdict, PageFetchJob, PolicyAction, PolicyDecision};
use redis::streams::{StreamReadOptions, StreamReadReply};
use redis::AsyncCommands;
use reqwest::{Client, RequestBuilder};
use schema::{LlmResponse, PromptPayload};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env, fmt,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use taxonomy::{ActivationState, FallbackReason, TaxonomyStore};
use tokio::sync::Mutex;
use tokio::{signal, time::Instant};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct WorkerConfig {
    pub queue_name: String,
    pub redis_url: String,
    pub cache_channel: String,
    #[serde(default = "default_stream")]
    pub stream: String,
    #[serde(default = "default_page_fetch_stream")]
    pub page_fetch_stream: String,
    pub database_url: String,
    #[serde(default)]
    pub llm_endpoint: Option<String>,
    #[serde(default)]
    pub llm_api_key: Option<String>,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub routing: RoutingConfig,
    #[serde(default = "default_metrics_host")]
    pub metrics_host: String,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
}

#[derive(Debug, Deserialize, Clone)]
struct ProviderConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ProviderKind,
    pub endpoint: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ProviderKind {
    LmStudio,
    Ollama,
    Vllm,
    Openai,
    Anthropic,
    OpenaiCompatible,
    CustomJson,
}

impl ProviderKind {
    fn label(&self) -> &'static str {
        match self {
            ProviderKind::LmStudio => "lmstudio",
            ProviderKind::Ollama => "ollama",
            ProviderKind::Vllm => "vllm",
            ProviderKind::Openai => "openai",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::OpenaiCompatible => "openai_compatible",
            ProviderKind::CustomJson => "custom_json",
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
struct RoutingConfig {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub primary_retry_max: Option<usize>,
    #[serde(default)]
    pub primary_retry_backoff_ms: Option<u64>,
    #[serde(default)]
    pub primary_retry_max_backoff_ms: Option<u64>,
    #[serde(default)]
    pub retryable_status_codes: Vec<u16>,
    #[serde(default)]
    pub fallback_cooldown_secs: Option<u64>,
    #[serde(default)]
    pub fallback_max_per_min: Option<usize>,
}

impl WorkerConfig {
    fn resolve_router(&self) -> Result<ProviderRouter> {
        if !self.providers.is_empty() {
            let name = self
                .routing
                .default
                .as_deref()
                .or_else(|| self.providers.first().map(|p| p.name.as_str()))
                .ok_or_else(|| anyhow!("providers defined but no default specified"))?;
            let primary = self.find_provider(name)?;
            let fallback = if let Some(fallback_name) = self.routing.fallback.as_deref() {
                if fallback_name == name {
                    None
                } else {
                    Some(self.find_provider(fallback_name)?)
                }
            } else {
                None
            };
            let mut catalog = Vec::new();
            catalog.push(metrics::ProviderSummary {
                name: primary.name.clone(),
                provider_type: primary.kind.label().to_string(),
                endpoint: primary.endpoint.clone(),
                role: metrics::ProviderRole::Primary,
            });
            if let Some(fallback_ref) = fallback.as_ref() {
                catalog.push(metrics::ProviderSummary {
                    name: fallback_ref.name.clone(),
                    provider_type: fallback_ref.kind.label().to_string(),
                    endpoint: fallback_ref.endpoint.clone(),
                    role: metrics::ProviderRole::Fallback,
                });
            }
            return Ok(ProviderRouter {
                primary,
                fallback,
                catalog: Arc::new(catalog),
            });
        }

        // fallback to legacy fields
        let endpoint = self
            .llm_endpoint
            .clone()
            .ok_or_else(|| anyhow!("llm_endpoint missing and no providers configured"))?;
        let api_key = self
            .llm_api_key
            .clone()
            .or_else(|| env::var("LLM_API_KEY").ok())
            .unwrap_or_default();
        Ok(ProviderRouter {
            primary: ResolvedProvider {
                name: "legacy".into(),
                kind: ProviderKind::CustomJson,
                endpoint,
                model: None,
                timeout_ms: None,
                headers: HashMap::new(),
                api_key,
            },
            fallback: None,
            catalog: Arc::new(vec![metrics::ProviderSummary {
                name: "legacy".into(),
                provider_type: ProviderKind::CustomJson.label().to_string(),
                endpoint: "(config file)".into(),
                role: metrics::ProviderRole::Legacy,
            }]),
        })
    }

    fn find_provider(&self, name: &str) -> Result<ResolvedProvider> {
        let provider = self
            .providers
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| anyhow!("provider '{name}' not found"))?;
        ResolvedProvider::from_config(provider)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ResolvedProvider {
    name: String,
    kind: ProviderKind,
    endpoint: String,
    model: Option<String>,
    timeout_ms: Option<u64>,
    headers: HashMap<String, String>,
    api_key: String,
}

struct ProviderRouter {
    primary: ResolvedProvider,
    fallback: Option<ResolvedProvider>,
    catalog: Arc<Vec<metrics::ProviderSummary>>,
}

impl ProviderRouter {
    fn primary(&self) -> &ResolvedProvider {
        &self.primary
    }

    fn fallback(&self) -> Option<&ResolvedProvider> {
        self.fallback.as_ref()
    }

    fn catalog(&self) -> Arc<Vec<metrics::ProviderSummary>> {
        Arc::clone(&self.catalog)
    }
}

impl ResolvedProvider {
    fn from_config(config: &ProviderConfig) -> Result<Self> {
        let api_key = match (&config.api_key, &config.api_key_env) {
            (Some(value), _) if !value.is_empty() => value.clone(),
            (_, Some(env_key)) => env::var(env_key)
                .map_err(|_| anyhow!("env var {env_key} not set for provider {}", config.name))?,
            _ => String::new(),
        };
        Ok(Self {
            name: config.name.clone(),
            kind: config.kind,
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
            timeout_ms: config.timeout_ms,
            headers: config.headers.clone(),
            api_key,
        })
    }
}

fn default_stream() -> String {
    "classification-jobs".into()
}

fn default_page_fetch_stream() -> String {
    "page-fetch-jobs".into()
}

fn default_metrics_host() -> String {
    "0.0.0.0".into()
}

fn default_metrics_port() -> u16 {
    19015
}

const SYSTEM_PROMPT: &str = "You are an AI analyst classifying web traffic for a trust and safety team. Respond ONLY with JSON matching the schema and avoid prose.";
const PROMPT_HTML_CONTEXT_LIMIT: usize = 120_000;
const OPENAI_DEFAULT_MODEL: &str = "gpt-4o-mini";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const CONTENT_WAIT_ATTEMPTS: usize = 40;
const CONTENT_WAIT_DELAY_SECS: u64 = 3;
const CACHE_TTL_SECONDS: u64 = 3600;
const NON_CANONICAL_RETRY_ATTEMPTS: usize = 3;
const DEFAULT_FAILOVER_POLICY: &str = "aggressive";
const DEFAULT_PRIMARY_RETRY_MAX: usize = 3;
const DEFAULT_PRIMARY_RETRY_BACKOFF_MS: u64 = 500;
const DEFAULT_PRIMARY_RETRY_MAX_BACKOFF_MS: u64 = 5000;
const DEFAULT_RETRYABLE_STATUS_CODES: &[u16] = &[408, 429, 500, 502, 503, 504];
const DEFAULT_FALLBACK_COOLDOWN_SECS: u64 = 30;
const DEFAULT_FALLBACK_MAX_PER_MIN: usize = 30;

#[derive(Debug, Clone, Copy)]
enum FailoverPolicy {
    Aggressive,
    Safe,
    Disabled,
}

impl FailoverPolicy {
    fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "safe" => Self::Safe,
            "disabled" => Self::Disabled,
            _ => Self::Aggressive,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Aggressive => "aggressive",
            Self::Safe => "safe",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InvocationFailureClass {
    Retryable,
    NonRetryable,
}

#[derive(Debug, Clone)]
struct InvocationFailure {
    class: InvocationFailureClass,
    status: Option<u16>,
    reason: String,
}

#[derive(Debug, Clone)]
struct FailoverRuntime {
    policy: FailoverPolicy,
    primary_retry_max: usize,
    primary_retry_backoff_ms: u64,
    primary_retry_max_backoff_ms: u64,
    retryable_status_codes: HashSet<u16>,
    fallback_cooldown_secs: u64,
    fallback_max_per_min: usize,
}

impl FailoverRuntime {
    fn from_routing(routing: &RoutingConfig) -> Self {
        let policy_raw = env::var("OD_LLM_FAILOVER_POLICY")
            .ok()
            .or_else(|| routing.policy.clone())
            .unwrap_or_else(|| DEFAULT_FAILOVER_POLICY.to_string());
        let policy = FailoverPolicy::parse(&policy_raw);

        let primary_retry_max = env_usize("OD_LLM_PRIMARY_RETRY_MAX")
            .or(routing.primary_retry_max)
            .unwrap_or(DEFAULT_PRIMARY_RETRY_MAX);
        let primary_retry_backoff_ms = env_u64("OD_LLM_PRIMARY_RETRY_BACKOFF_MS")
            .or(routing.primary_retry_backoff_ms)
            .unwrap_or(DEFAULT_PRIMARY_RETRY_BACKOFF_MS);
        let primary_retry_max_backoff_ms = env_u64("OD_LLM_PRIMARY_RETRY_MAX_BACKOFF_MS")
            .or(routing.primary_retry_max_backoff_ms)
            .unwrap_or(DEFAULT_PRIMARY_RETRY_MAX_BACKOFF_MS);
        let fallback_cooldown_secs = env_u64("OD_LLM_FALLBACK_COOLDOWN_SECS")
            .or(routing.fallback_cooldown_secs)
            .unwrap_or(DEFAULT_FALLBACK_COOLDOWN_SECS);
        let fallback_max_per_min = env_usize("OD_LLM_FALLBACK_MAX_PER_MIN")
            .or(routing.fallback_max_per_min)
            .unwrap_or(DEFAULT_FALLBACK_MAX_PER_MIN);

        let retryable_status_codes = env_csv_u16("OD_LLM_RETRYABLE_STATUS_CODES")
            .unwrap_or_else(|| {
                if routing.retryable_status_codes.is_empty() {
                    DEFAULT_RETRYABLE_STATUS_CODES.to_vec()
                } else {
                    routing.retryable_status_codes.clone()
                }
            })
            .into_iter()
            .collect::<HashSet<_>>();

        Self {
            policy,
            primary_retry_max,
            primary_retry_backoff_ms,
            primary_retry_max_backoff_ms,
            retryable_status_codes,
            fallback_cooldown_secs,
            fallback_max_per_min,
        }
    }

    fn is_retryable_status(&self, status: u16) -> bool {
        self.retryable_status_codes.contains(&status)
    }
}

#[derive(Debug)]
struct FallbackBudgetState {
    opened_until: Option<Instant>,
    window_start: Instant,
    window_events: VecDeque<Instant>,
}

impl FallbackBudgetState {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            opened_until: None,
            window_start: now,
            window_events: VecDeque::new(),
        }
    }

    fn allow_and_record(&mut self, cfg: &FailoverRuntime) -> Result<(), &'static str> {
        let now = Instant::now();
        if let Some(until) = self.opened_until {
            if now < until {
                return Err("cooldown_active");
            }
            self.opened_until = None;
        }

        if now.duration_since(self.window_start).as_secs() >= 60 {
            self.window_start = now;
            self.window_events.clear();
        }
        while let Some(ts) = self.window_events.front() {
            if now.duration_since(*ts).as_secs() >= 60 {
                self.window_events.pop_front();
            } else {
                break;
            }
        }
        if self.window_events.len() >= cfg.fallback_max_per_min {
            return Err("budget_exhausted");
        }

        self.window_events.push_back(now);
        Ok(())
    }

    fn trip_cooldown(&mut self, cfg: &FailoverRuntime) {
        self.opened_until = Some(Instant::now() + Duration::from_secs(cfg.fallback_cooldown_secs));
    }
}

#[derive(Debug)]
struct RetryableJobError {
    reason: String,
}

impl fmt::Display for RetryableJobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl std::error::Error for RetryableJobError {}

#[derive(Debug)]
struct NonRetryableJobError {
    reason: String,
}

impl fmt::Display for NonRetryableJobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl std::error::Error for NonRetryableJobError {}

fn env_u64(key: &str) -> Option<u64> {
    env::var(key).ok()?.trim().parse::<u64>().ok()
}

fn env_usize(key: &str) -> Option<usize> {
    env::var(key).ok()?.trim().parse::<usize>().ok()
}

fn env_csv_u16(key: &str) -> Option<Vec<u16>> {
    let raw = env::var(key).ok()?;
    let mut values = Vec::new();
    for token in raw.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(v) = trimmed.parse::<u16>() {
            values.push(v);
        }
    }
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let cfg: WorkerConfig = config_core::load_config("config/llm-worker.json")?;
    let failover = FailoverRuntime::from_routing(&cfg.routing);
    info!(
        target = "svc-llm-worker",
        queue = %cfg.queue_name,
        channel = %cfg.cache_channel,
        stream = %cfg.stream,
        failover_policy = failover.policy.as_str(),
        primary_retry_max = failover.primary_retry_max,
        fallback_cooldown_secs = failover.fallback_cooldown_secs,
        fallback_max_per_min = failover.fallback_max_per_min,
        "LLM worker initialized"
    );

    let router = match cfg.resolve_router() {
        Ok(p) => p,
        Err(err) => {
            return Err(err.context("failed to resolve LLM providers"));
        }
    };

    let provider_catalog = router.catalog();

    let metrics_host = cfg.metrics_host.clone();
    let metrics_port = cfg.metrics_port;
    let catalog_for_metrics = provider_catalog.clone();
    tokio::spawn(async move {
        if let Err(err) =
            metrics::serve_metrics(&metrics_host, metrics_port, catalog_for_metrics).await
        {
            error!(target = "svc-llm-worker", %err, "metrics server exited");
        }
    });

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&cfg.database_url)
        .await?;

    let taxonomy =
        Arc::new(TaxonomyStore::load_default().context("failed to load canonical taxonomy")?);
    let (activation_state, activation_refresh_enabled) = match ActivationState::load(&pool).await {
        Ok(state) => (state, true),
        Err(err) => {
            warn!(
                target = "svc-llm-worker",
                %err,
                "failed to load taxonomy activation profile; defaulting to allow-all"
            );
            (ActivationState::allow_all(), false)
        }
    };
    let activation = Arc::new(activation_state);
    if activation_refresh_enabled {
        ActivationState::spawn_refresh_task(Arc::clone(&activation), pool.clone());
    }

    let cache_listener = CacheListener::new(&cfg.redis_url, &cfg.cache_channel).await?;
    tokio::spawn(cache_listener.run());

    let job_consumer =
        JobConsumer::new(&cfg, router, pool.clone(), taxonomy.clone(), activation, failover).await?;
    tokio::spawn(job_consumer.run());

    signal::ctrl_c().await?;
    Ok(())
}

fn init_tracing() -> Result<()> {
    let root = env::var("OD_LOG_DIR").unwrap_or_else(|_| "logs".to_string());
    let log_dir = PathBuf::from(root).join("llm-worker");
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("failed to create log directory {}", log_dir.display()))?;

    let file_path = log_dir.join("llm-worker.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .with_context(|| format!("failed to open log file {}", file_path.display()))?;

    let stdout_layer = tracing_subscriber::fmt::layer().json();
    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_ansi(false)
        .with_writer(move || {
            file.try_clone()
                .expect("failed to clone llm-worker log file handle")
        });
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(stdout_layer)
        .with(file_layer)
        .init();
    info!(target = "svc-llm-worker", path = %file_path.display(), "file logging enabled");
    Ok(())
}

struct CacheListener {
    redis_url: String,
    channel: String,
}

struct DecisionCachePublisher {
    client: redis::Client,
    channel: String,
}

impl DecisionCachePublisher {
    async fn new(redis_url: &str, channel: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url.to_string())?;
        Ok(Self {
            client,
            channel: channel.to_string(),
        })
    }

    async fn publish(&self, key: &str, decision: &PolicyDecision, ttl_secs: u64) -> Result<()> {
        let mut conn = self.client.get_async_connection().await?;
        let payload = serde_json::to_string(decision)?;
        redis::cmd("SETEX")
            .arg(key)
            .arg(ttl_secs)
            .arg(&payload)
            .query_async::<_, ()>(&mut conn)
            .await?;
        let invalidate = serde_json::json!({
            "kind": "review",
            "normalized_key": key,
        })
        .to_string();
        redis::cmd("PUBLISH")
            .arg(&self.channel)
            .arg(invalidate)
            .query_async::<_, ()>(&mut conn)
            .await?;
        Ok(())
    }
}

struct JobConsumer {
    redis_url: String,
    stream: String,
    page_fetch_stream: String,
    pool: PgPool,
    router: ProviderRouter,
    cache_publisher: DecisionCachePublisher,
    taxonomy: Arc<TaxonomyStore>,
    taxonomy_prompt: String,
    activation: Arc<ActivationState>,
    failover: FailoverRuntime,
    fallback_budget: Mutex<FallbackBudgetState>,
}

impl JobConsumer {
    async fn new(
        cfg: &WorkerConfig,
        router: ProviderRouter,
        pool: PgPool,
        taxonomy: Arc<TaxonomyStore>,
        activation: Arc<ActivationState>,
        failover: FailoverRuntime,
    ) -> Result<Self> {
        let cache_publisher =
            DecisionCachePublisher::new(&cfg.redis_url, &cfg.cache_channel).await?;
        let taxonomy_prompt = build_taxonomy_prompt(taxonomy.as_ref());
        Ok(Self {
            redis_url: cfg.redis_url.clone(),
            stream: cfg.stream.clone(),
            page_fetch_stream: cfg.page_fetch_stream.clone(),
            pool,
            router,
            cache_publisher,
            taxonomy,
            taxonomy_prompt,
            activation,
            failover,
            fallback_budget: Mutex::new(FallbackBudgetState::new()),
        })
    }

    async fn run(self) {
        loop {
            if let Err(err) = self.consume().await {
                error!(target = "svc-llm-worker", %err, "job consumer error");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }

    async fn consume(&self) -> Result<(), redis::RedisError> {
        let client = redis::Client::open(self.redis_url.clone())?;
        let mut conn = client.get_async_connection().await?;
        let options = StreamReadOptions::default().block(5000).count(10);
        let mut last_id = "$".to_string();
        loop {
            let reply: StreamReadReply = conn
                .xread_options(&[&self.stream], &[last_id.as_str()], &options)
                .await?;
            for stream in reply.keys {
                for entry in stream.ids {
                    last_id = entry.id.clone();
                    if entry_too_old(&entry.id, 300_000) {
                        continue;
                    }
                    if let Some(payload) = entry.get::<String>("payload") {
                        if let Err(err) = self.process_job(&payload).await {
                            error!(target = "svc-llm-worker", %err, "failed to process job");
                        }
                    }
                }
            }
        }
    }

    async fn process_job(&self, payload: &str) -> Result<(), anyhow::Error> {
        metrics::record_job_started();
        let job_hint = serde_json::from_str::<ClassificationJobPayload>(payload).ok();
        let result = self.handle_job(payload).await;
        match &result {
            Ok(_) => metrics::record_job_completed(),
            Err(err) => {
                let requires_content = job_hint
                    .as_ref()
                    .map(|job| job.requires_content)
                    .unwrap_or(false);
                let should_requeue = err.downcast_ref::<ContentNotReady>().is_some()
                    || err.downcast_ref::<RetryableJobError>().is_some();
                if should_requeue {
                    warn!(
                        target = "svc-llm-worker",
                        requires_content,
                        normalized_key = job_hint
                            .as_ref()
                            .map(|job| job.normalized_key.as_str())
                            .unwrap_or("unknown"),
                        err = %err,
                        "classification job will be requeued"
                    );
                    if let Err(requeue_err) = self.requeue(payload).await {
                        error!(
                            target = "svc-llm-worker",
                            %requeue_err,
                            "failed to requeue pending job"
                        );
                    }
                } else {
                    if let Some(job) = job_hint.as_ref() {
                        let _ = self
                            .mark_pending(job, "failed", Some(&err.to_string()))
                            .await;
                    }
                    warn!(
                        target = "svc-llm-worker",
                        normalized_key = job_hint
                            .as_ref()
                            .map(|job| job.normalized_key.as_str())
                            .unwrap_or("unknown"),
                        err = %err,
                        "classification job failed without requeue"
                    );
                    metrics::record_job_failed();
                }
            }
        }
        result
    }

    async fn requeue(&self, payload: &str) -> Result<(), redis::RedisError> {
        let client = redis::Client::open(self.redis_url.clone())?;
        let mut conn = client.get_async_connection().await?;
        redis::cmd("XADD")
            .arg(&self.stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async::<_, ()>(&mut conn)
            .await?;

        if let Ok(job) = serde_json::from_str::<ClassificationJobPayload>(payload) {
            if job.requires_content {
                let fetch_job = PageFetchJob {
                    normalized_key: job.normalized_key,
                    url: job.full_url,
                    hostname: job.hostname,
                    trace_id: Some(job.trace_id),
                    ttl_seconds: None,
                };
                if let Ok(fetch_payload) = serde_json::to_string(&fetch_job) {
                    let _ = redis::cmd("XADD")
                        .arg(&self.page_fetch_stream)
                        .arg("*")
                        .arg("payload")
                        .arg(fetch_payload)
                        .query_async::<_, ()>(&mut conn)
                        .await;
                }
            }
        }

        Ok(())
    }

    async fn handle_job(&self, payload: &str) -> Result<(), anyhow::Error> {
        let mut job: ClassificationJobPayload = serde_json::from_str(payload)?;
        if job.requires_content {
            self.mark_pending(&job, "waiting_content", None).await?;
        }
        self.enrich_job_with_content(&mut job).await?;
        let mut provider_name = String::new();
        let mut selected_verdict: Option<LlmResponse> = None;
        let mut selected_fallback_reason = None;

        for attempt in 1..=NON_CANONICAL_RETRY_ATTEMPTS {
            let retry_instruction = if attempt > 1 {
                Some("Previous response used non-canonical taxonomy labels. Retry and return ONLY canonical IDs listed in Allowed Taxonomy IDs.")
            } else {
                None
            };
            let prompt = build_prompt(&job, &self.taxonomy_prompt, retry_instruction);
            let (raw_verdict, provider) = self.invoke_with_fallback(&job, &prompt).await?;
            provider_name = provider;

            let raw_category = raw_verdict.primary_category.clone();
            let raw_subcategory = raw_verdict.subcategory.clone();
            let (canonical_verdict, fallback_reason) = self.apply_taxonomy(raw_verdict);

            if let Some(reason) = fallback_reason {
                metrics::record_taxonomy_fallback(reason.as_str());
                warn!(
                    target = "svc-llm-worker",
                    attempt,
                    max_attempts = NON_CANONICAL_RETRY_ATTEMPTS,
                    reason = reason.as_str(),
                    provider = %provider_name,
                    normalized_key = %job.normalized_key,
                    original_category = %raw_category,
                    original_subcategory = %raw_subcategory,
                    canonical_category = %canonical_verdict.primary_category,
                    canonical_subcategory = %canonical_verdict.subcategory,
                    "non-canonical taxonomy labels detected"
                );
                selected_verdict = Some(canonical_verdict);
                selected_fallback_reason = Some(reason);
                if attempt < NON_CANONICAL_RETRY_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    continue;
                }
            } else {
                selected_verdict = Some(canonical_verdict);
                selected_fallback_reason = None;
            }
            break;
        }

        let mut verdict =
            selected_verdict.ok_or_else(|| anyhow!("missing classification verdict"))?;
        let fallback_reason = selected_fallback_reason;
        let recommended_action = parse_policy_action(&verdict.recommended_action)?;
        let activation_allowed = self
            .activation
            .is_enabled(&verdict.primary_category, Some(&verdict.subcategory));
        let activation_blocked = !activation_allowed;
        let final_action = if activation_allowed {
            recommended_action
        } else {
            metrics::record_activation_block();
            warn!(
                target = "svc-llm-worker",
                category = %verdict.primary_category,
                subcategory = %verdict.subcategory,
                "taxonomy activation blocked verdict"
            );
            PolicyAction::Block
        };
        verdict.recommended_action = final_action.to_string();
        let taxonomy_version = self.taxonomy.taxonomy().version.clone();
        let action = store_classification(
            &self.pool,
            &job,
            &verdict,
            &taxonomy_version,
            fallback_reason,
            final_action,
            activation_blocked,
        )
        .await
        .context("failed to persist classification")?;
        self.publish_cache_entry(&job, &verdict, action.clone())
            .await
            .context("failed to publish cache entry")?;
        self.clear_pending(&job.normalized_key).await?;
        info!(
            target = "svc-llm-worker",
            key = job.normalized_key,
            action = ?action,
            provider = provider_name,
            "classification stored"
        );
        Ok(())
    }

    fn apply_taxonomy(&self, verdict: LlmResponse) -> (LlmResponse, Option<FallbackReason>) {
        let sub_input = if verdict.subcategory.trim().is_empty() {
            None
        } else {
            Some(verdict.subcategory.as_str())
        };
        let validated = self
            .taxonomy
            .validate_labels(&verdict.primary_category, sub_input);

        let canonical = LlmResponse {
            primary_category: validated.category.id.clone(),
            subcategory: validated.subcategory.id.clone(),
            risk_level: verdict.risk_level,
            confidence: verdict.confidence,
            recommended_action: verdict.recommended_action,
        };

        (canonical, validated.fallback_reason)
    }

    async fn publish_cache_entry(
        &self,
        job: &ClassificationJobPayload,
        verdict: &LlmResponse,
        action: PolicyAction,
    ) -> Result<()> {
        let cache_entry = PolicyDecision {
            action: action.clone(),
            cache_hit: true,
            verdict: Some(ClassificationVerdict {
                primary_category: verdict.primary_category.clone(),
                subcategory: verdict.subcategory.clone(),
                risk_level: verdict.risk_level.clone(),
                confidence: verdict.confidence,
                recommended_action: action,
            }),
        };
        self.cache_publisher
            .publish(&job.normalized_key, &cache_entry, CACHE_TTL_SECONDS)
            .await?;
        Ok(())
    }

    async fn mark_pending(
        &self,
        job: &ClassificationJobPayload,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let base_url = job.base_url.clone().or_else(|| Some(job.full_url.clone()));
        sqlx::query(
            r#"
            INSERT INTO classification_requests (normalized_key, status, base_url, last_error)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (normalized_key)
            DO UPDATE SET
                status = EXCLUDED.status,
                base_url = COALESCE(EXCLUDED.base_url, classification_requests.base_url),
                last_error = EXCLUDED.last_error,
                updated_at = NOW()
            "#,
        )
        .bind(&job.normalized_key)
        .bind(status)
        .bind(base_url)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn clear_pending(&self, normalized_key: &str) -> Result<()> {
        sqlx::query("DELETE FROM classification_requests WHERE normalized_key = $1")
            .bind(normalized_key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn invoke_with_fallback(
        &self,
        job: &ClassificationJobPayload,
        prompt: &str,
    ) -> Result<(LlmResponse, String)> {
        let primary = self.router.primary();
        let primary_failure = self.invoke_primary_with_retry(primary, job, prompt).await;
        match primary_failure {
            Ok(response) => return Ok((response, primary.name.clone())),
            Err(err) => {
                error!(
                    target = "svc-llm-worker",
                    normalized_key = %job.normalized_key,
                    provider = %primary.name,
                    class = ?err.class,
                    status = ?err.status,
                    reason = %err.reason,
                    policy = self.failover.policy.as_str(),
                    "primary provider failed"
                );

                if !self.should_attempt_fallback(&err) {
                    metrics::record_fallback_skipped("policy_or_error_class");
                    let reason = format!(
                        "primary provider failed and fallback skipped: {}",
                        err.reason
                    );
                    return match err.class {
                        InvocationFailureClass::Retryable => {
                            Err(RetryableJobError { reason }.into())
                        }
                        InvocationFailureClass::NonRetryable => {
                            Err(NonRetryableJobError { reason }.into())
                        }
                    };
                }

                let fallback = if let Some(provider) = self.router.fallback() {
                    provider
                } else {
                    metrics::record_fallback_skipped("fallback_not_configured");
                    let reason = format!("primary failed and fallback not configured: {}", err.reason);
                    return Err(RetryableJobError { reason }.into());
                };

                {
                    let mut budget = self.fallback_budget.lock().await;
                    if let Err(blocked_reason) = budget.allow_and_record(&self.failover) {
                        metrics::record_fallback_skipped(blocked_reason);
                        let reason = format!(
                            "fallback blocked by {} after primary failure: {}",
                            blocked_reason, err.reason
                        );
                        return Err(RetryableJobError { reason }.into());
                    }
                }

                metrics::record_fallback_attempt(&primary.name, &fallback.name, "primary_failed");
                info!(
                    target = "svc-llm-worker",
                    normalized_key = %job.normalized_key,
                    from_provider = %primary.name,
                    to_provider = %fallback.name,
                    reason = %err.reason,
                    "attempting provider fallback"
                );

                match invoke_llm(fallback, job, prompt).await {
                    Ok(response) => {
                        info!(
                            target = "svc-llm-worker",
                            normalized_key = %job.normalized_key,
                            provider = %fallback.name,
                            "fallback provider succeeded"
                        );
                        Ok((response, fallback.name.clone()))
                    }
                    Err(fallback_err) => {
                        let classified = classify_invocation_failure(&fallback_err, &self.failover);
                        error!(
                            target = "svc-llm-worker",
                            normalized_key = %job.normalized_key,
                            provider = %fallback.name,
                            class = ?classified.class,
                            status = ?classified.status,
                            reason = %classified.reason,
                            "fallback provider failed"
                        );
                        let mut budget = self.fallback_budget.lock().await;
                        budget.trip_cooldown(&self.failover);
                        let reason = format!(
                            "fallback provider failed after primary error: {}",
                            classified.reason
                        );
                        match classified.class {
                            InvocationFailureClass::Retryable => {
                                Err(RetryableJobError { reason }.into())
                            }
                            InvocationFailureClass::NonRetryable => {
                                Err(NonRetryableJobError { reason }.into())
                            }
                        }
                    }
                }
            }
        }
    }

    async fn invoke_primary_with_retry(
        &self,
        provider: &ResolvedProvider,
        job: &ClassificationJobPayload,
        prompt: &str,
    ) -> Result<LlmResponse, InvocationFailure> {
        let max_attempts = self.failover.primary_retry_max.max(1);
        let mut attempt = 1usize;
        loop {
            match invoke_llm(provider, job, prompt).await {
                Ok(response) => {
                    if attempt > 1 {
                        info!(
                            target = "svc-llm-worker",
                            normalized_key = %job.normalized_key,
                            provider = %provider.name,
                            attempt,
                            "primary provider succeeded after retry"
                        );
                    }
                    return Ok(response);
                }
                Err(err) => {
                    let failure = classify_invocation_failure(&err, &self.failover);
                    error!(
                        target = "svc-llm-worker",
                        normalized_key = %job.normalized_key,
                        provider = %provider.name,
                        attempt,
                        max_attempts,
                        class = ?failure.class,
                        status = ?failure.status,
                        reason = %failure.reason,
                        "llm invocation failed"
                    );

                    if attempt >= max_attempts || failure.class == InvocationFailureClass::NonRetryable
                    {
                        if attempt >= max_attempts {
                            metrics::record_primary_retry_exhausted(&provider.name, "attempt_limit");
                        }
                        return Err(failure);
                    }

                    let backoff = calculate_retry_backoff_ms(&self.failover, attempt);
                    metrics::record_primary_retry(&provider.name, "retryable_error");
                    warn!(
                        target = "svc-llm-worker",
                        normalized_key = %job.normalized_key,
                        provider = %provider.name,
                        attempt,
                        next_attempt = attempt + 1,
                        backoff_ms = backoff,
                        "scheduling primary provider retry"
                    );
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                    attempt += 1;
                }
            }
        }
    }

    fn should_attempt_fallback(&self, failure: &InvocationFailure) -> bool {
        match self.failover.policy {
            FailoverPolicy::Disabled => false,
            FailoverPolicy::Aggressive => true,
            FailoverPolicy::Safe => failure.class == InvocationFailureClass::Retryable,
        }
    }

    async fn enrich_job_with_content(&self, job: &mut ClassificationJobPayload) -> Result<()> {
        if job.requires_content {
            let snippet = self.await_page_content(&job.normalized_key).await?;
            if let Some(snippet) = snippet {
                job.content_excerpt = snippet.content_excerpt;
                job.content_hash = snippet.content_hash;
                job.content_version = snippet.content_version;
                job.content_language = snippet.content_language;
            }
            return Ok(());
        }

        if has_content(&job.content_excerpt)
            && job.content_hash.is_some()
            && job.content_version.is_some()
        {
            return Ok(());
        }

        if let Some(snippet) = self.load_page_content(&job.normalized_key).await? {
            if !has_content(&job.content_excerpt) {
                job.content_excerpt = snippet.content_excerpt;
            }
            if job.content_hash.is_none() {
                job.content_hash = snippet.content_hash;
            }
            if job.content_version.is_none() {
                job.content_version = snippet.content_version;
            }
            if job.content_language.is_none() {
                job.content_language = snippet.content_language;
            }
        }

        Ok(())
    }

    async fn load_page_content(&self, normalized_key: &str) -> Result<Option<PageContentSnippet>> {
        let row = sqlx::query(
            r#"
            SELECT text_excerpt, content_hash, fetch_version
            FROM page_contents
            WHERE normalized_key = $1
              AND expires_at > NOW()
            ORDER BY fetch_version DESC
            LIMIT 1
            "#,
        )
        .bind(normalized_key)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let content_excerpt: Option<String> = row.try_get("text_excerpt")?;
            let content_hash: Option<String> = row.try_get("content_hash")?;
            let fetch_version: i32 = row.try_get("fetch_version")?;
            Ok(Some(PageContentSnippet {
                content_excerpt,
                content_hash,
                content_version: Some(i64::from(fetch_version)),
                content_language: None,
            }))
        } else {
            Ok(None)
        }
    }

    async fn await_page_content(&self, normalized_key: &str) -> Result<Option<PageContentSnippet>> {
        for _ in 0..CONTENT_WAIT_ATTEMPTS {
            if let Some(snippet) = self.load_page_content(normalized_key).await? {
                return Ok(Some(snippet));
            }
            tokio::time::sleep(std::time::Duration::from_secs(CONTENT_WAIT_DELAY_SECS)).await;
        }
        Err(ContentNotReady.into())
    }
}

fn entry_too_old(entry_id: &str, max_age_ms: u64) -> bool {
    let Some((millis, _)) = entry_id.split_once('-') else {
        return false;
    };
    let Ok(ts_ms) = millis.parse::<u64>() else {
        return false;
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(ts_ms);
    now_ms.saturating_sub(ts_ms) > max_age_ms
}

impl CacheListener {
    async fn new(redis_url: &str, channel: &str) -> Result<Self> {
        Ok(Self {
            redis_url: redis_url.to_string(),
            channel: channel.to_string(),
        })
    }

    async fn run(self) {
        loop {
            match redis::Client::open(self.redis_url.clone()) {
                Ok(client) => {
                    if let Err(err) = self.listen(client).await {
                        error!(target = "svc-llm-worker", %err, "cache listener error");
                    }
                }
                Err(err) => error!(target = "svc-llm-worker", %err, "failed to connect to redis"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    async fn listen(&self, client: redis::Client) -> Result<(), redis::RedisError> {
        let conn = client.get_async_connection().await?;
        let mut pubsub = conn.into_pubsub();
        pubsub.subscribe(&self.channel).await?;
        let mut stream = pubsub.on_message();
        while let Some(msg) = stream.next().await {
            let payload: String = msg.get_payload()?;
            info!(
                target = "svc-llm-worker",
                event = payload,
                "cache invalidation received"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ClassificationJobPayload {
    normalized_key: String,
    entity_level: String,
    hostname: String,
    full_url: String,
    trace_id: String,
    #[serde(default)]
    requires_content: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_language: Option<String>,
}

#[derive(Debug)]
struct PageContentSnippet {
    content_excerpt: Option<String>,
    content_hash: Option<String>,
    content_version: Option<i64>,
    content_language: Option<String>,
}

fn has_content(value: &Option<String>) -> bool {
    value
        .as_ref()
        .map(|text| !text.trim().is_empty())
        .unwrap_or(false)
}

fn parse_policy_action(value: &str) -> Result<PolicyAction> {
    match value {
        "Allow" => Ok(PolicyAction::Allow),
        "Block" => Ok(PolicyAction::Block),
        "Warn" => Ok(PolicyAction::Warn),
        "Monitor" => Ok(PolicyAction::Monitor),
        "Review" => Ok(PolicyAction::Review),
        "RequireApproval" => Ok(PolicyAction::RequireApproval),
        other => Err(anyhow!("invalid action {other}")),
    }
}

async fn store_classification(
    pool: &PgPool,
    job: &ClassificationJobPayload,
    verdict: &schema::LlmResponse,
    taxonomy_version: &str,
    fallback_reason: Option<FallbackReason>,
    final_action: PolicyAction,
    activation_blocked: bool,
) -> Result<PolicyAction> {
    let new_id = Uuid::new_v4();
    let ttl_seconds = 3600;
    let sfw = matches!(final_action, PolicyAction::Allow);
    let mut flags_map = Map::new();
    flags_map.insert("source".to_string(), Value::from("llm-worker"));
    if let Some(reason) = fallback_reason {
        flags_map.insert(
            "taxonomy_fallback_reason".to_string(),
            Value::from(reason.as_str()),
        );
    }
    if activation_blocked {
        flags_map.insert(
            "taxonomy_activation_state".to_string(),
            Value::from("blocked"),
        );
    }
    let flags = Value::Object(flags_map);

    let row = sqlx::query(
        r#"INSERT INTO classifications
            (id, normalized_key, taxonomy_version, model_version, primary_category, subcategory,
             risk_level, recommended_action, confidence, sfw, flags, ttl_seconds, status, next_refresh_at)
            VALUES ($1, $2, $3, 'llm-sim', $4, $5, $6, $7, $8, $9, $10, $11, 'active', NOW() + INTERVAL '4 hours')
            ON CONFLICT (normalized_key)
            DO UPDATE SET
                primary_category = EXCLUDED.primary_category,
                subcategory = EXCLUDED.subcategory,
                risk_level = EXCLUDED.risk_level,
                recommended_action = EXCLUDED.recommended_action,
                confidence = EXCLUDED.confidence,
                sfw = EXCLUDED.sfw,
                flags = EXCLUDED.flags,
                ttl_seconds = EXCLUDED.ttl_seconds,
                updated_at = NOW()
            RETURNING id"#,
    )
    .bind(new_id)
    .bind(&job.normalized_key)
    .bind(taxonomy_version)
    .bind(&verdict.primary_category)
    .bind(&verdict.subcategory)
    .bind(&verdict.risk_level)
    .bind(final_action.to_string())
    .bind(verdict.confidence as f64)
    .bind(sfw)
    .bind(flags)
    .bind(ttl_seconds)
    .fetch_one(pool)
    .await?;

    let classification_id: Uuid = row.get("id");
    let current_version: i64 = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT MAX(version) FROM classification_versions WHERE classification_id = $1",
    )
    .bind(classification_id)
    .fetch_one(pool)
    .await?
    .unwrap_or(0) as i64;
    let next_version = current_version + 1;

    sqlx::query(
        "INSERT INTO classification_versions (id, classification_id, version, changed_by, reason, payload)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(classification_id)
    .bind(next_version)
    .bind(Some("llm-worker".to_string()))
    .bind(Some("automated".to_string()))
    .bind(json!({
        "normalized_key": job.normalized_key,
        "category": verdict.primary_category,
        "action": final_action,
    }))
    .execute(pool)
        .await?;

    Ok(final_action)
}

async fn invoke_llm(
    provider: &ResolvedProvider,
    job: &ClassificationJobPayload,
    prompt: &str,
) -> Result<LlmResponse> {
    let (head_chars, title_chars, body_chars, total_chars) = html_context_lengths(job_excerpt(job));
    info!(
        target = "svc-llm-worker",
        normalized_key = %job.normalized_key,
        provider = %provider.name,
        requires_content = job.requires_content,
        html_context_present = total_chars > 0,
        head_chars,
        title_chars,
        body_chars,
        html_context_chars = total_chars,
        content_hash = ?job.content_hash,
        content_version = ?job.content_version,
        "invoking llm provider"
    );
    metrics::record_llm_invocation();
    metrics::record_provider_invocation(&provider.name);
    let start = Instant::now();
    let result = match provider.kind {
        ProviderKind::Ollama => invoke_ollama(provider, prompt).await,
        ProviderKind::LmStudio => invoke_lmstudio_chat(provider, prompt).await,
        ProviderKind::Openai | ProviderKind::Vllm | ProviderKind::OpenaiCompatible => {
            invoke_openai_chat(provider, prompt).await
        }
        ProviderKind::Anthropic => invoke_anthropic(provider, prompt).await,
        ProviderKind::CustomJson => invoke_custom_json(provider, job).await,
    };

    match result {
        Ok(response) => {
            let elapsed = start.elapsed().as_secs_f64();
            metrics::observe_llm_latency(elapsed);
            metrics::observe_provider_latency(&provider.name, elapsed);
            Ok(response)
        }
        Err(err) => {
            if err
                .downcast_ref::<reqwest::Error>()
                .map(|e| e.is_timeout())
                .unwrap_or(false)
            {
                metrics::record_llm_timeout();
                metrics::record_provider_timeout(&provider.name);
            }
            metrics::record_llm_failure();
            metrics::record_provider_failure(&provider.name);
            Err(err)
        }
    }
}

fn classify_invocation_failure(err: &anyhow::Error, failover: &FailoverRuntime) -> InvocationFailure {
    if let Some(req_err) = err.downcast_ref::<reqwest::Error>() {
        if req_err.is_timeout() || req_err.is_connect() || req_err.is_request() {
            return InvocationFailure {
                class: InvocationFailureClass::Retryable,
                status: None,
                reason: req_err.to_string(),
            };
        }
        if let Some(status) = req_err.status() {
            let status_u16 = status.as_u16();
            let class = if failover.is_retryable_status(status_u16) {
                InvocationFailureClass::Retryable
            } else {
                InvocationFailureClass::NonRetryable
            };
            return InvocationFailure {
                class,
                status: Some(status_u16),
                reason: req_err.to_string(),
            };
        }
    }

    let msg = err.to_string();
    let lowered = msg.to_ascii_lowercase();
    let class = if lowered.contains("timed out")
        || lowered.contains("connection refused")
        || lowered.contains("temporarily unavailable")
    {
        InvocationFailureClass::Retryable
    } else {
        InvocationFailureClass::NonRetryable
    };
    InvocationFailure {
        class,
        status: None,
        reason: msg,
    }
}

fn calculate_retry_backoff_ms(cfg: &FailoverRuntime, attempt: usize) -> u64 {
    let base = cfg.primary_retry_backoff_ms.max(1);
    let exp = 2u64.saturating_pow((attempt.saturating_sub(1)).min(10) as u32);
    let raw = base.saturating_mul(exp);
    raw.min(cfg.primary_retry_max_backoff_ms.max(base))
}

async fn invoke_custom_json(
    provider: &ResolvedProvider,
    job: &ClassificationJobPayload,
) -> Result<LlmResponse> {
    let client = Client::new();
    let payload = PromptPayload {
        normalized_key: &job.normalized_key,
        hostname: &job.hostname,
        full_url: &job.full_url,
        entity_level: &job.entity_level,
        trace_id: &job.trace_id,
        content_excerpt: job_excerpt(job),
        content_hash: job.content_hash.as_deref(),
        content_version: job.content_version,
    };
    let mut request = client.post(&provider.endpoint);
    if !provider.api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", provider.api_key));
    }
    request = apply_provider_headers(request, provider);
    request = apply_timeout(request, provider);
    let response = request.json(&payload).send().await?;
    let response = response.error_for_status()?;
    let verdict = response.json::<LlmResponse>().await.map_err(|err| {
        metrics::record_invalid_response();
        err
    })?;
    verdict.normalize().map_err(|err| {
        metrics::record_invalid_response();
        err
    })
}

async fn invoke_openai_chat(provider: &ResolvedProvider, prompt: &str) -> Result<LlmResponse> {
    let client = Client::new();
    let model = provider.model.as_deref().unwrap_or(OPENAI_DEFAULT_MODEL);
    let body = json!({
        "model": model,
        "temperature": 0.0,
        "response_format": {"type": "json_object"},
        "max_tokens": 256,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": prompt}
        ]
    });

    let mut request = client
        .post(&provider.endpoint)
        .header("Content-Type", "application/json");
    if !provider.api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", provider.api_key));
    }
    request = apply_provider_headers(request, provider);
    request = apply_timeout(request, provider);
    let response = request.json(&body).send().await?;
    let response = response.error_for_status()?;
    let payload: OpenAiChatResponse = response.json().await.map_err(|err| {
        metrics::record_invalid_response();
        err
    })?;
    let message = payload
        .choices
        .first()
        .ok_or_else(|| anyhow!("openai response missing choices"))?;
    let content = message.message.content_text()?;
    parse_llm_json_text(&content)
}

async fn invoke_lmstudio_chat(provider: &ResolvedProvider, prompt: &str) -> Result<LlmResponse> {
    let client = Client::new();
    let model = provider.model.as_deref().unwrap_or("lmstudio-model");
    let body = json!({
        "model": model,
        "temperature": 0.0,
        "stream": false,
        "max_tokens": 256,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": prompt}
        ]
    });

    let mut request = client
        .post(&provider.endpoint)
        .header("Content-Type", "application/json");
    if !provider.api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", provider.api_key));
    }
    request = apply_provider_headers(request, provider);
    request = apply_timeout(request, provider);
    let response = request.json(&body).send().await?;
    let response = response.error_for_status()?;
    let payload: OpenAiChatResponse = response.json().await.map_err(|err| {
        metrics::record_invalid_response();
        err
    })?;
    let message = payload
        .choices
        .first()
        .ok_or_else(|| anyhow!("lm studio response missing choices"))?;
    let content = message.message.content_text()?;
    parse_llm_json_text(&content)
}

async fn invoke_anthropic(provider: &ResolvedProvider, prompt: &str) -> Result<LlmResponse> {
    let client = Client::new();
    let model = provider
        .model
        .as_deref()
        .unwrap_or("claude-3-sonnet-20240229");
    let body = json!({
        "model": model,
        "max_tokens": 512,
        "temperature": 0.0,
        "system": SYSTEM_PROMPT,
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt}
                ]
            }
        ]
    });

    if provider.api_key.is_empty() {
        return Err(anyhow!("anthropic provider requires api_key"));
    }
    let mut request = client
        .post(&provider.endpoint)
        .header("content-type", "application/json")
        .header("x-api-key", &provider.api_key)
        .header("anthropic-version", ANTHROPIC_VERSION);
    request = apply_provider_headers(request, provider);
    request = apply_timeout(request, provider);
    let response = request.json(&body).send().await?;
    let response = response.error_for_status()?;
    let payload: AnthropicResponse = response.json().await.map_err(|err| {
        metrics::record_invalid_response();
        err
    })?;
    let text = payload.first_text()?;
    parse_llm_json_text(&text)
}

async fn invoke_ollama(provider: &ResolvedProvider, prompt: &str) -> Result<LlmResponse> {
    let client = Client::new();
    let model = provider.model.as_deref().unwrap_or("llama3");
    let prompt = format!("{}\n\n{}", SYSTEM_PROMPT, prompt);
    let body = json!({
        "model": model,
        "prompt": prompt,
        "stream": false
    });
    let mut request = client.post(&provider.endpoint);
    request = apply_provider_headers(request, provider);
    request = apply_timeout(request, provider);
    let response = request.json(&body).send().await?;
    let response = response.error_for_status()?;
    let payload: OllamaResponse = response.json().await.map_err(|err| {
        metrics::record_invalid_response();
        err
    })?;
    parse_llm_json_text(payload.response.trim())
}

fn build_prompt(
    job: &ClassificationJobPayload,
    taxonomy_prompt: &str,
    retry_instruction: Option<&str>,
) -> String {
    let mut sections = vec![format!(
        "Classify the following web request. Return JSON with fields: primary_category, subcategory, risk_level, confidence (0-1), recommended_action (Allow|Block|Warn|Monitor|Review|RequireApproval). Use ONLY canonical taxonomy IDs for primary_category and subcategory.\\nNormalized Key: {}\\nHostname: {}\\nURL: {}\\nEntity Level: {}\\nTrace ID: {}",
        job.normalized_key, job.hostname, job.full_url, job.entity_level, job.trace_id
    )];
    sections.push(format!("Allowed Taxonomy IDs:\n{}", taxonomy_prompt));

    if let Some(instruction) = retry_instruction {
        sections.push(format!("Retry Instruction: {instruction}"));
    }

    if let Some(excerpt) = job_excerpt(job) {
        let (formatted_excerpt, truncated) = format_html_context(excerpt);
        sections.push(format!(
            "Homepage HTML Context (head + title + body, {} chars{}):\\n{}",
            formatted_excerpt.chars().count(),
            if truncated { ", truncated" } else { "" },
            formatted_excerpt
        ));
    } else {
        sections.push(
            "Homepage HTML Context: unavailable (content fetch pending, failed, or disabled)."
                .into(),
        );
    }

    if let Some(hash) = job.content_hash.as_deref() {
        sections.push(format!("Content Hash: {hash}"));
    }
    if let Some(version) = job.content_version {
        sections.push(format!("Content Version: {version}"));
    }
    if let Some(language) = job.content_language.as_deref() {
        sections.push(format!("Content Language: {language}"));
    }

    sections.join("\\n")
}

fn format_excerpt(excerpt: &str) -> (String, bool) {
    let cleaned = excerpt.trim();
    let mut buffer = String::new();
    for ch in cleaned.chars().take(PROMPT_HTML_CONTEXT_LIMIT) {
        buffer.push(ch);
    }
    let total_chars = cleaned.chars().count();
    let truncated = total_chars > buffer.chars().count();
    if truncated {
        buffer.push('…');
    }
    (buffer, truncated)
}

fn format_html_context(html_context: &str) -> (String, bool) {
    format_excerpt(html_context)
}

fn build_taxonomy_prompt(store: &TaxonomyStore) -> String {
    let taxonomy = store.taxonomy();
    let mut lines = Vec::new();
    lines.push(format!("taxonomy_version: {}", taxonomy.version));
    for category in &taxonomy.categories {
        lines.push(format!("- {} ({})", category.id, category.name));
        let sub_ids = category
            .subcategories
            .iter()
            .map(|sub| format!("{} ({})", sub.id, sub.name))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("  subcategories: {sub_ids}"));
    }
    lines.join("\n")
}

fn html_context_lengths(value: Option<&str>) -> (usize, usize, usize, usize) {
    let Some(text) = value else {
        return (0, 0, 0, 0);
    };
    let head = section_length(text, "[HEAD]", "[/HEAD]");
    let title = section_length(text, "[TITLE]", "[/TITLE]");
    let body = section_length(text, "[BODY]", "[/BODY]");
    let total = text.chars().count();
    (head, title, body, total)
}

fn section_length(text: &str, start: &str, end: &str) -> usize {
    let Some(start_idx) = text.find(start) else {
        return 0;
    };
    let content_start = start_idx + start.len();
    let Some(end_rel) = text[content_start..].find(end) else {
        return 0;
    };
    text[content_start..content_start + end_rel].chars().count()
}

fn job_excerpt(job: &ClassificationJobPayload) -> Option<&str> {
    job.content_excerpt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

#[derive(Debug)]
struct ContentNotReady;

impl fmt::Display for ContentNotReady {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "page content not ready")
    }
}

impl std::error::Error for ContentNotReady {}

fn parse_llm_json_text(text: &str) -> Result<LlmResponse> {
    let mut cleaned = text.trim();
    if let Some(stripped) = cleaned.strip_prefix("```json") {
        cleaned = stripped.trim();
    } else if let Some(stripped) = cleaned.strip_prefix("```") {
        cleaned = stripped.trim();
    }
    if let Some(stripped) = cleaned.strip_suffix("```") {
        cleaned = stripped.trim();
    }

    if let Ok(parsed) = serde_json::from_str::<LlmResponse>(cleaned) {
        return parsed.normalize().map_err(|err| {
            metrics::record_invalid_response();
            err
        });
    }

    if let Some(candidate) = extract_balanced_json_object(cleaned) {
        let parsed = serde_json::from_str::<LlmResponse>(&candidate).map_err(|err| {
            metrics::record_invalid_response();
            err
        })?;
        return parsed.normalize().map_err(|err| {
            metrics::record_invalid_response();
            err
        });
    }

    metrics::record_invalid_response();
    Err(anyhow!("llm response did not contain valid JSON object"))
}

fn extract_balanced_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match b {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match b {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = Some(idx);
                }
                depth += 1;
            }
            b'}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(begin) = start {
                        let candidate = &text[begin..=idx];
                        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                            return Some(candidate.to_string());
                        }
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }
    None
}

fn apply_provider_headers(
    mut request: RequestBuilder,
    provider: &ResolvedProvider,
) -> RequestBuilder {
    for (key, value) in &provider.headers {
        request = request.header(key, value);
    }
    request
}

fn apply_timeout(mut request: RequestBuilder, provider: &ResolvedProvider) -> RequestBuilder {
    if let Some(ms) = provider.timeout_ms {
        request = request.timeout(Duration::from_millis(ms));
    }
    request
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: Value,
}

impl OpenAiMessage {
    fn content_text(&self) -> Result<String> {
        match &self.content {
            Value::String(text) => Ok(text.clone()),
            Value::Array(parts) => {
                let combined = parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n");
                if combined.is_empty() {
                    Err(anyhow!("openai response missing text content"))
                } else {
                    Ok(combined)
                }
            }
            _ => Err(anyhow!("unsupported OpenAI content type")),
        }
    }
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicBlock>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    text: Option<String>,
}

impl AnthropicResponse {
    fn first_text(&self) -> Result<String> {
        self.content
            .iter()
            .filter_map(|block| block.text.clone())
            .next()
            .ok_or_else(|| anyhow!("anthropic response missing text"))
    }
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Json, routing::post, Router};
    use portpicker::pick_unused_port;
    use serde_json::{json, Value};
    use std::{
        env,
        process::{Command, Stdio},
        sync::Arc,
    };
    use tokio::{
        net::TcpListener,
        task::JoinHandle,
        time::{sleep, timeout, Duration, Instant},
    };

    #[test]
    fn build_prompt_includes_excerpt() {
        let taxonomy = TaxonomyStore::load_default().expect("taxonomy");
        let taxonomy_prompt = build_taxonomy_prompt(&taxonomy);
        let job = ClassificationJobPayload {
            normalized_key: "url:https://example.com/".into(),
            entity_level: "url".into(),
            hostname: "example.com".into(),
            full_url: "https://example.com/".into(),
            trace_id: "trace-test".into(),
            requires_content: false,
            base_url: None,
            content_excerpt: Some("This is a captured page excerpt.".into()),
            content_hash: Some("abc123".into()),
            content_version: Some(2),
            content_language: Some("en".into()),
        };
        let prompt = build_prompt(&job, &taxonomy_prompt, None);
        assert!(prompt.contains("Allowed Taxonomy IDs"));
        assert!(prompt.contains("social-media"));
        assert!(prompt.contains("Homepage HTML Context"));
        assert!(prompt.contains("captured page excerpt"));
        assert!(prompt.contains("Content Hash: abc123"));
        assert!(prompt.contains("Content Version: 2"));
    }

    #[test]
    fn build_prompt_handles_missing_excerpt() {
        let taxonomy = TaxonomyStore::load_default().expect("taxonomy");
        let taxonomy_prompt = build_taxonomy_prompt(&taxonomy);
        let job = ClassificationJobPayload {
            normalized_key: "url:https://empty.example".into(),
            entity_level: "url".into(),
            hostname: "empty.example".into(),
            full_url: "https://empty.example".into(),
            trace_id: "trace-empty".into(),
            requires_content: false,
            base_url: None,
            content_excerpt: None,
            content_hash: None,
            content_version: None,
            content_language: None,
        };
        let prompt = build_prompt(&job, &taxonomy_prompt, None);
        assert!(prompt.contains("Homepage HTML Context: unavailable"));
    }

    #[test]
    fn parse_llm_json_text_handles_reasoning_wrappers() {
        let payload = r#"<|channel|>analysis<|message|>internal reasoning<|end|><|start|>assistant<|channel|>final<|message|>{
  "primary_category": "Social Media",
  "subcategory": "Photo/Video Sharing",
  "risk_level": "Low",
  "confidence": 0.99,
  "recommended_action": "Allow"
}"#;
        let parsed = parse_llm_json_text(payload).expect("parses wrapped json");
        assert_eq!(parsed.risk_level, "low");
        assert_eq!(parsed.recommended_action, "Allow");
    }

    #[test]
    fn extract_balanced_json_object_handles_braces_in_strings() {
        let payload = r#"noise {"note":"a } brace","value":1} trailing"#;
        let extracted = extract_balanced_json_object(payload).expect("extracts object");
        assert!(extracted.contains("\"value\":1"));
    }

    #[tokio::test]
    async fn processes_queue_job_and_persists_classification() -> Result<()> {
        let (redis_guard, redis_port) = start_redis_container()?;
        let redis_url = format!("redis://{}:{}/", test_host(), redis_port);
        wait_for_redis(&redis_url).await?;

        let (postgres_guard, pg_port) = start_postgres_container()?;
        let database_url = format!(
            "postgres://postgres:postgres@{}:{}/postgres",
            test_host(),
            pg_port
        );
        let pool = connect_postgres(&database_url).await?;
        apply_migrations(&pool).await;

        let (llm_endpoint, server_task) = spawn_mock_llm().await;

        let cfg = WorkerConfig {
            queue_name: "classification-jobs".into(),
            redis_url: redis_url.clone(),
            cache_channel: "od:cache:invalidate".into(),
            stream: "classification-jobs".into(),
            page_fetch_stream: "page-fetch-jobs".into(),
            database_url: database_url.clone(),
            llm_endpoint: Some(llm_endpoint.clone()),
            llm_api_key: Some("test-key".into()),
            providers: Vec::new(),
            routing: RoutingConfig::default(),
            metrics_host: "127.0.0.1".into(),
            metrics_port: 0,
        };

        let router = cfg.resolve_router().unwrap();
        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let activation = Arc::new(ActivationState::allow_all());
        let failover = FailoverRuntime::from_routing(&cfg.routing);
        let consumer = JobConsumer::new(&cfg, router, pool.clone(), taxonomy, activation, failover)
            .await
            .unwrap();
        let consumer_handle = tokio::spawn(async move { consumer.run().await });

        let job = ClassificationJobPayload {
            normalized_key: "domain:integration.test".into(),
            entity_level: "domain".into(),
            hostname: "integration.test".into(),
            full_url: "https://integration.test/".into(),
            trace_id: "trace-123".into(),
            requires_content: false,
            base_url: None,
            content_excerpt: None,
            content_hash: None,
            content_version: None,
            content_language: None,
        };

        sleep(Duration::from_millis(500)).await;
        enqueue_job(&redis_url, &job).await.expect("enqueue job");

        timeout(Duration::from_secs(30), async {
            loop {
                if classification_exists(&pool, &job.normalized_key)
                    .await
                    .expect("query classification")
                {
                    break;
                }
                sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .expect("classification persisted");

        consumer_handle.abort();
        server_task.abort();
        drop(redis_guard);
        drop(postgres_guard);
        Ok(())
    }

    #[tokio::test]
    async fn fails_on_invalid_llm_response() -> Result<()> {
        let (redis_guard, redis_port) = start_redis_container()?;
        let redis_url = format!("redis://{}:{}/", test_host(), redis_port);
        wait_for_redis(&redis_url).await?;

        let (postgres_guard, pg_port) = start_postgres_container()?;
        let database_url = format!(
            "postgres://postgres:postgres@{}:{}/postgres",
            test_host(),
            pg_port
        );
        let pool = connect_postgres(&database_url).await?;
        apply_migrations(&pool).await;

        let (llm_endpoint, server_task) = spawn_llm_with_payload(json!({
            "primary_category": "News / Media",
            "subcategory": "National",
            "risk_level": "low",
            "confidence": 0.88,
            "recommended_action": "DROP"
        }))
        .await;

        let cfg = WorkerConfig {
            queue_name: "classification-jobs".into(),
            redis_url: redis_url.clone(),
            cache_channel: "od:cache:invalidate".into(),
            stream: "classification-jobs".into(),
            page_fetch_stream: "page-fetch-jobs".into(),
            database_url: database_url.clone(),
            llm_endpoint: Some(llm_endpoint.clone()),
            llm_api_key: Some("test-key".into()),
            providers: Vec::new(),
            routing: RoutingConfig::default(),
            metrics_host: "127.0.0.1".into(),
            metrics_port: 0,
        };

        let router = cfg.resolve_router().unwrap();
        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let activation = Arc::new(ActivationState::allow_all());
        let failover = FailoverRuntime::from_routing(&cfg.routing);
        let consumer = JobConsumer::new(&cfg, router, pool.clone(), taxonomy, activation, failover)
            .await
            .unwrap();
        let consumer_handle = tokio::spawn(async move { consumer.run().await });

        let job = ClassificationJobPayload {
            normalized_key: "domain:invalid-llm.test".into(),
            entity_level: "domain".into(),
            hostname: "invalid-llm.test".into(),
            full_url: "https://invalid-llm.test/".into(),
            trace_id: "trace-invalid".into(),
            requires_content: false,
            base_url: None,
            content_excerpt: None,
            content_hash: None,
            content_version: None,
            content_language: None,
        };

        sleep(Duration::from_millis(500)).await;
        enqueue_job(&redis_url, &job).await.expect("enqueue job");

        sleep(Duration::from_secs(3)).await;
        let exists = classification_exists(&pool, &job.normalized_key)
            .await
            .expect("query classification");
        assert!(
            !exists,
            "invalid LLM response should not persist classification"
        );

        consumer_handle.abort();
        server_task.abort();
        drop(redis_guard);
        drop(postgres_guard);
        Ok(())
    }

    #[tokio::test]
    async fn classification_persists_canonical_labels_and_flags() -> Result<()> {
        let (redis_guard, redis_port) = start_redis_container()?;
        let redis_url = format!("redis://{}:{}/", test_host(), redis_port);
        wait_for_redis(&redis_url).await?;

        let (postgres_guard, pg_port) = start_postgres_container()?;
        let database_url = format!(
            "postgres://postgres:postgres@{}:{}/postgres",
            test_host(),
            pg_port
        );
        let pool = connect_postgres(&database_url).await?;
        apply_migrations(&pool).await;

        let (llm_endpoint, server_task) = spawn_llm_with_payload(json!({
            "primary_category": "Social",
            "subcategory": "Short form video",
            "risk_level": "low",
            "confidence": 0.91,
            "recommended_action": "Allow"
        }))
        .await;

        let cfg = WorkerConfig {
            queue_name: "classification-jobs".into(),
            redis_url: redis_url.clone(),
            cache_channel: "od:cache:invalidate".into(),
            stream: "classification-jobs".into(),
            page_fetch_stream: "page-fetch-jobs".into(),
            database_url: database_url.clone(),
            llm_endpoint: Some(llm_endpoint.clone()),
            llm_api_key: Some("test-key".into()),
            providers: Vec::new(),
            routing: RoutingConfig::default(),
            metrics_host: "127.0.0.1".into(),
            metrics_port: 0,
        };

        let router = cfg.resolve_router().unwrap();
        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let activation = Arc::new(ActivationState::allow_all());
        let failover = FailoverRuntime::from_routing(&cfg.routing);
        let consumer = JobConsumer::new(&cfg, router, pool.clone(), taxonomy, activation, failover)
            .await
            .unwrap();
        let consumer_handle = tokio::spawn(async move { consumer.run().await });

        let job = ClassificationJobPayload {
            normalized_key: "domain:canonical.test".into(),
            entity_level: "domain".into(),
            hostname: "canonical.test".into(),
            full_url: "https://canonical.test/".into(),
            trace_id: "trace-canonical".into(),
            requires_content: false,
            base_url: None,
            content_excerpt: None,
            content_hash: None,
            content_version: None,
            content_language: None,
        };

        tokio::time::sleep(Duration::from_millis(500)).await;
        enqueue_job(&redis_url, &job).await.expect("enqueue job");

        timeout(Duration::from_secs(30), async {
            loop {
                if classification_exists(&pool, &job.normalized_key)
                    .await
                    .expect("classification query")
                {
                    break;
                }
                sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .expect("classification persisted");

        let row = sqlx::query(
            "SELECT primary_category, subcategory, flags FROM classifications WHERE normalized_key = $1",
        )
        .bind(&job.normalized_key)
        .fetch_one(&pool)
        .await?;
        let primary: String = row.try_get("primary_category")?;
        let subcategory: String = row.try_get("subcategory")?;
        let flags: Value = row.try_get("flags")?;

        assert_eq!(primary, "social-media");
        assert_eq!(subcategory, "short-video-platforms");
        assert!(
            flags.get("taxonomy_fallback_reason").is_none(),
            "expected no fallback when canonical labels resolved"
        );

        consumer_handle.abort();
        server_task.abort();
        drop(redis_guard);
        drop(postgres_guard);
        Ok(())
    }

    async fn spawn_mock_llm() -> (String, JoinHandle<()>) {
        spawn_llm_with_payload(json!({
            "primary_category": "News / Media",
            "subcategory": "National",
            "risk_level": "low",
            "confidence": 0.88,
            "recommended_action": "Allow"
        }))
        .await
    }

    async fn spawn_llm_with_payload(payload: serde_json::Value) -> (String, JoinHandle<()>) {
        let payload = Arc::new(payload);
        let route_payload = Arc::clone(&payload);
        let app = Router::new().route(
            "/classify",
            post(move |Json(_body): Json<serde_json::Value>| {
                let route_payload = Arc::clone(&route_payload);
                async move { Json((*route_payload).clone()) }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock llm");
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{}:{}/classify", addr.ip(), addr.port());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve mock llm");
        });
        (url, task)
    }

    async fn enqueue_job(
        redis_url: &str,
        job: &ClassificationJobPayload,
    ) -> Result<(), redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let mut conn = client.get_async_connection().await?;
        let payload = serde_json::to_string(job).expect("serialize job");
        let _: () = redis::cmd("XADD")
            .arg("classification-jobs")
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }

    async fn classification_exists(pool: &PgPool, key: &str) -> Result<bool> {
        let row = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM classifications WHERE normalized_key = $1",
        )
        .bind(key)
        .fetch_one(pool)
        .await?;
        Ok(row > 0)
    }

    async fn apply_migrations(pool: &PgPool) {
        let migrations = [
            include_str!("../../../services/admin-api/migrations/0003_classifications.sql"),
            include_str!("../../../services/admin-api/migrations/0004_spec20_artifacts.sql"),
            include_str!("../../../services/admin-api/migrations/0005_page_contents.sql"),
            include_str!("../../../services/admin-api/migrations/0006_classification_requests.sql"),
        ];

        for ddl in migrations {
            match apply_sql_batch(pool, ddl).await {
                Ok(_) => continue,
                Err(err)
                    if ddl.contains("page_contents")
                        && err
                            .to_string()
                            .contains("generation expression is not immutable") =>
                {
                    apply_page_contents_fallback(pool).await;
                }
                Err(err) => panic!("apply migration: {err}"),
            }
        }
    }

    async fn apply_sql_batch(pool: &PgPool, sql: &str) -> Result<()> {
        sqlx::raw_sql(sql).execute(pool).await?;
        Ok(())
    }

    async fn apply_page_contents_fallback(pool: &PgPool) {
        for statement in PAGE_CONTENTS_TEST_DDL {
            sqlx::query(statement)
                .execute(pool)
                .await
                .expect("apply fallback migration statement");
        }
    }

    const PAGE_CONTENTS_TEST_DDL: &[&str] = &[
        r#"
CREATE TABLE IF NOT EXISTS page_contents (
    id UUID PRIMARY KEY,
    normalized_key TEXT NOT NULL,
    fetch_version INTEGER NOT NULL DEFAULT 1,
    content_type TEXT,
    content_hash TEXT,
    raw_bytes BYTEA,
    text_excerpt TEXT,
    char_count INTEGER,
    byte_count INTEGER,
    fetch_status TEXT NOT NULL,
    fetch_reason TEXT,
    ttl_seconds INTEGER NOT NULL DEFAULT 21600,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);
"#,
        r#"
CREATE OR REPLACE FUNCTION page_contents_set_expiry()
RETURNS TRIGGER AS $$
BEGIN
    NEW.expires_at := COALESCE(
        NEW.fetched_at,
        NOW()
    ) + (NEW.ttl_seconds * INTERVAL '1 second');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
"#,
        "DROP TRIGGER IF EXISTS trg_page_contents_set_expiry ON page_contents;",
        r#"
CREATE TRIGGER trg_page_contents_set_expiry
BEFORE INSERT ON page_contents
FOR EACH ROW EXECUTE FUNCTION page_contents_set_expiry();
"#,
        r#"
CREATE UNIQUE INDEX IF NOT EXISTS page_contents_norm_key_version_idx
    ON page_contents (normalized_key, fetch_version DESC);
"#,
        r#"
CREATE INDEX IF NOT EXISTS page_contents_expires_idx
    ON page_contents (expires_at);
"#,
    ];

    async fn connect_postgres(database_url: &str) -> Result<PgPool> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match PgPoolOptions::new()
                .max_connections(5)
                .connect(database_url)
                .await
            {
                Ok(pool) => return Ok(pool),
                Err(_err) if Instant::now() < deadline => {
                    sleep(Duration::from_millis(250)).await;
                    continue;
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    async fn wait_for_redis(redis_url: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match redis::Client::open(redis_url) {
                Ok(client) => match client.get_async_connection().await {
                    Ok(mut conn) => {
                        if redis::cmd("PING")
                            .query_async::<_, String>(&mut conn)
                            .await
                            .is_ok()
                        {
                            return Ok(());
                        }
                    }
                    Err(_) => {}
                },
                Err(_) => {}
            }

            if Instant::now() > deadline {
                anyhow::bail!("redis did not become ready");
            }
            sleep(Duration::from_millis(200)).await;
        }
    }

    struct DockerContainer {
        id: String,
    }

    impl DockerContainer {
        fn run(image: &str, args: Vec<String>) -> Result<Self> {
            let mut cmd = Command::new("docker");
            cmd.arg("run").arg("-d").arg("--rm");
            for arg in args {
                cmd.arg(arg);
            }
            cmd.arg(image);
            let output = cmd
                .output()
                .with_context(|| format!("failed to launch docker image {image}"))?;
            if !output.status.success() {
                anyhow::bail!(
                    "docker run failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let id = String::from_utf8(output.stdout)
                .context("failed to read docker container id")?
                .trim()
                .to_string();
            Ok(Self { id })
        }
    }

    impl Drop for DockerContainer {
        fn drop(&mut self) {
            let _ = Command::new("docker")
                .arg("rm")
                .arg("-f")
                .arg(&self.id)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    fn start_redis_container() -> Result<(DockerContainer, u16)> {
        let port = pick_unused_port().context("no free port for redis")?;
        let container = DockerContainer::run(
            "redis:7-alpine",
            vec!["-p".into(), format!("{}:6379", port)],
        )?;
        Ok((container, port))
    }

    fn start_postgres_container() -> Result<(DockerContainer, u16)> {
        let port = pick_unused_port().context("no free port for postgres")?;
        let container = DockerContainer::run(
            "postgres:16-alpine",
            vec![
                "-p".into(),
                format!("{}:5432", port),
                "-e".into(),
                "POSTGRES_PASSWORD=postgres".into(),
                "-e".into(),
                "POSTGRES_USER=postgres".into(),
            ],
        )?;
        Ok((container, port))
    }

    fn test_host() -> String {
        env::var("TEST_DOCKER_HOST").unwrap_or_else(|_| "127.0.0.1".into())
    }
}
