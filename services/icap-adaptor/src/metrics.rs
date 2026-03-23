use anyhow::Result;
use axum::{routing::get, Router};
use once_cell::sync::Lazy;
use prometheus::{self, Encoder, Gauge, Histogram, HistogramOpts, IntCounter, TextEncoder};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

static CACHE_HITS: Lazy<IntCounter> =
    Lazy::new(|| prometheus::register_int_counter!("cache_hits", "Cache hits").unwrap());
static CACHE_MISSES: Lazy<IntCounter> =
    Lazy::new(|| prometheus::register_int_counter!("cache_misses", "Cache misses").unwrap());
static CACHE_HIT_RATIO: Lazy<Gauge> = Lazy::new(|| {
    prometheus::register_gauge!("cache_hit_ratio", "Ratio of cache hits vs lookups").unwrap()
});
static POLICY_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "policy_decision_latency_seconds",
        "Latency of policy requests",
    )
    .buckets(vec![0.001, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0]);
    prometheus::register_histogram!(opts).unwrap()
});
static ICAP_ERRORS: Lazy<IntCounter> =
    Lazy::new(|| prometheus::register_int_counter!("icap_errors", "ICAP handler errors").unwrap());
static SQUID_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "squid_to_icap_latency_seconds",
        "End-to-end latency from Squid request to ICAP response",
    )
    .buckets(vec![0.001, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0]);
    prometheus::register_histogram!(opts).unwrap()
});

pub fn record_cache_hit() {
    CACHE_HITS.inc();
    update_cache_ratio();
}

pub fn record_cache_miss() {
    CACHE_MISSES.inc();
    update_cache_ratio();
}

pub fn observe_policy_latency(seconds: f64) {
    POLICY_LATENCY.observe(seconds);
}

pub fn record_error() {
    ICAP_ERRORS.inc();
}

pub fn observe_squid_roundtrip(seconds: f64) {
    SQUID_LATENCY.observe(seconds);
}

fn update_cache_ratio() {
    let hits = CACHE_HITS.get() as f64;
    let misses = CACHE_MISSES.get() as f64;
    let total = hits + misses;
    if total > 0.0 {
        CACHE_HIT_RATIO.set(hits / total);
    }
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
    info!(target = "svc-icap", %host, port, "metrics server listening");
    axum::serve(listener, router).await?;
    Ok(())
}
