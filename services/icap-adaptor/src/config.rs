use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct IcapConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_preview_size")]
    pub preview_size: usize,
    pub redis_url: Option<String>,
    pub policy_endpoint: Option<String>,
    #[serde(default = "default_metrics_host")]
    pub metrics_host: String,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
    #[serde(default = "default_cache_channel")]
    pub cache_channel: String,
    #[serde(default = "default_require_content")]
    pub require_content: bool,
    #[serde(default = "default_pending_cache_ttl")]
    pub pending_cache_ttl_seconds: u64,
    #[serde(default)]
    pub job_queue: Option<JobQueueConfig>,
    #[serde(default)]
    pub page_fetch_queue: Option<PageFetchQueueConfig>,
    #[serde(default)]
    pub admin_api: Option<AdminApiConfig>,
}

pub fn load() -> anyhow::Result<IcapConfig> {
    let cfg = config_core::load_config::<IcapConfig>("config/icap.json")?;
    Ok(cfg)
}

const fn default_preview_size() -> usize {
    4096
}

fn default_metrics_host() -> String {
    "0.0.0.0".to_string()
}

const fn default_metrics_port() -> u16 {
    19005
}

fn default_cache_channel() -> String {
    "od:cache:invalidate".to_string()
}

const fn default_require_content() -> bool {
    true
}

const fn default_pending_cache_ttl() -> u64 {
    60
}

#[derive(Debug, Deserialize, Clone)]
pub struct JobQueueConfig {
    pub redis_url: String,
    #[serde(default = "default_job_stream")]
    pub stream: String,
}

fn default_job_stream() -> String {
    "classification-jobs".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct PageFetchQueueConfig {
    pub redis_url: String,
    #[serde(default = "default_page_fetch_stream")]
    pub stream: String,
    #[serde(default = "default_page_fetch_ttl")]
    pub ttl_seconds: i32,
}

fn default_page_fetch_stream() -> String {
    "page-fetch-jobs".to_string()
}

const fn default_page_fetch_ttl() -> i32 {
    21_600
}

#[derive(Debug, Deserialize, Clone)]
pub struct AdminApiConfig {
    pub base_url: String,
    #[serde(default)]
    pub admin_token: Option<String>,
}
