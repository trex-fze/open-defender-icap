use anyhow::Result;
use axum::{routing::get, Router};
use once_cell::sync::Lazy;
use prometheus::{Encoder, Histogram, HistogramOpts, IntCounter, TextEncoder};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

static JOBS_STARTED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "page_fetch_jobs_started_total",
        "Number of page fetch jobs pulled from the queue"
    )
    .unwrap()
});

static JOBS_COMPLETED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "page_fetch_jobs_completed_total",
        "Number of page fetch jobs successfully stored"
    )
    .unwrap()
});

static JOBS_FAILED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "page_fetch_jobs_failed_total",
        "Number of page fetch jobs that failed"
    )
    .unwrap()
});

static JOBS_SKIPPED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "page_fetch_jobs_skipped_total",
        "Jobs skipped because cached content is still fresh"
    )
    .unwrap()
});

static CRAWL_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "page_fetch_crawl_failures_total",
        "Number of Crawl4AI invocations that failed"
    )
    .unwrap()
});

static FETCH_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "page_fetch_duration_seconds",
        "Latency for Crawl4AI fetch operations",
    )
    .buckets(vec![0.5, 1.0, 2.0, 5.0, 10.0, 20.0]);
    prometheus::register_histogram!(opts).unwrap()
});

#[derive(Clone)]
pub struct MetricsServer;

impl MetricsServer {
    pub fn new() -> Self {
        Self
    }

    pub fn record_job_started(&self) {
        JOBS_STARTED.inc();
    }

    pub fn record_job_completed(&self) {
        JOBS_COMPLETED.inc();
    }

    pub fn record_job_failed(&self) {
        JOBS_FAILED.inc();
    }

    pub fn record_job_skipped(&self) {
        JOBS_SKIPPED.inc();
    }

    pub fn record_crawl_failure(&self) {
        CRAWL_FAILURES.inc();
    }

    pub fn observe_fetch_latency(&self, seconds: f64) {
        FETCH_LATENCY.observe(seconds);
    }

    pub async fn run(&self, host: String, port: u16) -> Result<()> {
        let router = Router::new().route("/metrics", get(metrics_handler));
        let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        info!(
            target = "svc-page-fetcher",
            host = host,
            port = port,
            "metrics server listening"
        );
        axum::serve(listener, router).await?;
        Ok(())
    }
}

async fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}
