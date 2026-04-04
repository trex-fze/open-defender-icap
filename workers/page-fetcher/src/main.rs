mod metrics;

use anyhow::{anyhow, Context, Result};
use common_types::PageFetchJob;
use config_core::load_config;
use metrics::MetricsServer;
use redis::streams::{StreamReadOptions, StreamReadReply};
use redis::AsyncCommands;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::signal;
use tokio::time::Instant;
use tracing::{error, info, Level};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Deserialize, Clone)]
struct WorkerConfig {
    #[serde(default = "default_queue")]
    pub queue_name: String,
    #[serde(default = "default_stream")]
    pub stream: String,
    #[serde(default = "default_stream_start_id")]
    pub stream_start_id: String,
    pub redis_url: String,
    pub crawl_service_url: String,
    pub database_url: String,
    #[serde(default = "default_metrics_host")]
    pub metrics_host: String,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
    #[serde(default = "default_max_excerpt")]
    pub max_excerpt_chars: usize,
    #[serde(default = "default_max_html_context_chars")]
    pub max_html_context_chars: usize,
    #[serde(default = "default_max_html_bytes")]
    pub max_html_bytes: usize,
    #[serde(default = "default_ttl_seconds")]
    pub ttl_seconds: i32,
    #[serde(default = "default_fetch_timeout")]
    pub fetch_timeout_seconds: u64,
    #[serde(default = "default_pool_size")]
    pub database_pool_size: u32,
}

fn default_queue() -> String {
    "page-fetcher".into()
}

fn default_stream() -> String {
    "page-fetch-jobs".into()
}

fn default_stream_start_id() -> String {
    "$".into()
}

fn default_metrics_host() -> String {
    "0.0.0.0".into()
}

fn default_metrics_port() -> u16 {
    19025
}

const fn default_max_excerpt() -> usize {
    4_000
}

const fn default_max_html_context_chars() -> usize {
    120_000
}

const fn default_max_html_bytes() -> usize {
    524_288
}

const fn default_ttl_seconds() -> i32 {
    21_600 // 6 hours
}

const fn default_fetch_timeout() -> u64 {
    60
}

const fn default_pool_size() -> u32 {
    5
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CrawlApiResponse {
    status: String,
    markdown_text: String,
    cleaned_text: String,
    content_type: String,
    language: Option<String>,
    title: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug)]
struct FetchFailure {
    fetch_status: String,
    fetch_reason: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct CrawlRequestPayload {
    url: String,
    normalized_key: String,
    max_html_bytes: usize,
    max_text_chars: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: WorkerConfig = load_config("config/page-fetcher.json")?;
    let pool = PgPoolOptions::new()
        .max_connections(cfg.database_pool_size)
        .connect(&cfg.database_url)
        .await?;
    let http_client = Client::builder()
        .timeout(std::time::Duration::from_secs(cfg.fetch_timeout_seconds))
        .build()?;

    info!(
        target = "svc-page-fetcher",
        stream = %cfg.stream,
        queue = %cfg.queue_name,
        "page fetcher initialized"
    );

    let metrics = MetricsServer::new();
    let metrics_task = metrics.run(cfg.metrics_host.clone(), cfg.metrics_port);

    let worker = PageFetcher::new(cfg, pool, http_client, metrics.clone()).await?;
    let worker_task = worker.run();

    tokio::select! {
        result = worker_task => {
            if let Err(err) = result {
                error!(target = "svc-page-fetcher", %err, "worker exited");
            }
        }
        result = metrics_task => {
            if let Err(err) = result {
                error!(target = "svc-page-fetcher", %err, "metrics server exited");
            }
        }
        _ = shutdown_signal() => {
            info!(target = "svc-page-fetcher", "shutdown signal received");
        }
    }

    Ok(())
}

async fn shutdown_signal() {
    let _ = signal::ctrl_c().await;
}

struct PageFetcher {
    cfg: WorkerConfig,
    pool: PgPool,
    client: Client,
    metrics: MetricsServer,
}

impl PageFetcher {
    async fn new(
        cfg: WorkerConfig,
        pool: PgPool,
        client: Client,
        metrics: MetricsServer,
    ) -> Result<Self> {
        Ok(Self {
            cfg,
            pool,
            client,
            metrics,
        })
    }

