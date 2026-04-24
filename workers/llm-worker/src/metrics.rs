use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use once_cell::sync::Lazy;
use prometheus::{
    self, Encoder, Histogram, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, TextEncoder,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::info;

static JOBS_STARTED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_jobs_started_total",
        "Number of classification jobs pulled from the queue"
    )
    .unwrap()
});

static JOBS_COMPLETED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_jobs_completed_total",
        "Number of classification jobs successfully persisted"
    )
    .unwrap()
});

static JOBS_FAILED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_jobs_failed_total",
        "Number of classification jobs that failed processing"
    )
    .unwrap()
});

static JOBS_REQUEUED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_jobs_requeued_total",
        "Number of classification jobs requeued for retry"
    )
    .unwrap()
});

static JOBS_DUPLICATE_SKIPPED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_jobs_duplicate_skipped_total",
        "Number of classification jobs skipped by idempotency dedupe"
    )
    .unwrap()
});

static DLQ_PUBLISHED: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_dlq_published_total",
        "Number of LLM stream entries published to dead-letter queue",
        &["reason"]
    )
    .unwrap()
});

static JOBS_TERMINALIZED: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_jobs_terminalized_total",
        "Number of jobs terminalized with failure reasons",
        &["reason"]
    )
    .unwrap()
});

static PENDING_STATUS_UPDATES: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_pending_status_updates_total",
        "Classification pending status transitions",
        &["status"]
    )
    .unwrap()
});

static LLM_INVOCATIONS: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!("llm_invocations_total", "Total LLM API invocation attempts")
        .unwrap()
});

static LLM_PROVIDER_INVOCATIONS: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_provider_invocations_total",
        "LLM invocation attempts by provider",
        &["provider"]
    )
    .unwrap()
});

static LLM_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_invocation_failures_total",
        "LLM invocations that failed before receiving a valid response"
    )
    .unwrap()
});

static LLM_PROVIDER_FAILURES: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_provider_failures_total",
        "LLM invocation failures by provider",
        &["provider"]
    )
    .unwrap()
});

static LLM_PROVIDER_SUCCESS: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_provider_success_total",
        "LLM invocation successes by provider",
        &["provider"]
    )
    .unwrap()
});

static LLM_PROVIDER_FAILURE_CLASS: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_provider_failure_class_total",
        "LLM invocation failures by provider, class and status",
        &["provider", "class", "status_code"]
    )
    .unwrap()
});

static LLM_FALLBACK_ATTEMPTS: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_fallback_attempts_total",
        "Number of provider fallback attempts",
        &["from_provider", "to_provider", "reason"]
    )
    .unwrap()
});

static LLM_FALLBACK_SKIPPED: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_fallback_skipped_total",
        "Number of times provider fallback was skipped",
        &["reason"]
    )
    .unwrap()
});

static LLM_PRIMARY_RETRIES: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_primary_retries_total",
        "Number of retries on primary provider",
        &["provider", "reason"]
    )
    .unwrap()
});

static LLM_PRIMARY_RETRY_EXHAUSTED: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_primary_retry_exhausted_total",
        "Number of primary provider requests that exhausted retries",
        &["provider", "reason"]
    )
    .unwrap()
});

static LLM_TIMEOUTS: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!("llm_timeouts_total", "LLM invocations that timed out")
        .unwrap()
});

static LLM_PROVIDER_TIMEOUTS: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_provider_timeouts_total",
        "LLM invocation timeouts by provider",
        &["provider"]
    )
    .unwrap()
});

static LLM_INVALID_RESPONSES: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_invalid_responses_total",
        "LLM responses rejected by schema validation"
    )
    .unwrap()
});

static TAXONOMY_ACTIVATION_BLOCKS: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "taxonomy_activation_blocks_total",
        "Classification verdicts blocked by activation profile"
    )
    .unwrap()
});

static TAXONOMY_FALLBACKS: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "taxonomy_fallback_total",
        "Number of taxonomy normalization fallbacks",
        &["reason"]
    )
    .unwrap()
});

static LLM_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "llm_request_duration_seconds",
        "Latency for outbound LLM HTTP requests",
    )
    .buckets(vec![0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0]);
    prometheus::register_histogram!(opts).unwrap()
});

