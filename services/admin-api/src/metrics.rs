use once_cell::sync::Lazy;
use prometheus::{register_int_counter, Encoder, IntCounter, TextEncoder};

static TAXONOMY_ACTIVATION_CHANGES: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "taxonomy_activation_changes_total",
        "Number of times the taxonomy activation profile was saved"
    )
    .unwrap()
});

static AUTH_LOGIN_SUCCESS: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "admin_auth_login_success_total",
        "Number of successful local auth login attempts"
    )
    .unwrap()
});

static AUTH_LOGIN_FAILURE: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "admin_auth_login_failure_total",
        "Number of failed local auth login attempts"
    )
    .unwrap()
});

static AUTH_LOCKOUT: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "admin_auth_lockout_total",
        "Number of auth attempts blocked by account lockout"
    )
    .unwrap()
});

static AUTH_REFRESH_SUCCESS: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "admin_auth_refresh_success_total",
        "Number of successful auth refresh token exchanges"
    )
    .unwrap()
});

static AUTH_REFRESH_FAILURE: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "admin_auth_refresh_failure_total",
        "Number of failed auth refresh token exchanges"
    )
    .unwrap()
});

static AUTH_LOGOUT: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "admin_auth_logout_total",
        "Number of auth logout/revocation requests"
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

pub fn record_auth_login_success() {
    AUTH_LOGIN_SUCCESS.inc();
}

pub fn record_auth_login_failure() {
    AUTH_LOGIN_FAILURE.inc();
}

pub fn record_auth_lockout() {
    AUTH_LOCKOUT.inc();
}

pub fn record_auth_refresh_success() {
    AUTH_REFRESH_SUCCESS.inc();
}

pub fn record_auth_refresh_failure() {
    AUTH_REFRESH_FAILURE.inc();
}

pub fn record_auth_logout() {
    AUTH_LOGOUT.inc();
}
