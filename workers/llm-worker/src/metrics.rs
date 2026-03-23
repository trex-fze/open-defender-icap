use anyhow::Result;
use axum::{routing::get, Router};
use once_cell::sync::Lazy;
use prometheus::{self, Encoder, Histogram, HistogramOpts, IntCounter, TextEncoder};
use std::net::SocketAddr;
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

static LLM_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "llm_invocation_failures_total",
        "LLM invocations that failed before receiving a valid response"
    )
    .unwrap()
});

static LLM_TIMEOUTS: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!("llm_timeouts_total", "LLM invocations that timed out")
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

pub fn record_llm_failure() {
    LLM_FAILURES.inc();
}

pub fn record_llm_timeout() {
    LLM_TIMEOUTS.inc();
}

pub fn record_invalid_response() {
    LLM_INVALID_RESPONSES.inc();
}

pub fn observe_llm_latency(seconds: f64) {
    LLM_LATENCY.observe(seconds);
}

async fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

pub async fn serve_metrics(host: &str, port: u16) -> Result<()> {
    let router = Router::new().route("/metrics", get(|| async { metrics_handler().await }));
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
