mod metrics;

use anyhow::{anyhow, Context, Result};
use common_types::PageFetchJob;
use config_core::load_config;
use metrics::MetricsServer;
use redis::streams::{StreamReadOptions, StreamReadReply};
use redis::AsyncCommands;
use regex::Regex;
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
    cleaned_text: String,
    raw_html: String,
    content_type: String,
    language: Option<String>,
    title: Option<String>,
    metadata: Option<Value>,
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
        let crawl_response = self.fetch_content(&job, &url).await?;
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
            "SELECT EXISTS(SELECT 1 FROM page_contents WHERE normalized_key = $1 AND expires_at > NOW())",
        )
        .bind(normalized_key)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    async fn fetch_content(&self, job: &PageFetchJob, url: &Url) -> Result<CrawlApiResponse> {
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
            .context("crawl service request failed")?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_else(|_| "<empty>".into());
            self.metrics.record_crawl_failure();
            return Err(anyhow!("crawl service error: {}", body));
        }

        let parsed: CrawlApiResponse = response.json().await.context("invalid crawl payload")?;
        if parsed.status != "ok" {
            self.metrics.record_crawl_failure();
            return Err(anyhow!("crawl service returned status {}", parsed.status));
        }
        Ok(parsed)
    }

    async fn store_content(
        &self,
        job: &PageFetchJob,
        ttl_seconds: i32,
        crawl: &CrawlApiResponse,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};

        let raw_bytes = crawl.raw_html.as_bytes();
        let text_excerpt = build_html_context(&crawl.raw_html, self.cfg.max_html_context_chars);
        let hash = Sha256::digest(text_excerpt.as_bytes());
        let hash_hex = format!("{:x}", hash);
        let version = self.next_version(&job.normalized_key).await?;

        sqlx::query(
            r#"INSERT INTO page_contents
                (id, normalized_key, fetch_version, content_type, content_hash, raw_bytes,
                 text_excerpt, char_count, byte_count, fetch_status, fetch_reason,
                 ttl_seconds, fetched_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'ok', $10, $11, NOW())"#,
        )
        .bind(Uuid::new_v4())
        .bind(&job.normalized_key)
        .bind(version)
        .bind(&crawl.content_type)
        .bind(&hash_hex)
        .bind(raw_bytes)
        .bind(&text_excerpt)
        .bind(text_excerpt.chars().count() as i32)
        .bind(raw_bytes.len() as i32)
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

fn build_html_context(raw_html: &str, max_chars: usize) -> String {
    let head = extract_html_section(raw_html, "head").unwrap_or_default();
    let title = extract_html_section(raw_html, "title").unwrap_or_default();
    let body = extract_html_section(raw_html, "body").unwrap_or_else(|| raw_html.to_string());

    let title_budget = max_chars.min(2_000);
    let head_budget = max_chars.saturating_sub(title_budget).min(30_000);
    let used_title = truncate_chars(&title, title_budget);
    let used_head = truncate_chars(&head, head_budget);
    let remaining = max_chars
        .saturating_sub(used_title.chars().count())
        .saturating_sub(used_head.chars().count());
    let used_body = truncate_chars(&body, remaining);

    format!(
        "[HEAD]\n{}\n[/HEAD]\n[TITLE]\n{}\n[/TITLE]\n[BODY]\n{}\n[/BODY]",
        used_head, used_title, used_body
    )
}

fn extract_html_section(raw_html: &str, tag: &str) -> Option<String> {
    let pattern = format!(r"(?is)<{tag}\b[^>]*>(.*?)</{tag}>");
    let regex = Regex::new(&pattern).ok()?;
    let captures = regex.captures(raw_html)?;
    captures
        .get(1)
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
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
    fn build_html_context_extracts_head_title_body() {
        let html = r#"<html><head><meta charset=\"utf-8\"><title>Example</title></head><body><h1>Hello</h1></body></html>"#;
        let context = build_html_context(html, 10_000);
        assert!(context.contains("[HEAD]"));
        assert!(context.contains("meta charset"));
        assert!(context.contains("[TITLE]"));
        assert!(context.contains("Example"));
        assert!(context.contains("[BODY]"));
        assert!(context.contains("<h1>Hello</h1>"));
    }

    #[test]
    fn build_html_context_respects_limit() {
        let html = format!(
            "<html><head><title>T</title></head><body>{}</body></html>",
            "a".repeat(5_000)
        );
        let context = build_html_context(&html, 1_000);
        assert!(context.chars().count() <= 1_100);
    }
}