static LLM_PROVIDER_LATENCY: Lazy<HistogramVec> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "llm_provider_request_duration_seconds",
        "Latency for outbound LLM HTTP requests broken down by provider",
    )
    .buckets(vec![0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0]);
    prometheus::register_histogram_vec!(opts, &["provider"]).unwrap()
});

static PENDING_AGE: Lazy<HistogramVec> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "llm_pending_age_seconds",
        "Observed age of classification pending rows in seconds",
    )
    .buckets(vec![5.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0]);
    prometheus::register_histogram_vec!(opts, &["phase"]).unwrap()
});

static TERMINALIZATION_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "llm_terminalization_latency_seconds",
        "Latency from pending request creation to terminalized classification state",
    )
    .buckets(vec![10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0]);
    prometheus::register_histogram!(opts).unwrap()
});

static STALE_PENDING_ELIGIBLE: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_stale_pending_eligible_total",
        "Number of jobs eligible for stale pending online diversion"
    )
    .unwrap()
});

static STALE_PENDING_DIVERT: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_stale_pending_divert_total",
        "Stale pending diversion attempts/results by provider",
        &["provider", "result"]
    )
    .unwrap()
});

static STALE_PENDING_HEALTHCHECK: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_stale_pending_healthcheck_total",
        "Stale pending provider healthcheck outcomes",
        &["provider", "result"]
    )
    .unwrap()
});

static STALE_PENDING_SKIPPED: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_stale_pending_skipped_total",
        "Stale pending diversion skips by reason",
        &["reason"]
    )
    .unwrap()
});

static CONTEXT_MODE: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_context_mode_total",
        "Classification jobs by provider and context mode",
        &["provider", "mode"]
    )
    .unwrap()
});

static METADATA_ONLY_GUARDRAIL: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_metadata_only_guardrail_total",
        "Metadata-only guardrail applications",
        &["type"]
    )
    .unwrap()
});

static METADATA_ONLY_REQUEUE: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_metadata_only_requeue_total",
        "Metadata-only classifications that stay pending for content follow-up"
    )
    .unwrap()
});

static METADATA_ONLY_REASON: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_metadata_only_reason_total",
        "Metadata-only classifications by reason",
        &["reason"]
    )
    .unwrap()
});

static FETCH_FAILURE_FALLBACK: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_fetch_failure_fallback_total",
        "Metadata fallback triggered by repeated fetch failures",
        &["provider", "result"]
    )
    .unwrap()
});

static PRIMARY_OUTPUT_INVALID: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_primary_output_invalid_total",
        "Primary provider responses that failed JSON/schema contract checks"
    )
    .unwrap()
});

static ONLINE_VERIFICATION: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_online_verification_total",
        "Online verification outcomes after local output-invalid failures",
        &["result"]
    )
    .unwrap()
});

static TERMINAL_INSUFFICIENT_EVIDENCE: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_terminal_insufficient_evidence_total",
        "Classifications terminalized to unknown-unclassified/insufficient-evidence"
    )
    .unwrap()
});

static PROMPT_INJECTION_MARKER: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_prompt_injection_marker_total",
        "Prompt-injection marker hits observed in content excerpts",
        &["marker"]
    )
    .unwrap()
});

static PROMPT_INJECTION_GUARDRAIL: Lazy<IntCounterVec> = Lazy::new(|| {
    prometheus::register_int_counter_vec!(
        "llm_prompt_injection_guardrail_total",
        "Prompt-injection guardrail applications by type",
        &["type"]
    )
    .unwrap()
});

pub fn record_job_started() {
    JOBS_STARTED.inc();
}

pub fn record_job_completed() {
    JOBS_COMPLETED.inc();
}

pub fn record_job_failed() {
    JOBS_FAILED.inc();
}

pub fn record_job_requeued() {
    JOBS_REQUEUED.inc();
}

pub fn record_job_duplicate() {
    JOBS_DUPLICATE_SKIPPED.inc();
}

pub fn record_dlq_published(reason: &str) {
    DLQ_PUBLISHED.with_label_values(&[reason]).inc();
}

pub fn record_job_terminalized(reason: &str) {
    JOBS_TERMINALIZED.with_label_values(&[reason]).inc();
}

