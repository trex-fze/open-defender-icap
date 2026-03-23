use once_cell::sync::Lazy;
use prometheus::{self, Encoder, Histogram, HistogramOpts, IntCounter, TextEncoder};

static INGEST_BATCHES: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!("ingest_batches_total", "Number of bulk batches processed")
        .unwrap()
});

static INGEST_EVENTS: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "ingest_events_total",
        "Number of individual events processed"
    )
    .unwrap()
});

static INGEST_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "ingest_failures_total",
        "Number of failed bulk indexing attempts"
    )
    .unwrap()
});

static INGEST_DURATION: Lazy<Histogram> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "ingest_bulk_duration_seconds",
        "Time spent performing a bulk index request",
    )
    .buckets(vec![0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0]);
    prometheus::register_histogram!(opts).unwrap()
});

pub fn record_batch(event_count: usize, duration_secs: f64) {
    INGEST_BATCHES.inc();
    INGEST_EVENTS.inc_by(event_count as u64);
    INGEST_DURATION.observe(duration_secs);
}

pub fn record_failure() {
    INGEST_FAILURES.inc();
}

pub async fn metrics_endpoint() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap_or_default()
}