    async fn run(self) -> Result<()> {
        let redis = redis::Client::open(self.cfg.redis_url.clone())?;
        let mut conn = redis.get_async_connection().await?;
        let options = StreamReadOptions::default().block(5000).count(10);
        let mut last_id = self.cfg.stream_start_id.clone();
        loop {
            let reply: StreamReadReply = conn
                .xread_options(&[&self.cfg.stream], &[last_id.as_str()], &options)
                .await?;
            for stream in &reply.keys {
                for entry in &stream.ids {
                    last_id = entry.id.clone();
                    if entry_too_old(&entry.id, 300_000) {
                        continue;
                    }
                    if let Some(payload) = entry.get::<String>("payload") {
                        if let Err(err) = self.process_job(&payload).await {
                            self.metrics.record_job_failed();
                            let key = serde_json::from_str::<PageFetchJob>(&payload)
                                .map(|job| job.normalized_key)
                                .unwrap_or_else(|_| "unknown".into());
                            error!(target = "svc-page-fetcher", %err, key, "job failed");
                        } else {
                            self.metrics.record_job_completed();
                        }
                    }
                }
            }
        }
    }

    async fn process_job(&self, payload: &str) -> Result<()> {
        self.metrics.record_job_started();
        let job: PageFetchJob = serde_json::from_str(payload)?;
        let ttl_seconds = job.ttl_seconds.unwrap_or(self.cfg.ttl_seconds);
        let url = Url::parse(&job.url).context("job url invalid")?;

        if self.content_fresh(&job.normalized_key).await? {
            self.metrics.record_job_skipped();
            info!(
                target = "svc-page-fetcher",
                key = job.normalized_key,
                "existing content still fresh; skipping"
            );
            return Ok(());
        }

        let start = Instant::now();
        let crawl_response = match self.fetch_content(&job, &url).await {
            Ok(response) => response,
            Err(fetch_failure) => {
                self.store_failure(&job, ttl_seconds, &fetch_failure).await?;
                self.metrics.record_crawl_failure();
                return Err(anyhow!(fetch_failure.detail));
            }
        };
        self.metrics
            .observe_fetch_latency(start.elapsed().as_secs_f64());
        self.store_content(&job, ttl_seconds, &crawl_response)
            .await?;
        info!(
            target = "svc-page-fetcher",
            key = job.normalized_key,
            "stored crawl content"
        );
        Ok(())
    }

    async fn content_fresh(&self, normalized_key: &str) -> Result<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM page_contents WHERE normalized_key = $1 AND expires_at > NOW() AND LOWER(fetch_status) = 'ok')",
        )
        .bind(normalized_key)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    async fn fetch_content(
        &self,
        job: &PageFetchJob,
        url: &Url,
    ) -> std::result::Result<CrawlApiResponse, FetchFailure> {
        let payload = CrawlRequestPayload {
            url: url.to_string(),
            normalized_key: job.normalized_key.clone(),
            max_html_bytes: self.cfg.max_html_bytes,
            max_text_chars: self.cfg.max_excerpt_chars,
        };

        let response = self
            .client
            .post(format!(
                "{}/crawl",
                self.cfg.crawl_service_url.trim_end_matches('/')
            ))
            .json(&payload)
            .send()
            .await
            .map_err(|err| FetchFailure {
                fetch_status: "failed".into(),
                fetch_reason: "crawl_service_unreachable".into(),
                detail: format!("crawl service request failed: {err}"),
            })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_else(|_| "<empty>".into());
            let (fetch_status, fetch_reason) = classify_crawl_failure(&body);
            return Err(FetchFailure {
                fetch_status,
                fetch_reason,
                detail: format!("crawl service error: {body}"),
            });
        }

        let parsed: CrawlApiResponse = response.json().await.map_err(|err| FetchFailure {
            fetch_status: "failed".into(),
            fetch_reason: "invalid_crawl_payload".into(),
            detail: format!("invalid crawl payload: {err}"),
        })?;
        if parsed.status != "ok" {
            return Err(FetchFailure {
                fetch_status: "failed".into(),
                fetch_reason: "crawl_status_non_ok".into(),
                detail: format!("crawl service returned status {}", parsed.status),
            });
        }
        Ok(parsed)
    }

    async fn store_failure(
        &self,
        job: &PageFetchJob,
        ttl_seconds: i32,
        failure: &FetchFailure,
    ) -> Result<()> {
        let version = self.next_version(&job.normalized_key).await?;
        sqlx::query(
            r#"INSERT INTO page_contents
                (id, normalized_key, fetch_version, content_type, content_hash,
                 text_excerpt, char_count, byte_count, fetch_status, fetch_reason,
                 ttl_seconds, fetched_at)
             VALUES ($1, $2, $3, NULL, NULL, NULL, NULL, NULL, $4, $5, $6, NOW())"#,
        )
        .bind(Uuid::new_v4())
        .bind(&job.normalized_key)
        .bind(version)
        .bind(&failure.fetch_status)
        .bind(&failure.fetch_reason)
        .bind(ttl_seconds)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn store_content(
        &self,
        job: &PageFetchJob,
        ttl_seconds: i32,
        crawl: &CrawlApiResponse,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};

        let text_excerpt = build_markdown_excerpt(
            &crawl.markdown_text,
            &crawl.cleaned_text,
            crawl.title.as_deref(),
            self.cfg.max_html_context_chars,
        );
        let excerpt_bytes = text_excerpt.as_bytes();
        let hash = Sha256::digest(text_excerpt.as_bytes());
        let hash_hex = format!("{:x}", hash);
        let version = self.next_version(&job.normalized_key).await?;

        sqlx::query(
            r#"INSERT INTO page_contents
                (id, normalized_key, fetch_version, content_type, content_hash,
                 text_excerpt, char_count, byte_count, fetch_status, fetch_reason,
                 ttl_seconds, fetched_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'ok', $9, $10, NOW())"#,
        )
        .bind(Uuid::new_v4())
        .bind(&job.normalized_key)
        .bind(version)
        .bind(&crawl.content_type)
        .bind(&hash_hex)
        .bind(&text_excerpt)
        .bind(text_excerpt.chars().count() as i32)
        .bind(excerpt_bytes.len() as i32)
        .bind(&job.hostname)
        .bind(ttl_seconds)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn next_version(&self, normalized_key: &str) -> Result<i64> {
        let current: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(fetch_version) FROM page_contents WHERE normalized_key = $1",
        )
        .bind(normalized_key)
        .fetch_one(&self.pool)
        .await?;
        Ok(current.unwrap_or(0) as i64 + 1)
    }
}

