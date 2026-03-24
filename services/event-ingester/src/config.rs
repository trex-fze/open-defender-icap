use std::env;

#[derive(Debug, Clone)]
pub struct IngestConfig {
    pub bind_addr: String,
    pub elastic_url: String,
    pub elastic_index_prefix: String,
    pub elastic_index_pattern: String,
    pub elastic_template_name: String,
    pub elastic_ilm_name: String,
    pub elastic_api_key: Option<String>,
    pub elastic_username: Option<String>,
    pub elastic_password: Option<String>,
    pub filebeat_secret: Option<String>,
    pub log_level: String,
    pub ingest_retry_attempts: usize,
    pub apply_templates: bool,
    pub page_fetch_redis_url: Option<String>,
    pub page_fetch_stream: String,
    pub page_fetch_ttl_seconds: i32,
}

impl IngestConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            bind_addr: env_or("OD_INGEST_BIND", "0.0.0.0:19100"),
            elastic_url: env_req("OD_ELASTIC_URL")?,
            elastic_index_prefix: env_or("OD_ELASTIC_INDEX_PREFIX", "traffic-events"),
            elastic_index_pattern: env_or("OD_ELASTIC_INDEX_PATTERN", "traffic-events-*"),
            elastic_template_name: env_or("OD_ELASTIC_TEMPLATE_NAME", "traffic-events-template"),
            elastic_ilm_name: env_or("OD_ELASTIC_ILM_NAME", "traffic-events-ilm"),
            elastic_api_key: env::var("OD_ELASTIC_API_KEY")
                .ok()
                .filter(|v| !v.is_empty()),
            elastic_username: env::var("OD_ELASTIC_USERNAME")
                .ok()
                .filter(|v| !v.is_empty()),
            elastic_password: env::var("OD_ELASTIC_PASSWORD")
                .ok()
                .filter(|v| !v.is_empty()),
            filebeat_secret: env::var("OD_FILEBEAT_SECRET")
                .ok()
                .filter(|v| !v.is_empty()),
            log_level: env_or("OD_LOG", "info"),
            ingest_retry_attempts: env_or("OD_INGEST_RETRY_ATTEMPTS", "3").parse().unwrap_or(3),
            apply_templates: env_or("OD_ELASTIC_APPLY_TEMPLATES", "true")
                .parse()
                .unwrap_or(true),
            page_fetch_redis_url: env::var("OD_PAGE_FETCH_REDIS_URL")
                .ok()
                .filter(|v| !v.is_empty()),
            page_fetch_stream: env_or("OD_PAGE_FETCH_STREAM", "page-fetch-jobs"),
            page_fetch_ttl_seconds: env_or("OD_PAGE_FETCH_TTL_SECONDS", "21600")
                .parse()
                .unwrap_or(21_600),
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_req(key: &str) -> anyhow::Result<String> {
    env::var(key)
        .map_err(|_| anyhow::anyhow!("missing required env var {key}"))
        .map(|value| value.trim().to_string())
        .and_then(|value| {
            if value.is_empty() {
                Err(anyhow::anyhow!("env var {key} cannot be empty"))
            } else {
                Ok(value)
            }
        })
}
