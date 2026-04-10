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
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{collections::HashMap, env};
use tokio::signal;
use tokio::time::Instant;
use tracing::{error, info, warn, Level};
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
    #[serde(default = "default_idempotency_ttl_seconds")]
    pub idempotency_ttl_seconds: u64,
    #[serde(default = "default_pool_size")]
    pub database_pool_size: u32,
    #[serde(default = "default_terminal_retry_cooldown_seconds")]
    pub terminal_retry_cooldown_seconds: u64,
    #[serde(default = "default_blocked_retry_cooldown_seconds")]
    pub blocked_retry_cooldown_seconds: u64,
    #[serde(default = "default_unsupported_retry_cooldown_seconds")]
    pub unsupported_retry_cooldown_seconds: u64,
    #[serde(default)]
    pub unsupported_host_allowlist: Vec<String>,
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

const fn default_idempotency_ttl_seconds() -> u64 {
    86_400
}

const fn default_pool_size() -> u32 {
    5
}

const fn default_terminal_retry_cooldown_seconds() -> u64 {
    1_200
}

const fn default_blocked_retry_cooldown_seconds() -> u64 {
    14_400
}

const fn default_unsupported_retry_cooldown_seconds() -> u64 {
    21_600
}

fn check_config_mode_enabled() -> bool {
    std::env::args().any(|arg| arg == "--check-config")
}

fn default_stream_consumer_name(prefix: &str) -> String {
    let host = env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "local".to_string());
    let sanitized_host: String = host
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    format!("{}-{}-{}", prefix, sanitized_host, std::process::id())
}