fn build_markdown_excerpt(
    markdown_text: &str,
    cleaned_text: &str,
    title: Option<&str>,
    max_chars: usize,
) -> String {
    let base = if !markdown_text.trim().is_empty() {
        markdown_text
    } else {
        cleaned_text
    };

    let mut normalized_lines = Vec::new();
    for line in base.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !normalized_lines
                .last()
                .map(|l: &String| l.is_empty())
                .unwrap_or(false)
            {
                normalized_lines.push(String::new());
            }
            continue;
        }
        normalized_lines.push(trimmed.to_string());
    }
    let mut excerpt = normalized_lines.join("\n").trim().to_string();
    if excerpt.is_empty() {
        excerpt = base.trim().to_string();
    }

    if let Some(title) = title.map(str::trim).filter(|value| !value.is_empty()) {
        excerpt = format!("# {}\n\n{}", title, excerpt);
    }

    truncate_chars(&excerpt, max_chars)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    let mut out = String::new();
    for ch in value.chars().take(limit) {
        out.push(ch);
    }
    out
}

fn classify_crawl_failure(detail: &str) -> (String, String) {
    let lowered = detail.to_ascii_lowercase();
    if lowered.contains("anti_bot_or_access_denied")
        || lowered.contains("http_403")
        || lowered.contains("access denied")
        || lowered.contains("captcha")
        || lowered.contains("http_429")
    {
        return ("blocked".into(), "anti_bot_or_access_denied".into());
    }
    if lowered.contains("name_not_resolved") || lowered.contains("dns") {
        return ("failed".into(), "dns_resolution_failed".into());
    }
    if lowered.contains("connection_refused") {
        return ("failed".into(), "connection_refused".into());
    }
    if lowered.contains("timed out") || lowered.contains("timeout") {
        return ("failed".into(), "timeout".into());
    }
    ("failed".into(), "crawl_failed".into())
}

fn entry_too_old(entry_id: &str, max_age_ms: u64) -> bool {
    let Some((millis, _)) = entry_id.split_once('-') else {
        return false;
    };
    let Ok(ts_ms) = millis.parse::<u64>() else {
        return false;
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(ts_ms);
    now_ms.saturating_sub(ts_ms) > max_age_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_markdown_excerpt_prefers_markdown_and_title() {
        let excerpt = build_markdown_excerpt(
            "## Welcome\n\nThis is **content**",
            "fallback text",
            Some("Example"),
            10_000,
        );
        assert!(excerpt.contains("# Example"));
        assert!(excerpt.contains("## Welcome"));
        assert!(excerpt.contains("**content**"));
    }

    #[test]
    fn build_markdown_excerpt_respects_limit() {
        let markdown = format!("{}", "a".repeat(5_000));
        let excerpt = build_markdown_excerpt(&markdown, "", None, 1_000);
        assert!(excerpt.chars().count() <= 1_000);
    }
}