pub fn record_pending_status(status: &str) {
    PENDING_STATUS_UPDATES.with_label_values(&[status]).inc();
}

pub fn record_llm_invocation() {
    LLM_INVOCATIONS.inc();
}

pub fn record_provider_invocation(provider: &str) {
    LLM_PROVIDER_INVOCATIONS
        .with_label_values(&[provider])
        .inc();
}

pub fn record_llm_failure() {
    LLM_FAILURES.inc();
}

pub fn record_provider_failure(provider: &str) {
    LLM_PROVIDER_FAILURES.with_label_values(&[provider]).inc();
}

pub fn record_provider_success(provider: &str) {
    LLM_PROVIDER_SUCCESS.with_label_values(&[provider]).inc();
}

pub fn record_provider_failure_class(provider: &str, class: &str, status_code: Option<u16>) {
    let status = status_code
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    LLM_PROVIDER_FAILURE_CLASS
        .with_label_values(&[provider, class, status.as_str()])
        .inc();
}

pub fn record_fallback_attempt(from_provider: &str, to_provider: &str, reason: &str) {
    LLM_FALLBACK_ATTEMPTS
        .with_label_values(&[from_provider, to_provider, reason])
        .inc();
}

pub fn record_fallback_skipped(reason: &str) {
    LLM_FALLBACK_SKIPPED.with_label_values(&[reason]).inc();
}

pub fn record_primary_retry(provider: &str, reason: &str) {
    LLM_PRIMARY_RETRIES
        .with_label_values(&[provider, reason])
        .inc();
}

pub fn record_primary_retry_exhausted(provider: &str, reason: &str) {
    LLM_PRIMARY_RETRY_EXHAUSTED
        .with_label_values(&[provider, reason])
        .inc();
}

pub fn record_llm_timeout() {
    LLM_TIMEOUTS.inc();
}

pub fn record_provider_timeout(provider: &str) {
    LLM_PROVIDER_TIMEOUTS.with_label_values(&[provider]).inc();
}

pub fn record_invalid_response() {
    LLM_INVALID_RESPONSES.inc();
}

pub fn record_activation_block() {
    TAXONOMY_ACTIVATION_BLOCKS.inc();
}

pub fn record_taxonomy_fallback(reason: &str) {
    TAXONOMY_FALLBACKS.with_label_values(&[reason]).inc();
}

pub fn observe_llm_latency(seconds: f64) {
    LLM_LATENCY.observe(seconds);
}

pub fn observe_provider_latency(provider: &str, seconds: f64) {
    LLM_PROVIDER_LATENCY
        .with_label_values(&[provider])
        .observe(seconds);
}

pub fn observe_pending_age_seconds(seconds: f64, phase: &str) {
    PENDING_AGE.with_label_values(&[phase]).observe(seconds);
}

pub fn observe_terminalization_latency_seconds(seconds: f64) {
    TERMINALIZATION_LATENCY.observe(seconds);
}

pub fn record_stale_pending_eligible() {
    STALE_PENDING_ELIGIBLE.inc();
}

pub fn record_stale_pending_divert(provider: &str, result: &str) {
    STALE_PENDING_DIVERT
        .with_label_values(&[provider, result])
        .inc();
}

pub fn record_stale_pending_healthcheck(provider: &str, result: &str) {
    STALE_PENDING_HEALTHCHECK
        .with_label_values(&[provider, result])
        .inc();
}

pub fn record_stale_pending_skipped(reason: &str) {
    STALE_PENDING_SKIPPED.with_label_values(&[reason]).inc();
}

pub fn record_context_mode(provider: &str, mode: &str) {
    CONTEXT_MODE.with_label_values(&[provider, mode]).inc();
}

pub fn record_metadata_only_guardrail(guardrail_type: &str) {
    METADATA_ONLY_GUARDRAIL
        .with_label_values(&[guardrail_type])
        .inc();
}

pub fn record_metadata_only_requeue() {
    METADATA_ONLY_REQUEUE.inc();
}

pub fn record_metadata_only_reason(reason: &str) {
    METADATA_ONLY_REASON.with_label_values(&[reason]).inc();
}

