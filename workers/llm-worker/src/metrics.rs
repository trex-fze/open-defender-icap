use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use once_cell::sync::Lazy;
use prometheus::{self, Encoder, Histogram, HistogramOpts, HistogramVec, IntCounter, IntCounterVec, TextEncoder};
use serde::Serialize;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
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

pub fn record_job_started() {
    JOBS_STARTED.inc();
}

pub fn record_job_completed() {
    JOBS_COMPLETED.inc();
}

pub fn record_job_failed() {
    JOBS_FAILED.inc();
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
    LLM_PROVIDER_FAILURES
        .with_label_values(&[provider])
        .inc();
}

pub fn record_llm_timeout() {
    LLM_TIMEOUTS.inc();
}

pub fn record_provider_timeout(provider: &str) {
    LLM_PROVIDER_TIMEOUTS
        .with_label_values(&[provider])
        .inc();
}

pub fn record_invalid_response() {
    LLM_INVALID_RESPONSES.inc();
}

pub fn observe_llm_latency(seconds: f64) {
    LLM_LATENCY.observe(seconds);
}

pub fn observe_provider_latency(provider: &str, seconds: f64) {
    LLM_PROVIDER_LATENCY
        .with_label_values(&[provider])
        .observe(seconds);
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

pub async fn serve_metrics(
    host: &str,
    port: u16,
    providers: Arc<Vec<ProviderSummary>>,
) -> Result<()> {
    let router = Router::new()
        .route("/metrics", get(|| async { metrics_handler().await }))
        .route("/providers", get(providers_handler))
        .with_state(providers);
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
    State(providers): State<Arc<Vec<ProviderSummary>>>,
) -> Json<Vec<ProviderSummary>> {
    Json((*providers).clone())
}
