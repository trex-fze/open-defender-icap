use once_cell::sync::Lazy;
use prometheus::{register_int_counter, Encoder, IntCounter, TextEncoder};

static TAXONOMY_ACTIVATION_CHANGES: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "taxonomy_activation_changes_total",
        "Number of times the taxonomy activation profile was saved"
    )
    .unwrap()
});

#[derive(Clone)]
pub struct ReviewMetrics {
    #[allow(dead_code)]
    review_sla_seconds: u64,
}

impl ReviewMetrics {
    pub fn new(review_sla_seconds: u64) -> Self {
        Self { review_sla_seconds }
    }

    pub fn render(&self) -> Result<String, prometheus::Error> {
        let metric_families = prometheus::gather();
        let mut buffer = Vec::new();
        TextEncoder::new().encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer).unwrap_or_default())
    }
}

pub fn record_taxonomy_activation_change() {
    TAXONOMY_ACTIVATION_CHANGES.inc();
}
