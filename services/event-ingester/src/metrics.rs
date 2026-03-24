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

static PAGE_FETCH_ATTEMPTS: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "page_fetch_jobs_attempted_total",
        "Number of page fetch jobs attempted"
    )
    .unwrap()
});

static PAGE_FETCH_ENQUEUED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "page_fetch_jobs_enqueued_total",
        "Number of page fetch jobs successfully enqueued"
    )
    .unwrap()
});

static PAGE_FETCH_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "page_fetch_jobs_failed_total",
        "Number of page fetch jobs that failed to enqueue"
    )
    .unwrap()
});

static PAGE_FETCH_SKIPPED: Lazy<IntCounter> = Lazy::new(|| {
    prometheus::register_int_counter!(
        "page_fetch_jobs_skipped_total",
        "Events that did not produce a valid page fetch job"
    )
    .unwrap()
});

pub fn record_batch(event_count: usize, duration_secs: f64) {
    INGEST_BATCHES.inc();
    INGEST_EVENTS.inc_by(event_count as u64);
    INGEST_DURATION.observe(duration_secs);
}

pub fn record_failure() {
    INGEST_FAILURES.inc();
}

pub fn record_page_fetch_attempt() {
    PAGE_FETCH_ATTEMPTS.inc();
}

pub fn record_page_fetch_enqueued() {
    PAGE_FETCH_ENQUEUED.inc();
}

pub fn record_page_fetch_failure() {
    PAGE_FETCH_FAILURES.inc();
}

pub fn record_page_fetch_skipped() {
    PAGE_FETCH_SKIPPED.inc();
}

pub async fn metrics_endpoint() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap_or_default()
}