pub fn record_fetch_failure_fallback(provider: &str, result: &str) {
    FETCH_FAILURE_FALLBACK
        .with_label_values(&[provider, result])
        .inc();
}

pub fn record_primary_output_invalid() {
    PRIMARY_OUTPUT_INVALID.inc();
}

pub fn record_online_verification(result: &str) {
    ONLINE_VERIFICATION.with_label_values(&[result]).inc();
}

pub fn record_terminal_insufficient_evidence() {
    TERMINAL_INSUFFICIENT_EVIDENCE.inc();
}

pub fn record_prompt_injection_marker(marker: &str) {
    PROMPT_INJECTION_MARKER.with_label_values(&[marker]).inc();
}

pub fn record_prompt_injection_guardrail(guardrail_type: &str) {
    PROMPT_INJECTION_GUARDRAIL
        .with_label_values(&[guardrail_type])
        .inc();
}

async fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

#[derive(Debug, Clone, Serialize)]
pub enum ProviderRole {
    #[serde(rename = "primary")]
    Primary,
    #[serde(rename = "fallback")]
    Fallback,
    #[serde(rename = "legacy")]
    Legacy,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderSummary {
    pub name: String,
    pub provider_type: String,
    pub endpoint: String,
    pub role: ProviderRole,
}

#[derive(Debug, Clone)]
pub struct ProviderProbeConfig {
    pub name: String,
    pub provider_type: String,
    pub endpoint: String,
    pub headers: HashMap<String, String>,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealthStatus {
    Healthy,
    Degraded,
    Unreachable,
    Misconfigured,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatusSummary {
    pub name: String,
    pub provider_type: String,
    pub endpoint: String,
    pub role: ProviderRole,
    pub health_status: ProviderHealthStatus,
    pub health_checked_at_ms: u64,
    pub health_latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_detail: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedProviderHealth {
    checked_at: Instant,
    snapshot: ProviderStatusSummary,
}

#[derive(Clone)]
struct ProvidersState {
    providers: Arc<Vec<ProviderSummary>>,
    probe_configs: Arc<HashMap<String, ProviderProbeConfig>>,
    health_cache: Arc<Mutex<HashMap<String, CachedProviderHealth>>>,
    client: reqwest::Client,
    ttl: Duration,
    timeout: Duration,
}

pub async fn serve_metrics(
    host: &str,
    port: u16,
    providers: Arc<Vec<ProviderSummary>>,
    probe_configs: Arc<Vec<ProviderProbeConfig>>,
    health_ttl: Duration,
    health_timeout: Duration,
) -> Result<()> {
    let mut probe_map = HashMap::with_capacity(probe_configs.len());
    for cfg in probe_configs.iter() {
        probe_map.insert(cfg.name.clone(), cfg.clone());
    }
    let state = ProvidersState {
        providers,
        probe_configs: Arc::new(probe_map),
        health_cache: Arc::new(Mutex::new(HashMap::new())),
        client: reqwest::Client::new(),
        ttl: health_ttl,
        timeout: health_timeout,
    };

    let router = Router::new()
        .route("/metrics", get(|| async { metrics_handler().await }))
        .route("/providers", get(providers_handler))
        .with_state(state);
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!(
        target = "svc-llm-worker",
        host = host,
        port = port,
        "metrics server listening"
    );
    axum::serve(listener, router).await?;
    Ok(())
}

async fn providers_handler(
    State(state): State<ProvidersState>,
) -> Json<Vec<ProviderStatusSummary>> {
    let mut snapshots = Vec::with_capacity(state.providers.len());
    for provider in state.providers.iter() {
        snapshots.push(provider_health_snapshot(&state, provider).await);
    }
    Json(snapshots)
}

async fn provider_health_snapshot(
    state: &ProvidersState,
    provider: &ProviderSummary,
) -> ProviderStatusSummary {
    {
        let cache = state.health_cache.lock().await;
        if let Some(entry) = cache.get(&provider.name) {
            if entry.checked_at.elapsed() < state.ttl {
                return entry.snapshot.clone();
            }
        }
    }

    let snapshot = probe_provider_health(state, provider).await;
    let mut cache = state.health_cache.lock().await;
    cache.insert(
        provider.name.clone(),
        CachedProviderHealth {
            checked_at: Instant::now(),
            snapshot: snapshot.clone(),
        },
    );
    snapshot
}

async fn probe_provider_health(
    state: &ProvidersState,
    provider: &ProviderSummary,
) -> ProviderStatusSummary {
    let checked_at_ms = unix_timestamp_ms();
    let start = Instant::now();
    let Some(probe_cfg) = state.probe_configs.get(&provider.name) else {
        return ProviderStatusSummary {
            name: provider.name.clone(),
            provider_type: provider.provider_type.clone(),
            endpoint: provider.endpoint.clone(),
            role: provider.role.clone(),
            health_status: ProviderHealthStatus::Unknown,
            health_checked_at_ms: checked_at_ms,
            health_latency_ms: 0,
            health_http_status: None,
            health_detail: Some("provider probe configuration unavailable".to_string()),
        };
    };

    let probe_url = derive_probe_url(probe_cfg);
    let mut request = state.client.get(probe_url).timeout(state.timeout);
    for (header_name, header_value) in probe_cfg.headers.iter() {
        request = request.header(header_name, header_value);
    }
    if !probe_cfg.api_key.trim().is_empty() {
        if probe_cfg.provider_type.eq_ignore_ascii_case("anthropic") {
            request = request
                .header("x-api-key", probe_cfg.api_key.as_str())
                .header("anthropic-version", "2023-06-01");
        } else {
            request = request.bearer_auth(probe_cfg.api_key.as_str());
        }
    }

    match request.send().await {
        Ok(response) => {
            let http_status = response.status().as_u16();
            let health_status = if response.status().is_success() {
                ProviderHealthStatus::Healthy
            } else if http_status == 401 || http_status == 403 {
                ProviderHealthStatus::Misconfigured
            } else if http_status == 408 || http_status == 429 || http_status >= 500 {
                ProviderHealthStatus::Degraded
            } else {
                ProviderHealthStatus::Unknown
            };
            ProviderStatusSummary {
                name: provider.name.clone(),
                provider_type: provider.provider_type.clone(),
                endpoint: provider.endpoint.clone(),
                role: provider.role.clone(),
                health_status,
                health_checked_at_ms: checked_at_ms,
                health_latency_ms: start.elapsed().as_millis() as u64,
                health_http_status: Some(http_status),
                health_detail: if response.status().is_success() {
                    None
                } else {
                    Some(format!("probe returned HTTP {}", http_status))
                },
            }
        }
        Err(err) => {
            let health_status = if err.is_connect() || err.is_timeout() {
                ProviderHealthStatus::Unreachable
            } else {
                ProviderHealthStatus::Degraded
            };
            ProviderStatusSummary {
                name: provider.name.clone(),
                provider_type: provider.provider_type.clone(),
                endpoint: provider.endpoint.clone(),
                role: provider.role.clone(),
                health_status,
                health_checked_at_ms: checked_at_ms,
                health_latency_ms: start.elapsed().as_millis() as u64,
                health_http_status: None,
                health_detail: Some(err.to_string()),
            }
        }
    }
}

fn derive_probe_url(config: &ProviderProbeConfig) -> String {
    let endpoint = config.endpoint.trim();
    let Ok(mut url) = reqwest::Url::parse(endpoint) else {
        return endpoint.to_string();
    };

    let provider_type = config.provider_type.to_ascii_lowercase();
    let path = url.path().to_string();
    if provider_type == "ollama" {
        url.set_path("/api/tags");
        return url.to_string();
    }

    if provider_type == "anthropic" {
        url.set_path("/v1/models");
        return url.to_string();
    }

    if provider_type == "openai"
        || provider_type == "openai-compatible"
        || provider_type == "openai_compatible"
        || provider_type == "vllm"
        || provider_type == "lmstudio"
    {
        if let Some(prefix) = path.strip_suffix("/chat/completions") {
            url.set_path(&format!("{}/models", prefix));
        } else if let Some(prefix) = path.strip_suffix("/messages") {
            url.set_path(&format!("{}/models", prefix));
        } else {
            url.set_path("/v1/models");
        }
        return url.to_string();
    }

    endpoint.to_string()
}

fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
