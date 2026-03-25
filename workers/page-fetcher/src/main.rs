mod metrics;

use anyhow::{anyhow, Context, Result};
use common_types::PageFetchJob;
use config_core::load_config;
use metrics::MetricsServer;
use redis::streams::{StreamReadOptions, StreamReadReply};
use redis::AsyncCommands;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
    pub redis_url: String,
    pub crawl_service_url: String,
    pub database_url: String,
    #[serde(default = "default_metrics_host")]
    pub metrics_host: String,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
    #[serde(default = "default_max_excerpt")]
    pub max_excerpt_chars: usize,
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

fn default_metrics_host() -> String {
    "0.0.0.0".into()
}

fn default_metrics_port() -> u16 {
    19025
}

const fn default_max_excerpt() -> usize {
    4_000
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
        let mut last_id = "0-0".to_string();
        loop {
            let reply: StreamReadReply = conn
                .xread_options(&[&self.cfg.stream], &[last_id.as_str()], &options)
                .await?;
            for stream in &reply.keys {
                for entry in &stream.ids {
                    last_id = entry.id.clone();
                    if let Some(payload) = entry.get::<String>("payload") {
                        if let Err(err) = self.process_job(&payload).await {
                            self.metrics.record_job_failed();
                            error!(target = "svc-page-fetcher", %err, "job failed");
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
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "<empty>".into());
            if let Ok(fallback) = self.fetch_via_http(job, url).await {
                info!(
                    target = "svc-page-fetcher",
                    key = job.normalized_key,
                    status = ?status,
                    "crawl service error ({body}); switching to HTTP fallback"
                );
                return Ok(fallback);
            }
            self.metrics.record_crawl_failure();
            return Err(anyhow!("crawl service error: {}", body));
        }

        let parsed: CrawlApiResponse = response.json().await.context("invalid crawl payload")?;
        if parsed.status != "ok" {
            if let Ok(fallback) = self.fetch_via_http(job, url).await {
                info!(
                    target = "svc-page-fetcher",
                    key = job.normalized_key,
                    status = %parsed.status,
                    "crawl status not ok; using HTTP fallback"
                );
                return Ok(fallback);
            }
            self.metrics.record_crawl_failure();
            return Err(anyhow!("crawl service returned status {}", parsed.status));
        }
        Ok(parsed)
    }

    async fn fetch_via_http(&self, _job: &PageFetchJob, url: &Url) -> Result<CrawlApiResponse> {
        let response = self
            .client
            .get(url.as_ref())
            .header("User-Agent", "OpenDefenderFallback/1.0")
            .send()
            .await
            .context("fallback request failed")?;

        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        if !status.is_success() {
            return Err(anyhow!("fallback request failed with status {}", status));
        }

        let mut raw_html = response.text().await.unwrap_or_default();
        let mut cleaned_text = raw_html.trim().to_string();
        if cleaned_text.chars().count() > self.cfg.max_excerpt_chars {
            cleaned_text = cleaned_text
                .chars()
                .take(self.cfg.max_excerpt_chars)
                .collect();
        }

        let mut raw_bytes = raw_html.into_bytes();
        if raw_bytes.len() > self.cfg.max_html_bytes {
            raw_bytes.truncate(self.cfg.max_html_bytes);
        }
        raw_html = String::from_utf8(raw_bytes).unwrap_or_default();

        let metadata = json!({
            "fallback": "reqwest",
            "status_code": status.as_u16(),
        });

        Ok(CrawlApiResponse {
            status: "ok".into(),
            cleaned_text,
            raw_html,
            content_type,
            language: None,
            title: None,
            metadata: Some(metadata),
        })
    }

    async fn store_content(
        &self,
        job: &PageFetchJob,
        ttl_seconds: i32,
        crawl: &CrawlApiResponse,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};

        let raw_bytes = crawl.raw_html.as_bytes();
        let text_excerpt = crawl.cleaned_text.trim();
        let hash = Sha256::digest(crawl.cleaned_text.as_bytes());
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
        .bind(text_excerpt)
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