fn validate_config(cfg: &WorkerConfig) -> Result<()> {
    let mut validator = config_core::ConfigValidator::new("page-fetcher");
    validator.require_non_empty(
        "queue_name",
        Some(cfg.queue_name.as_str()),
        "set queue_name in config/page-fetcher.json",
    );
    validator.require_non_empty(
        "redis_url",
        Some(cfg.redis_url.as_str()),
        "set redis_url in config/page-fetcher.json",
    );
    validator.require_non_empty(
        "crawl_service_url",
        Some(cfg.crawl_service_url.as_str()),
        "set crawl_service_url in config/page-fetcher.json",
    );
    validator.require_non_empty(
        "database_url",
        Some(cfg.database_url.as_str()),
        "set database_url in config/page-fetcher.json",
    );
    validator.finish()
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

#[derive(Debug, Clone, Serialize)]
struct CrawlAttempt {
    url: String,
    outcome: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct CrawlErrorDetail {
    report: Option<String>,
    reason: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrawlErrorEnvelope {
    detail: Option<CrawlErrorDetail>,
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
    validate_config(&cfg)?;
    if check_config_mode_enabled() {
        println!("page-fetcher config check passed");
        return Ok(());
    }
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
        let stream_group =
            env::var("OD_PAGE_FETCH_STREAM_GROUP").unwrap_or_else(|_| "page-fetcher".into());
        let stream_consumer = env::var("OD_PAGE_FETCH_STREAM_CONSUMER")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default_stream_consumer_name(&self.cfg.queue_name));
        let start_id = env::var("OD_PAGE_FETCH_STREAM_GROUP_START_ID")
            .unwrap_or_else(|_| self.cfg.stream_start_id.clone());
        let dead_letter_stream = env::var("OD_PAGE_FETCH_STREAM_DEAD_LETTER")
            .unwrap_or_else(|_| "page-fetch-jobs-dlq".into());
        let claim_idle_ms = env::var("OD_PAGE_FETCH_STREAM_CLAIM_IDLE_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30_000);
        let claim_batch = env::var("OD_PAGE_FETCH_STREAM_CLAIM_BATCH")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(25);
        info!(
            target = "svc-page-fetcher",
            stream_group = %stream_group,
            stream_consumer = %stream_consumer,
            "using stream consumer identity"
        );
        ensure_stream_group(&mut conn, &self.cfg.stream, &stream_group, &start_id).await?;
        let options_pending = StreamReadOptions::default()
            .group(&stream_group, &stream_consumer)
            .count(10);
        let options = StreamReadOptions::default()
            .group(&stream_group, &stream_consumer)
            .block(5000)
            .count(10);
        loop {
            let _ = claim_stale_entries(
                &mut conn,
                &self.cfg.stream,
                &stream_group,
                &stream_consumer,
                claim_idle_ms,
                claim_batch,
            )
            .await;

            let pending_reply: StreamReadReply = conn
                .xread_options(&[&self.cfg.stream], &["0"], &options_pending)
                .await?;
            for stream in &pending_reply.keys {
                for entry in &stream.ids {
                    if entry_too_old(&entry.id, 300_000) {
                        let _ = redis::cmd("XACK")
                            .arg(&self.cfg.stream)
                            .arg(&stream_group)
                            .arg(&entry.id)
                            .query_async::<_, i64>(&mut conn)
                            .await;
                        continue;
                    }
                    if let Some(payload) = entry.get::<String>("payload") {
                        let parseable = serde_json::from_str::<PageFetchJob>(&payload).is_ok();
                        if let Err(err) = self.process_job(&payload).await {
                            self.metrics.record_job_failed();
                            let key = serde_json::from_str::<PageFetchJob>(&payload)
                                .map(|job| job.normalized_key)
                                .unwrap_or_else(|_| "unknown".into());
                            error!(target = "svc-page-fetcher", %err, key, "pending job failed");
                            if !parseable {
                                let _ = publish_dead_letter(
                                    &mut conn,
                                    &dead_letter_stream,
                                    &self.cfg.stream,
                                    &entry.id,
                                    "invalid_payload",
                                    1,
                                    None,
                                    Some(&payload),
                                )
                                .await;
                            }
                        } else {
                            self.metrics.record_job_completed();
                        }
                    } else {
                        warn!(target = "svc-page-fetcher", entry_id = %entry.id, "pending stream entry missing payload field");
                        let _ = publish_dead_letter(
                            &mut conn,
                            &dead_letter_stream,
                            &self.cfg.stream,
                            &entry.id,
                            "missing_payload",
                            1,
                            None,
                            None,
                        )
                        .await;
                    }
                    let _ = redis::cmd("XACK")
                        .arg(&self.cfg.stream)
                        .arg(&stream_group)
                        .arg(&entry.id)
                        .query_async::<_, i64>(&mut conn)
                        .await;
                }
            }

            let reply: StreamReadReply = conn
                .xread_options(&[&self.cfg.stream], &[">"], &options)
                .await?;
            for stream in &reply.keys {
                for entry in &stream.ids {
                    if entry_too_old(&entry.id, 300_000) {
                        let _ = redis::cmd("XACK")
                            .arg(&self.cfg.stream)
                            .arg(&stream_group)
                            .arg(&entry.id)
                            .query_async::<_, i64>(&mut conn)
                            .await;
                        continue;
                    }
                    if let Some(payload) = entry.get::<String>("payload") {
                        let parseable = serde_json::from_str::<PageFetchJob>(&payload).is_ok();
                        if let Err(err) = self.process_job(&payload).await {
                            self.metrics.record_job_failed();
                            let key = serde_json::from_str::<PageFetchJob>(&payload)
                                .map(|job| job.normalized_key)
                                .unwrap_or_else(|_| "unknown".into());
                            error!(target = "svc-page-fetcher", %err, key, "job failed");
                            if !parseable {
                                let _ = publish_dead_letter(
                                    &mut conn,
                                    &dead_letter_stream,
                                    &self.cfg.stream,
                                    &entry.id,
                                    "invalid_payload",
                                    1,
                                    None,
                                    Some(&payload),
                                )
                                .await;
                            }
                        } else {
                            self.metrics.record_job_completed();
                        }
                    } else {
                        warn!(target = "svc-page-fetcher", entry_id = %entry.id, "stream entry missing payload field");
                        let _ = publish_dead_letter(
                            &mut conn,
                            &dead_letter_stream,
                            &self.cfg.stream,
                            &entry.id,
                            "missing_payload",
                            1,
                            None,
                            None,
                        )
                        .await;
                    }
                    let _ = redis::cmd("XACK")
                        .arg(&self.cfg.stream)
                        .arg(&stream_group)
                        .arg(&entry.id)
                        .query_async::<_, i64>(&mut conn)
                        .await;
                }
            }
        }
    }

    async fn process_job(&self, payload: &str) -> Result<()> {
        self.metrics.record_job_started();
        let job: PageFetchJob = serde_json::from_str(payload)?;
        let idempotency_key = page_fetch_idempotency_key(&job);
        if self.idempotency_key_seen(&idempotency_key).await? {
            self.metrics.record_job_duplicate();
            info!(
                target = "svc-page-fetcher",
                key = %job.normalized_key,
                idempotency_key = %idempotency_key,
                "duplicate page fetch job skipped"
            );
            return Ok(());
        }
        let ttl_seconds = job.ttl_seconds.unwrap_or(self.cfg.ttl_seconds);

        if self.content_fresh(&job.normalized_key).await? {
            self.metrics.record_job_skipped();
            info!(
                target = "svc-page-fetcher",
                key = job.normalized_key,
                "existing content still fresh; skipping"
            );
            self.idempotency_mark_processed(&idempotency_key).await?;
            return Ok(());
        }

        if self
            .terminal_fetch_recently_recorded(&job.normalized_key)
            .await?
        {
            self.metrics.record_terminal_skip();
            self.metrics.record_job_skipped();
            info!(
                target = "svc-page-fetcher",
                key = job.normalized_key,
                "recent terminal fetch outcome still in cooldown; skipping"
            );
            self.idempotency_mark_processed(&idempotency_key).await?;
            return Ok(());
        }

        let candidates = resolve_candidate_urls(&job)?;
        let mut attempts: Vec<CrawlAttempt> = Vec::new();
        let mut last_failure: Option<FetchFailure> = None;
        let mut dns_resolution_cache: HashMap<String, bool> = HashMap::new();
        let mut had_crawl_attempt = false;

        for candidate in candidates {
            let parsed = Url::parse(&candidate).context("candidate url invalid")?;
            let host = parsed.host_str().unwrap_or_default();
            if host.is_empty() {
                attempts.push(CrawlAttempt {
                    url: candidate,
                    outcome: "failed".into(),
                    reason: "invalid_candidate_host".into(),
                });
                last_failure = Some(FetchFailure {
                    fetch_status: "failed".into(),
                    fetch_reason: "invalid_candidate_host".into(),
                    detail: "candidate URL is missing a hostname".into(),
                });
                continue;
            }
            if is_likely_asset_host(host, &self.cfg.unsupported_host_allowlist) {
                self.metrics.record_asset_prefilter_skip();
                attempts.push(CrawlAttempt {
                    url: candidate,
                    outcome: "unsupported".into(),
                    reason: "asset_endpoint".into(),
                });
                continue;
            }

            if !self.host_resolves(host, &mut dns_resolution_cache).await {
                attempts.push(CrawlAttempt {
                    url: candidate,
                    outcome: "unsupported".into(),
                    reason: "dns_unresolvable".into(),
                });
                continue;
            }

            had_crawl_attempt = true;
            let start = Instant::now();
            match self.fetch_content(&job, &candidate).await {
                Ok(response) => {
                    self.metrics
                        .observe_fetch_latency(start.elapsed().as_secs_f64());
                    attempts.push(CrawlAttempt {
                        url: candidate.clone(),
                        outcome: "ok".into(),
                        reason: "ok".into(),
                    });
                    self.store_content(&job, ttl_seconds, &response, &candidate, &attempts)
                        .await?;
                    info!(
                        target = "svc-page-fetcher",
                        key = job.normalized_key,
                        resolved_url = candidate,
                        attempts = attempts.len(),
                        "stored crawl content"
                    );
                    self.idempotency_mark_processed(&idempotency_key).await?;
                    return Ok(());
                }
                Err(fetch_failure) => {
                    attempts.push(CrawlAttempt {
                        url: candidate,
                        outcome: fetch_failure.fetch_status.clone(),
                        reason: fetch_failure.fetch_reason.clone(),
                    });
                    last_failure = Some(fetch_failure);
                }
            }
        }

        let all_candidates_dns_unresolvable = !attempts.is_empty()
            && attempts
                .iter()
                .all(|attempt| attempt.reason == "dns_unresolvable");
        let no_candidates_reached_crawl = !had_crawl_attempt;

        let failure = if no_candidates_reached_crawl && all_candidates_dns_unresolvable {
            self.metrics.record_terminal_dns_unresolvable();
            FetchFailure {
                fetch_status: "unsupported".into(),
                fetch_reason: "dns_unresolvable".into(),
                detail: "all candidate hosts failed DNS preflight before Crawl4AI".into(),
            }
        } else {
            last_failure.unwrap_or(FetchFailure {
                fetch_status: "unsupported".into(),
                fetch_reason: "all_candidates_filtered".into(),
                detail: "all candidate URLs were filtered as non-page endpoints".into(),
            })
        };
        self.store_failure(&job, ttl_seconds, &failure, &attempts)
            .await?;
        self.metrics.record_crawl_failure();
        Err(anyhow!(failure.detail))
    }

    async fn idempotency_key_seen(&self, idempotency_key: &str) -> Result<bool> {
        let mut conn = redis::Client::open(self.cfg.redis_url.clone())?
            .get_async_connection()
            .await?;
        let key = format!("od:idempotency:page-fetch:{}", idempotency_key);
        let exists = redis::cmd("EXISTS")
            .arg(&key)
            .query_async::<_, i64>(&mut conn)
            .await?;
        Ok(exists > 0)
    }

    async fn idempotency_mark_processed(&self, idempotency_key: &str) -> Result<()> {
        let mut conn = redis::Client::open(self.cfg.redis_url.clone())?
            .get_async_connection()
            .await?;
        let key = format!("od:idempotency:page-fetch:{}", idempotency_key);
        redis::cmd("SET")
            .arg(&key)
            .arg("1")
            .arg("EX")
            .arg(self.cfg.idempotency_ttl_seconds as i64)
            .query_async::<_, ()>(&mut conn)
            .await?;
        Ok(())
    }

    async fn host_resolves(&self, host: &str, cache: &mut HashMap<String, bool>) -> bool {
        if let Some(resolved) = cache.get(host) {
            return *resolved;
        }

        self.metrics.record_dns_preflight_check();
        let resolved = tokio::net::lookup_host((host, 443))
            .await
            .map(|mut addrs| addrs.next().is_some())
            .unwrap_or(false);

        if resolved {
            self.metrics.record_dns_preflight_resolved();
        } else {
            self.metrics.record_dns_preflight_unresolved();
        }

        cache.insert(host.to_string(), resolved);
        resolved
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

    async fn terminal_fetch_recently_recorded(&self, normalized_key: &str) -> Result<bool> {
        let row = sqlx::query(
            "SELECT LOWER(fetch_status) AS fetch_status, EXTRACT(EPOCH FROM (NOW() - fetched_at))::bigint AS age_seconds FROM page_contents WHERE normalized_key = $1 ORDER BY fetched_at DESC LIMIT 1",
        )
        .bind(normalized_key)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(false);
        };

        let status = row
            .try_get::<Option<String>, _>("fetch_status")?
            .unwrap_or_default();
        let age_seconds = row
            .try_get::<Option<i64>, _>("age_seconds")?
            .unwrap_or(i64::MAX)
            .max(0) as u64;

        let cooldown = match status.as_str() {
            "blocked" => self.cfg.blocked_retry_cooldown_seconds,
            "unsupported" => self.cfg.unsupported_retry_cooldown_seconds,
            "failed" => self.cfg.terminal_retry_cooldown_seconds,
            _ => return Ok(false),
        };
        Ok(age_seconds <= cooldown)
    }

    async fn fetch_content(
        &self,
        job: &PageFetchJob,
        url: &str,
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
            let status_code = response.status().as_u16();
            let body = response.text().await.unwrap_or_else(|_| "<empty>".into());
            if let Some(failure) = parse_crawl_error_payload(&body) {
                return Err(failure);
            }
            let (fetch_status, fetch_reason) = classify_crawl_failure(&body, Some(status_code));
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
        attempts: &[CrawlAttempt],
    ) -> Result<()> {
        let version = self.next_version(&job.normalized_key).await?;
        let attempt_summary = serde_json::to_string(attempts)?;
        sqlx::query(
            r#"INSERT INTO page_contents
                (id, normalized_key, fetch_version, content_type, content_hash,
                  text_excerpt, char_count, byte_count, fetch_status, fetch_reason,
                  ttl_seconds, fetched_at, source_url, resolved_url, attempt_summary)
             VALUES ($1, $2, $3, NULL, NULL, NULL, NULL, NULL, $4, $5, $6, NOW(), $7, NULL, $8)"#,
        )
        .bind(Uuid::new_v4())
        .bind(&job.normalized_key)
        .bind(version)
        .bind(&failure.fetch_status)
        .bind(&failure.fetch_reason)
        .bind(ttl_seconds)
        .bind(&job.url)
        .bind(&attempt_summary)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn store_content(
        &self,
        job: &PageFetchJob,
        ttl_seconds: i32,
        crawl: &CrawlApiResponse,
        resolved_url: &str,
        attempts: &[CrawlAttempt],
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
        let attempt_summary = serde_json::to_string(attempts)?;
        let resolved_host = Url::parse(resolved_url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_string))
            .unwrap_or_else(|| job.hostname.clone());

        sqlx::query(
            r#"INSERT INTO page_contents
                (id, normalized_key, fetch_version, content_type, content_hash,
                  text_excerpt, char_count, byte_count, fetch_status, fetch_reason,
                  ttl_seconds, fetched_at, source_url, resolved_url, attempt_summary)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'ok', $9, $10, NOW(), $11, $12, $13)"#,
        )
        .bind(Uuid::new_v4())
        .bind(&job.normalized_key)
        .bind(version)
        .bind(&crawl.content_type)
        .bind(&hash_hex)
        .bind(&text_excerpt)
        .bind(text_excerpt.chars().count() as i32)
        .bind(excerpt_bytes.len() as i32)
        .bind(&resolved_host)
        .bind(ttl_seconds)
        .bind(&job.url)
        .bind(resolved_url)
        .bind(&attempt_summary)
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

fn parse_crawl_error_payload(payload: &str) -> Option<FetchFailure> {
    let parsed = serde_json::from_str::<CrawlErrorEnvelope>(payload).ok()?;
    let detail = parsed.detail?;
    let report = detail.report?.to_ascii_lowercase();
    let fetch_status = match report.as_str() {
        "blocked" => "blocked",
        "unsupported" => "unsupported",
        _ => "failed",
    }
    .to_string();
    let fetch_reason = detail.reason.unwrap_or_else(|| "crawl_failed".into());
    let message = detail
        .detail
        .unwrap_or_else(|| "crawl service error".into());
    Some(FetchFailure {
        fetch_status,
        fetch_reason,
        detail: message,
    })
}

fn classify_crawl_failure(detail: &str, status_code: Option<u16>) -> (String, String) {
    let lowered = detail.to_ascii_lowercase();
    if matches!(status_code, Some(403)) {
        return ("blocked".into(), "http_403".into());
    }
    if matches!(status_code, Some(429)) {
        return ("blocked".into(), "http_429".into());
    }
    if lowered.contains("minimal_text") || lowered.contains("no_content_elements") {
        return ("unsupported".into(), "no_content_endpoint".into());
    }
    if lowered.contains("anti_bot_or_access_denied")
        || lowered.contains("http_403")
        || lowered.contains("http_429")
        || lowered.contains("access denied")
        || lowered.contains("captcha")
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

fn is_likely_asset_host(hostname: &str, allowlist: &[String]) -> bool {
    if allowlist.iter().any(|allowed| {
        hostname.eq_ignore_ascii_case(allowed) || hostname.ends_with(&format!(".{allowed}"))
    }) {
        return false;
    }
    let lowered = hostname.to_ascii_lowercase();
    let labels: Vec<&str> = lowered.split('.').collect();
    labels.iter().any(|label| {
        matches!(
            *label,
            "cdn" | "cdns" | "img" | "images" | "image" | "static" | "assets" | "media" | "js"
        )
    })
}

fn resolve_candidate_urls(job: &PageFetchJob) -> Result<Vec<String>> {
    let mut values = Vec::new();
    values.push(job.url.clone());
    values.extend(job.candidate_urls.clone());
    let mut deduped = Vec::new();
    for value in values {
        if Url::parse(&value).is_err() {
            continue;
        }
        if !deduped.contains(&value) {
            deduped.push(value);
        }
    }
    if deduped.is_empty() {
        return Err(anyhow!("no valid candidate URLs in job payload"));
    }
    Ok(deduped)
}

fn page_fetch_idempotency_key(job: &PageFetchJob) -> String {
    job.idempotency_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| {
            let trace = job.trace_id.clone().unwrap_or_else(|| "none".to_string());
            format!("{}:{}:{}", job.normalized_key, job.url, trace)
        })
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

async fn ensure_stream_group(
    conn: &mut redis::aio::Connection,
    stream: &str,
    group: &str,
    start_id: &str,
) -> Result<(), redis::RedisError> {
    match redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(stream)
        .arg(group)
        .arg(start_id)
        .arg("MKSTREAM")
        .query_async::<_, String>(conn)
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => {
            if err.to_string().contains("BUSYGROUP") {
                Ok(())
            } else {
                Err(err)
            }
        }
    }
}

async fn claim_stale_entries(
    conn: &mut redis::aio::Connection,
    stream: &str,
    group: &str,
    consumer: &str,
    min_idle_ms: u64,
    batch: usize,
) -> Result<(), redis::RedisError> {
    let _: redis::Value = redis::cmd("XAUTOCLAIM")
        .arg(stream)
        .arg(group)
        .arg(consumer)
        .arg(min_idle_ms)
        .arg("0-0")
        .arg("COUNT")
        .arg(batch.max(1) as i64)
        .arg("JUSTID")
        .query_async(conn)
        .await?;
    Ok(())
}

async fn publish_dead_letter(
    conn: &mut redis::aio::Connection,
    dlq_stream: &str,
    source_stream: &str,
    entry_id: &str,
    reason: &str,
    delivery_count: u64,
    trace_id: Option<&str>,
    payload: Option<&str>,
) -> Result<(), redis::RedisError> {
    let first_seen_at = entry_id_timestamp_iso8601(entry_id).unwrap_or_default();
    let last_seen_at = chrono::Utc::now().to_rfc3339();
    redis::cmd("XADD")
        .arg(dlq_stream)
        .arg("*")
        .arg("source_stream")
        .arg(source_stream)
        .arg("entry_id")
        .arg(entry_id)
        .arg("reason")
        .arg(reason)
        .arg("delivery_count")
        .arg(delivery_count as i64)
        .arg("first_seen_at")
        .arg(first_seen_at)
        .arg("last_seen_at")
        .arg(last_seen_at)
        .arg("trace_id")
        .arg(trace_id.unwrap_or_default())
        .arg("payload")
        .arg(payload.unwrap_or_default())
        .query_async::<_, ()>(conn)
        .await?;
    metrics::record_dlq_published(reason);
    Ok(())
}

fn entry_id_timestamp_iso8601(entry_id: &str) -> Option<String> {
    let (millis, _) = entry_id.split_once('-')?;
    let ts_ms = millis.parse::<i64>().ok()?;
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ts_ms)?;
    Some(dt.to_rfc3339())
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

    #[test]
    fn classify_crawl_failure_marks_structural_as_unsupported() {
        let (status, reason) = classify_crawl_failure(
            "crawl4ai returned unsuccessful result: Structural: minimal_text, no_content_elements",
            Some(200),
        );
        assert_eq!(status, "unsupported");
        assert_eq!(reason, "no_content_endpoint");
    }

    #[test]
    fn parse_crawl_error_payload_uses_structured_fields() {
        let payload = r#"{"detail":{"error_code":"CRAWL_OUTCOME_ERROR","report":"blocked","reason":"http_403","status_code":403,"detail":"blocked by anti-bot"}}"#;
        let parsed = parse_crawl_error_payload(payload).expect("expected parsed failure");
        assert_eq!(parsed.fetch_status, "blocked");
        assert_eq!(parsed.fetch_reason, "http_403");
    }

    #[test]
    fn asset_host_prefilter_detects_common_labels() {
        assert!(is_likely_asset_host("cdn.fundraiseup.com", &[]));
        assert!(is_likely_asset_host("js.stripe.com", &[]));
        assert!(!is_likely_asset_host("www.mozilla.org", &[]));
        assert!(!is_likely_asset_host(
            "cdn.allowed.example.com",
            &["allowed.example.com".to_string()]
        ));
    }

    #[test]
    fn resolve_candidate_urls_preserves_order_and_dedupes() {
        let job = PageFetchJob {
            normalized_key: "domain:example.com".to_string(),
            url: "https://example.com/".to_string(),
            hostname: "example.com".to_string(),
            candidate_urls: vec![
                "https://example.com/".to_string(),
                "https://www.example.com/".to_string(),
            ],
            trace_id: None,
            idempotency_key: None,
            ttl_seconds: None,
        };
        let resolved = resolve_candidate_urls(&job).expect("should resolve candidates");
        assert_eq!(
            resolved,
            vec!["https://example.com/", "https://www.example.com/"]
        );
    }

    #[test]
    fn dns_terminal_detection_requires_all_candidates_dns_unresolvable() {
        let attempts = vec![
            CrawlAttempt {
                url: "https://example.com/".into(),
                outcome: "unsupported".into(),
                reason: "dns_unresolvable".into(),
            },
            CrawlAttempt {
                url: "https://www.example.com/".into(),
                outcome: "unsupported".into(),
                reason: "dns_unresolvable".into(),
            },
        ];
        assert!(attempts
            .iter()
            .all(|attempt| attempt.reason == "dns_unresolvable"));

        let mixed_attempts = vec![
            CrawlAttempt {
                url: "https://example.com/".into(),
                outcome: "unsupported".into(),
                reason: "asset_endpoint".into(),
            },
            CrawlAttempt {
                url: "https://www.example.com/".into(),
                outcome: "unsupported".into(),
                reason: "dns_unresolvable".into(),
            },
        ];
        assert!(!mixed_attempts
            .iter()
            .all(|attempt| attempt.reason == "dns_unresolvable"));
    }
}
