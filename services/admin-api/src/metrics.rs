use once_cell::sync::Lazy;
use prometheus::{
    register_histogram, register_int_counter, register_int_gauge, Encoder, Histogram, IntCounter,
    IntGauge, TextEncoder,
};
use sqlx::PgPool;

static REVIEW_OPEN_GAUGE: Lazy<IntGauge> = Lazy::new(|| {
    register_int_gauge!("review_queue_open_total", "Number of pending review items").unwrap()
});

static REVIEW_RESOLUTION_HISTO: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "review_resolution_seconds",
        "Histogram of review resolution times in seconds",
        vec![600.0, 1800.0, 3600.0, 10800.0, 21600.0, 43200.0]
    )
    .unwrap()
});

static REVIEW_SLA_MET: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "review_sla_met_total",
        "Count of review items resolved within SLA"
    )
    .unwrap()
});

static REVIEW_SLA_BREACHED: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "review_sla_breached_total",
        "Count of review items resolved outside SLA"
    )
    .unwrap()
});

#[derive(Clone)]
pub struct ReviewMetrics {
    sla_seconds: f64,
}

impl ReviewMetrics {
    pub fn new(sla_seconds: u64) -> Self {
        Self {
            sla_seconds: sla_seconds as f64,
        }
    }

    pub fn set_open_count(&self, value: i64) {
        REVIEW_OPEN_GAUGE.set(value);
    }

    pub fn record_resolution(&self, duration_secs: f64) {
        REVIEW_RESOLUTION_HISTO.observe(duration_secs);
        if duration_secs <= self.sla_seconds {
            REVIEW_SLA_MET.inc();
        } else {
            REVIEW_SLA_BREACHED.inc();
        }
    }

    pub async fn sync_from_db(&self, pool: &PgPool) -> Result<(), sqlx::Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM review_queue WHERE status = 'pending'")
                .fetch_one(pool)
                .await?;
        self.set_open_count(count);
        Ok(())
    }

    pub fn render(&self) -> Result<String, prometheus::Error> {
        let metric_families = prometheus::gather();
        let mut buffer = Vec::new();
        TextEncoder::new().encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sla_classification() {
        let metrics = ReviewMetrics::new(3600);
        let start_met = REVIEW_SLA_MET.get();
        let start_breach = REVIEW_SLA_BREACHED.get();
        metrics.record_resolution(1800.0);
        metrics.record_resolution(7200.0);
        assert_eq!(REVIEW_SLA_MET.get(), start_met + 1);
        assert_eq!(REVIEW_SLA_BREACHED.get(), start_breach + 1);
    }
}
