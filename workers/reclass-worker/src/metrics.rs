use anyhow::Result;
use axum::{routing::get, Router};
use once_cell::sync::Lazy;
use prometheus::{self, Encoder, IntCounter, IntGauge, TextEncoder};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

static JOBS_PLANNED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "reclassification_jobs_planned_total",
        "Number of reclassification jobs added to the queue"
    )
    .unwrap()
});

static JOBS_DISPATCHED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "reclassification_jobs_dispatched_total",
        "Number of reclassification jobs published to the classification stream"
    )
    .unwrap()
});

static JOBS_FAILED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "reclassification_jobs_failed_total",
        "Number of reclassification jobs that failed to dispatch"
    )
    .unwrap()
});

static BACKLOG_GAUGE: Lazy<IntGauge> = Lazy::new(|| {
    prometheus::register_int_gauge!(
        "reclassification_backlog",
        "Pending reclassification jobs awaiting dispatch"
    )
    .unwrap()
});

pub fn record_jobs_planned(count: u64) {
    if count > 0 {
        JOBS_PLANNED.inc_by(count);
    }
}

pub fn record_job_dispatched() {
    JOBS_DISPATCHED.inc();
}

pub fn record_job_failure() {
    JOBS_FAILED.inc();
}

pub fn set_reclass_backlog(backlog: i64) {
    BACKLOG_GAUGE.set(backlog);
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
        target = "svc-reclass",
        host = host,
        port = port,
        "metrics server listening"
    );
    axum::serve(listener, router).await?;
    Ok(())
}
