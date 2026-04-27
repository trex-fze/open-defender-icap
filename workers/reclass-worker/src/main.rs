mod metrics;

use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use common_types::PageFetchJob;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Row, Transaction};
use std::sync::Arc;
use taxonomy::TaxonomyStore;
use tokio::{signal, time::Duration};
use tracing::{error, info, warn, Level};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
struct ReclassConfig {
    pub redis_url: String,
    pub job_stream: String,
    pub database_url: String,
    #[serde(default = "default_planner_interval_secs")]
    pub planner_interval_seconds: u64,
    #[serde(default = "default_planner_batch_size")]
    pub planner_batch_size: i64,
    #[serde(default = "default_dispatch_batch_size")]
    pub dispatcher_batch_size: i64,
    #[serde(default = "default_metrics_host")]
    pub metrics_host: String,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
    #[serde(default = "default_db_pool_size")]
    pub db_pool_size: u32,
    #[serde(default)]
    pub page_fetch_queue: Option<PageFetchQueueConfig>,
}

fn default_planner_interval_secs() -> u64 {
    60
}

fn default_planner_batch_size() -> i64 {
    200
}

fn default_dispatch_batch_size() -> i64 {
    200
}

fn default_metrics_host() -> String {
    "0.0.0.0".into()
}

fn default_metrics_port() -> u16 {
    19016
}

fn default_db_pool_size() -> u32 {
    5
}

fn derive_base_url(full_url: &str, hostname: &str) -> String {
    if let Ok(parsed) = Url::parse(full_url) {
        if let Some(host) = parsed.host_str() {
            return format!("{}://{host}/", parsed.scheme());
        }
    }
    format!("https://{hostname}/")
}

fn check_config_mode_enabled() -> bool {
    std::env::args().any(|arg| arg == "--check-config")
}

fn validate_config(cfg: &ReclassConfig) -> Result<()> {
    let mut validator = config_core::ConfigValidator::new("reclass-worker");
    validator.require_non_empty(
        "reclass.redis_url",
        Some(cfg.redis_url.as_str()),
        "set redis_url in config/reclass-worker.json",
    );
    validator.require_non_empty(
        "reclass.database_url",
        Some(cfg.database_url.as_str()),
        "set database_url in config/reclass-worker.json",
    );
    validator.require_auth_url(
        "reclass.redis_url",
        Some(cfg.redis_url.as_str()),
        false,
        true,
        16,
        "set redis_url in config/reclass-worker.json with password-authenticated Redis credentials",
    );
    validator.require_auth_url(
        "reclass.database_url",
        Some(cfg.database_url.as_str()),
        true,
        true,
        12,
        "set database_url in config/reclass-worker.json with non-default DB credentials",
    );
    if let Some(queue) = &cfg.page_fetch_queue {
        validator.require_auth_url(
            "reclass.page_fetch_queue.redis_url",
            Some(queue.redis_url.as_str()),
            false,
            true,
            16,
            "set page_fetch_queue.redis_url with password-authenticated Redis credentials",
        );
    }
    validator.finish()
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: ReclassConfig = config_core::load_config("config/reclass-worker.json")?;
    validate_config(&cfg)?;
    if check_config_mode_enabled() {
        println!("reclass-worker config check passed");
        return Ok(());
    }
    info!(
        target = "svc-reclass",
        stream = %cfg.job_stream,
        planner_interval = cfg.planner_interval_seconds,
        "reclassification worker starting"
    );

    let metrics_host = cfg.metrics_host.clone();
    let metrics_port = cfg.metrics_port;
    tokio::spawn(async move {
        if let Err(err) = metrics::serve_metrics(&metrics_host, metrics_port).await {
            error!(target = "svc-reclass", %err, "metrics server exited");
        }
    });

    let pool = PgPoolOptions::new()
        .max_connections(cfg.db_pool_size)
        .connect(&cfg.database_url)
        .await?;

    let taxonomy_store =
        Arc::new(TaxonomyStore::load_default().context("failed to load canonical taxonomy")?);

    let publisher = JobPublisher::new(&cfg.redis_url, &cfg.job_stream)?;
    let page_fetch_publisher = cfg
        .page_fetch_queue
        .as_ref()
        .map(|queue| PageFetchPublisher::new(&queue.redis_url, &queue.stream, queue.ttl_seconds))
        .transpose()?;

    let planner = Planner::new(
        pool.clone(),
        cfg.planner_batch_size,
        Duration::from_secs(cfg.planner_interval_seconds.max(5)),
        Arc::clone(&taxonomy_store),
    );
    let dispatcher = Dispatcher::new(
        pool.clone(),
        publisher.clone(),
        page_fetch_publisher.clone(),
        cfg.dispatcher_batch_size,
    );

    tokio::spawn(planner.run());
    tokio::spawn(dispatcher.run());

    signal::ctrl_c().await?;
    Ok(())
}

#[derive(Clone)]
struct JobPublisher {
    client: redis::Client,
    stream: String,
}

impl JobPublisher {
    fn new(redis_url: &str, stream: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            stream: stream.to_string(),
        })
    }

    async fn publish(&self, job: &ClassificationJobMessage) -> Result<()> {
        let mut conn = self.client.get_async_connection().await?;
        let payload = serde_json::to_string(job)?;
        let _: () = redis::cmd("XADD")
            .arg(&self.stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }
}

impl PageFetchPublisher {
    fn new(redis_url: &str, stream: &str, default_ttl: i32) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            stream: stream.to_string(),
            default_ttl: default_ttl.max(60),
        })
    }

    async fn publish(&self, mut job: PageFetchJob) -> Result<()> {
        if job.ttl_seconds.unwrap_or(0) <= 0 {
            job.ttl_seconds = Some(self.default_ttl);
        }
        let mut conn = self.client.get_async_connection().await?;
        let payload = serde_json::to_string(&job)?;
        redis::cmd("XADD")
            .arg(&self.stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async::<_, ()>(&mut conn)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct ClassificationJobMessage {
    normalized_key: String,
    entity_level: String,
    hostname: String,
    full_url: String,
    trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    idempotency_key: Option<String>,
    requires_content: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_language: Option<String>,
}

#[derive(Clone)]
struct PageFetchPublisher {
    client: redis::Client,
    stream: String,
    default_ttl: i32,
}

#[derive(Debug, Clone)]
struct PageContentSnippet {
    content_excerpt: Option<String>,
    content_hash: Option<String>,
    content_version: Option<i64>,
    content_language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PageFetchQueueConfig {
    pub redis_url: String,
    #[serde(default = "default_page_fetch_stream")]
    pub stream: String,
    #[serde(default = "default_page_fetch_ttl")]
    pub ttl_seconds: i32,
}

fn default_page_fetch_stream() -> String {
    "page-fetch-jobs".into()
}

const fn default_page_fetch_ttl() -> i32 {
    21_600
}

struct Planner {
    pool: PgPool,
    batch_size: i64,
    interval: Duration,
    taxonomy: Arc<TaxonomyStore>,
    taxonomy_version: String,
}

impl Planner {
    fn new(
        pool: PgPool,
        batch_size: i64,
        interval: Duration,
        taxonomy: Arc<TaxonomyStore>,
    ) -> Self {
        let taxonomy_version = taxonomy.taxonomy().version.clone();
        Self {
            pool,
            batch_size,
            interval,
            taxonomy,
            taxonomy_version,
        }
    }

    async fn run(self) {
        let mut ticker = tokio::time::interval(self.interval);
        loop {
            ticker.tick().await;
            match self.plan_batch().await {
                Ok(planned) => {
                    if planned > 0 {
                        info!(
                            target = "svc-reclass",
                            planned = planned,
                            "scheduled reclassification jobs"
                        );
                    }
                }
                Err(err) => {
                    error!(target = "svc-reclass", %err, "planner iteration failed");
                }
            }
        }
    }

    async fn plan_batch(&self) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            r#"
            SELECT id, normalized_key, primary_category, subcategory,
                   COALESCE(ttl_seconds, 3600) as ttl_seconds
            FROM classifications
            WHERE status = 'active'
              AND next_refresh_at IS NOT NULL
              AND next_refresh_at <= NOW()
            ORDER BY next_refresh_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT $1
            "#,
        )
        .bind(self.batch_size)
        .fetch_all(&mut *tx)
        .await?;

        if rows.is_empty() {
            tx.commit().await?;
            metrics::set_reclass_backlog(self.fetch_backlog().await?);
            return Ok(0);
        }

        let mut planned = 0usize;
        for row in &rows {
            let classification_id: Uuid = row.try_get("id")?;
            let normalized_key: String = row.try_get("normalized_key")?;
            let ttl_seconds: i32 = row.try_get::<i32, _>("ttl_seconds")?;
            let primary_category: String = row.try_get("primary_category")?;
            let subcategory: String = row.try_get("subcategory")?;

            self.ensure_canonical_labels(&mut tx, classification_id, primary_category, subcategory)
                .await?;
            let ttl = i64::from(ttl_seconds).max(300);
            let next_refresh = Utc::now() + ChronoDuration::seconds(ttl);

            sqlx::query(
                "UPDATE classifications SET next_refresh_at = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(next_refresh)
            .bind(classification_id)
            .execute(&mut *tx)
            .await?;

            let inserted = sqlx::query(
                r#"
                INSERT INTO reclassification_jobs (id, normalized_key, reason, status, created_at)
                SELECT $1, $2, $3, 'pending', NOW()
                WHERE NOT EXISTS (
                    SELECT 1 FROM reclassification_jobs
                    WHERE normalized_key = $2
                      AND status IN ('pending', 'running')
                )
                RETURNING id
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(&normalized_key)
            .bind("ttl_refresh")
            .fetch_optional(&mut *tx)
            .await?;

            if inserted.is_some() {
                planned += 1;
            }
        }

        tx.commit().await?;
        metrics::record_jobs_planned(planned as u64);
        metrics::set_reclass_backlog(self.fetch_backlog().await?);
        Ok(planned)
    }

    async fn ensure_canonical_labels(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        classification_id: Uuid,
        category: String,
        subcategory: String,
    ) -> Result<()> {
        let validated = self
            .taxonomy
            .validate_labels(&category, Some(subcategory.as_str()));
        let new_category = validated.category.id.clone();
        let new_subcategory = validated.subcategory.id.clone();
        let needs_update = validated.fallback_reason.is_some()
            || new_category != category
            || new_subcategory != subcategory;

        if needs_update {
            sqlx::query(
                "UPDATE classifications SET primary_category = $1, subcategory = $2, taxonomy_version = $3, updated_at = NOW() WHERE id = $4",
            )
            .bind(&new_category)
            .bind(&new_subcategory)
            .bind(&self.taxonomy_version)
            .bind(classification_id)
            .execute(&mut **tx)
            .await?;
            info!(
                target = "svc-reclass",
                classification_id = %classification_id,
                "canonicalized legacy classification labels"
            );
        }

        Ok(())
    }

    async fn fetch_backlog(&self) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM reclassification_jobs WHERE status = 'pending'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get::<i64, _>("count")?)
    }
}

struct Dispatcher {
    pool: PgPool,
    publisher: JobPublisher,
    page_fetch_publisher: Option<PageFetchPublisher>,
    batch_size: i64,
}

impl Dispatcher {
    fn new(
        pool: PgPool,
        publisher: JobPublisher,
        page_fetch_publisher: Option<PageFetchPublisher>,
        batch_size: i64,
    ) -> Self {
        Self {
            pool,
            publisher,
            page_fetch_publisher,
            batch_size,
        }
    }

    async fn run(self) {
        loop {
            match self.dispatch_batch().await {
                Ok(processed) => {
                    if processed == 0 {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
                Err(err) => {
                    error!(target = "svc-reclass", %err, "dispatcher iteration failed");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    async fn dispatch_batch(&self) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            r#"
            SELECT id, normalized_key, reason
            FROM reclassification_jobs
            WHERE status = 'pending'
            ORDER BY created_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT $1
            "#,
        )
        .bind(self.batch_size)
        .fetch_all(&mut *tx)
        .await?;

        if rows.is_empty() {
            tx.commit().await?;
            metrics::set_reclass_backlog(self.fetch_backlog().await?);
            return Ok(0);
        }

        for row in &rows {
            let job_id: Uuid = row.try_get("id")?;
            sqlx::query("UPDATE reclassification_jobs SET status = 'running', started_at = NOW() WHERE id = $1")
                .bind(job_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;

        let mut processed = 0usize;
        for row in rows {
            let job_id: Uuid = row.try_get("id")?;
            let normalized_key: String = row.try_get("normalized_key")?;
            let reason: String = row.try_get("reason")?;
            let trace_id = job_id.to_string();

            match build_target(&normalized_key) {
                Ok(target) => {
                    let page_content = self.load_page_content(&normalized_key).await?;
                    let base_url = derive_base_url(&target.full_url, &target.hostname);
                    let message = ClassificationJobMessage {
                        normalized_key: normalized_key.clone(),
                        entity_level: target.entity_level.clone(),
                        hostname: target.hostname.clone(),
                        full_url: target.full_url.clone(),
                        trace_id: trace_id.clone(),
                        idempotency_key: Some(format!("cls:{}:{}", normalized_key, trace_id)),
                        requires_content: true,
                        base_url: Some(base_url.clone()),
                        content_excerpt: page_content
                            .as_ref()
                            .and_then(|content| content.content_excerpt.clone()),
                        content_hash: page_content
                            .as_ref()
                            .and_then(|content| content.content_hash.clone()),
                        content_version: page_content
                            .as_ref()
                            .and_then(|content| content.content_version),
                        content_language: page_content
                            .as_ref()
                            .and_then(|content| content.content_language.clone()),
                    };

                    match self.publisher.publish(&message).await {
                        Ok(_) => {
                            sqlx::query("UPDATE reclassification_jobs SET status = 'completed', completed_at = NOW() WHERE id = $1")
                                .bind(job_id)
                                .execute(&self.pool)
                                .await?;
                            metrics::record_job_dispatched();
                            if let Some(fetcher) = &self.page_fetch_publisher {
                                let fetch_job = PageFetchJob {
                                    normalized_key: normalized_key.clone(),
                                    url: base_url.clone(),
                                    hostname: target.hostname.clone(),
                                    candidate_urls: vec![base_url.clone()],
                                    trace_id: Some(trace_id.clone()),
                                    idempotency_key: Some(format!(
                                        "page:{}:{}",
                                        normalized_key, trace_id
                                    )),
                                    ttl_seconds: None,
                                };
                                match fetcher.publish(fetch_job).await {
                                    Ok(_) => metrics::record_page_fetch_dispatched(),
                                    Err(err) => {
                                        metrics::record_page_fetch_failure();
                                        warn!(
                                            target = "svc-reclass",
                                            %err,
                                            key = %normalized_key,
                                            "failed to enqueue page fetch job"
                                        );
                                    }
                                }
                            }
                            info!(
                                target = "svc-reclass",
                                key = %normalized_key,
                                reason = %reason,
                                "reclassification job dispatched"
                            );
                            processed += 1;
                        }
                        Err(err) => {
                            warn!(
                                target = "svc-reclass",
                                %err,
                                key = %normalized_key,
                                "failed to publish job, will retry"
                            );
                            sqlx::query(
                                "UPDATE reclassification_jobs SET status = 'pending', started_at = NULL WHERE id = $1",
                            )
                            .bind(job_id)
                            .execute(&self.pool)
                            .await?;
                            metrics::record_job_failure();
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        target = "svc-reclass",
                        %err,
                        key = %normalized_key,
                        "invalid normalized key, marking failed"
                    );
                    sqlx::query(
                        "UPDATE reclassification_jobs SET status = 'failed', completed_at = NOW() WHERE id = $1",
                    )
                    .bind(job_id)
                    .execute(&self.pool)
                    .await?;
                    metrics::record_job_failure();
                }
            }
        }

        metrics::set_reclass_backlog(self.fetch_backlog().await?);
        Ok(processed)
    }

    async fn load_page_content(&self, normalized_key: &str) -> Result<Option<PageContentSnippet>> {
        let row = sqlx::query(
            r#"
            SELECT text_excerpt, content_hash, fetch_version
            FROM page_contents
            WHERE normalized_key = $1
              AND expires_at > NOW()
            ORDER BY fetch_version DESC
            LIMIT 1
            "#,
        )
        .bind(normalized_key)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let content_excerpt: Option<String> = row.try_get("text_excerpt")?;
            let content_hash: Option<String> = row.try_get("content_hash")?;
            let fetch_version: i32 = row.try_get("fetch_version")?;
            Ok(Some(PageContentSnippet {
                content_excerpt,
                content_hash,
                content_version: Some(i64::from(fetch_version)),
                content_language: None,
            }))
        } else {
            Ok(None)
        }
    }

    async fn fetch_backlog(&self) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM reclassification_jobs WHERE status = 'pending'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get::<i64, _>("count")?)
    }
}

struct JobTarget {
    entity_level: String,
    hostname: String,
    full_url: String,
}

fn build_target(normalized_key: &str) -> Result<JobTarget> {
    let (prefix, remainder) = normalized_key
        .split_once(':')
        .ok_or_else(|| anyhow!("normalized_key missing prefix: {normalized_key}"))?;

    match prefix {
        "domain" | "subdomain" => {
            let host = remainder.trim().trim_matches('/');
            if host.is_empty() {
                return Err(anyhow!("empty host for normalized key {normalized_key}"));
            }
            Ok(JobTarget {
                entity_level: prefix.to_string(),
                hostname: host.to_string(),
                full_url: format!("https://{host}/"),
            })
        }
        "url" | "page" => {
            let parsed = Url::parse(remainder)
                .with_context(|| format!("invalid URL in normalized key {normalized_key}"))?;
            let host = parsed
                .host_str()
                .ok_or_else(|| anyhow!("url missing host for key {normalized_key}"))?
                .to_string();
            Ok(JobTarget {
                entity_level: prefix.to_string(),
                hostname: host,
                full_url: parsed.into(),
            })
        }
        _ => Err(anyhow!(
            "unsupported entity level {prefix} for key {normalized_key}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portpicker::pick_unused_port;
    use serde_json::json;
    use std::env;
    use std::process::{Command, Stdio};
    use tokio::time::{sleep, Duration, Instant};

    #[tokio::test]
    async fn planner_schedules_due_classifications() -> Result<()> {
        let (pg_guard, pg_port) = start_postgres_container()?;
        let database_url = format!(
            "postgres://postgres:postgres@{}:{}/postgres",
            test_host(),
            pg_port
        );
        let pool = connect_postgres(&database_url).await?;
        apply_migrations(&pool).await;

        insert_classification(
            &pool,
            "domain:planner.test",
            "planner.example.com",
            "NOW() - INTERVAL '1 minute'",
        )
        .await?;

        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let planner = Planner::new(pool.clone(), 10, Duration::from_secs(1), taxonomy);
        let planned = planner.plan_batch().await?;
        assert_eq!(planned, 1);

        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM reclassification_jobs WHERE normalized_key = $1",
        )
        .bind("domain:planner.test")
        .fetch_one(&pool)
        .await?;
        assert_eq!(count, 1);

        drop(pg_guard);
        Ok(())
    }

    #[tokio::test]
    async fn dispatcher_publishes_jobs_to_stream() -> Result<()> {
        let (pg_guard, pg_port) = start_postgres_container()?;
        let database_url = format!(
            "postgres://postgres:postgres@{}:{}/postgres",
            test_host(),
            pg_port
        );
        let pool = connect_postgres(&database_url).await?;
        apply_migrations(&pool).await;

        insert_classification(
            &pool,
            "domain:dispatcher.test",
            "dispatcher.example.com",
            "NOW() + INTERVAL '1 hour'",
        )
        .await?;

        let job_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO reclassification_jobs (id, normalized_key, reason, status, created_at) VALUES ($1, $2, $3, 'pending', NOW())",
        )
        .bind(job_id)
        .bind("domain:dispatcher.test")
        .bind("unit-test")
        .execute(&pool)
        .await?;

        let (redis_guard, redis_port) = start_redis_container()?;
        let redis_url = format!("redis://{}:{}/", test_host(), redis_port);
        wait_for_redis(&redis_url).await?;

        let publisher = JobPublisher::new(&redis_url, "classification-jobs")?;
        let dispatcher = Dispatcher::new(pool.clone(), publisher, None, 10);
        let processed = dispatcher.dispatch_batch().await?;
        assert_eq!(processed, 1);

        let status: String =
            sqlx::query_scalar("SELECT status FROM reclassification_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(status, "completed");

        let mut conn = redis::Client::open(redis_url.clone())?
            .get_async_connection()
            .await?;
        let stream_len: i64 = redis::cmd("XLEN")
            .arg("classification-jobs")
            .query_async(&mut conn)
            .await?;
        assert_eq!(stream_len, 1);

        drop(redis_guard);
        drop(pg_guard);
        Ok(())
    }

    #[tokio::test]
    async fn planner_canonicalizes_legacy_labels() -> Result<()> {
        let (pg_guard, pg_port) = start_postgres_container()?;
        let database_url = format!(
            "postgres://postgres:postgres@{}:{}/postgres",
            test_host(),
            pg_port
        );
        let pool = connect_postgres(&database_url).await?;
        apply_migrations(&pool).await;

        let classification_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO classifications (
                id, normalized_key, taxonomy_version, model_version,
                primary_category, subcategory, risk_level, recommended_action,
                confidence, sfw, flags, ttl_seconds, status, next_refresh_at
            ) VALUES (
                $1, $2, 'legacy', 'test', 'Social', 'Short form video',
                'low', 'Allow', 0.5, true, '{}'::jsonb, 3600, 'active', NOW()
            )
            "#,
        )
        .bind(classification_id)
        .bind("domain:legacy.test")
        .execute(&pool)
        .await?;

        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let planner = Planner::new(
            pool.clone(),
            1,
            Duration::from_secs(1),
            Arc::clone(&taxonomy),
        );
        let mut tx = pool.begin().await?;
        planner
            .ensure_canonical_labels(
                &mut tx,
                classification_id,
                "Social".into(),
                "Short form video".into(),
            )
            .await?;
        tx.commit().await?;

        let row =
            sqlx::query("SELECT primary_category, subcategory FROM classifications WHERE id = $1")
                .bind(classification_id)
                .fetch_one(&pool)
                .await?;
        let category: String = row.try_get("primary_category")?;
        let subcategory: String = row.try_get("subcategory")?;
        assert_eq!(category, "social-media");
        assert_eq!(subcategory, "short-video-platforms");

        drop(pg_guard);
        Ok(())
    }

    async fn insert_classification(
        pool: &PgPool,
        normalized_key: &str,
        hostname: &str,
        next_refresh_expr: &str,
    ) -> Result<()> {
        sqlx::query(&format!(
            "INSERT INTO classifications (id, normalized_key, taxonomy_version, model_version, primary_category, subcategory, risk_level, recommended_action, confidence, sfw, flags, ttl_seconds, status, next_refresh_at) VALUES ($1, $2, 'v1', 'test', 'News', 'General', 'low', 'Allow', 0.8, true, $3, 3600, 'active', {})",
            next_refresh_expr
        ))
        .bind(Uuid::new_v4())
        .bind(normalized_key)
        .bind(json!({"host": hostname}))
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn connect_postgres(database_url: &str) -> Result<PgPool> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match PgPoolOptions::new()
                .max_connections(5)
                .connect(database_url)
                .await
            {
                Ok(pool) => return Ok(pool),
                Err(_err) if Instant::now() < deadline => {
                    sleep(Duration::from_millis(250)).await;
                }
                Err(err) => return Err(err.into()),
            }
        }
    }

    async fn wait_for_redis(redis_url: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match redis::Client::open(redis_url) {
                Ok(client) => match client.get_async_connection().await {
                    Ok(mut conn) => {
                        if redis::cmd("PING")
                            .query_async::<_, String>(&mut conn)
                            .await
                            .is_ok()
                        {
                            return Ok(());
                        }
                    }
                    Err(_) => {}
                },
                Err(_) => {}
            }

            if Instant::now() > deadline {
                anyhow::bail!("redis did not become ready");
            }
            sleep(Duration::from_millis(200)).await;
        }
    }

    async fn apply_migrations(pool: &PgPool) {
        let migrations = [
            include_str!("../../../services/admin-api/migrations/0003_classifications.sql"),
            include_str!("../../../services/admin-api/migrations/0004_spec20_artifacts.sql"),
            include_str!("../../../services/admin-api/migrations/0005_page_contents.sql"),
            include_str!("../../../services/admin-api/migrations/0006_classification_requests.sql"),
        ];

        for ddl in migrations {
            match apply_sql_batch(pool, ddl).await {
                Ok(_) => continue,
                Err(err)
                    if ddl.contains("page_contents")
                        && err
                            .to_string()
                            .contains("generation expression is not immutable") =>
                {
                    apply_page_contents_fallback(pool).await;
                }
                Err(err) => panic!("apply migration: {err}"),
            }
        }
    }

    async fn apply_sql_batch(pool: &PgPool, sql: &str) -> Result<()> {
        sqlx::raw_sql(sql).execute(pool).await?;
        Ok(())
    }

    async fn apply_page_contents_fallback(pool: &PgPool) {
        for statement in PAGE_CONTENTS_TEST_DDL {
            sqlx::query(statement)
                .execute(pool)
                .await
                .expect("apply fallback migration statement");
        }
    }

    const PAGE_CONTENTS_TEST_DDL: &[&str] = &[
        r#"
CREATE TABLE IF NOT EXISTS page_contents (
    id UUID PRIMARY KEY,
    normalized_key TEXT NOT NULL,
    fetch_version INTEGER NOT NULL DEFAULT 1,
    content_type TEXT,
    content_hash TEXT,
    raw_bytes BYTEA,
    text_excerpt TEXT,
    char_count INTEGER,
    byte_count INTEGER,
    fetch_status TEXT NOT NULL,
    fetch_reason TEXT,
    ttl_seconds INTEGER NOT NULL DEFAULT 21600,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);
"#,
        r#"
CREATE OR REPLACE FUNCTION page_contents_set_expiry()
RETURNS TRIGGER AS $$
BEGIN
    NEW.expires_at := COALESCE(
        NEW.fetched_at,
        NOW()
    ) + (NEW.ttl_seconds * INTERVAL '1 second');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
"#,
        "DROP TRIGGER IF EXISTS trg_page_contents_set_expiry ON page_contents;",
        r#"
CREATE TRIGGER trg_page_contents_set_expiry
BEFORE INSERT ON page_contents
FOR EACH ROW EXECUTE FUNCTION page_contents_set_expiry();
"#,
        r#"
CREATE UNIQUE INDEX IF NOT EXISTS page_contents_norm_key_version_idx
    ON page_contents (normalized_key, fetch_version DESC);
"#,
        r#"
CREATE INDEX IF NOT EXISTS page_contents_expires_idx
    ON page_contents (expires_at);
"#,
    ];

    struct DockerContainer {
        id: String,
    }

    impl DockerContainer {
        fn run(image: &str, args: Vec<String>) -> Result<Self> {
            let mut cmd = Command::new("docker");
            cmd.arg("run").arg("-d").arg("--rm");
            for arg in args {
                cmd.arg(arg);
            }
            cmd.arg(image);
            let output = cmd
                .output()
                .with_context(|| format!("failed to launch docker image {image}"))?;
            if !output.status.success() {
                anyhow::bail!(
                    "docker run failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let id = String::from_utf8(output.stdout)
                .context("failed to read docker container id")?
                .trim()
                .to_string();
            Ok(Self { id })
        }
    }

    impl Drop for DockerContainer {
        fn drop(&mut self) {
            let _ = Command::new("docker")
                .arg("rm")
                .arg("-f")
                .arg(&self.id)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    fn start_postgres_container() -> Result<(DockerContainer, u16)> {
        let port = pick_unused_port().context("no free port for postgres")?;
        let container = DockerContainer::run(
            "postgres:16-alpine",
            vec![
                "-p".into(),
                format!("{}:5432", port),
                "-e".into(),
                "POSTGRES_PASSWORD=postgres".into(),
                "-e".into(),
                "POSTGRES_USER=postgres".into(),
            ],
        )?;
        Ok((container, port))
    }

    fn start_redis_container() -> Result<(DockerContainer, u16)> {
        let port = pick_unused_port().context("no free port for redis")?;
        let container = DockerContainer::run(
            "redis:7-alpine",
            vec!["-p".into(), format!("{}:6379", port)],
        )?;
        Ok((container, port))
    }

    fn base_reclass_cfg() -> ReclassConfig {
        ReclassConfig {
            redis_url: "redis://:redis-prod-secret-1234@redis:6379".into(),
            job_stream: "classification-jobs".into(),
            database_url:
                "postgres://svc_reclass:prod-db-password-123@postgres:5432/defender_admin".into(),
            planner_interval_seconds: 60,
            planner_batch_size: 200,
            dispatcher_batch_size: 200,
            metrics_host: "0.0.0.0".into(),
            metrics_port: 19016,
            db_pool_size: 5,
            page_fetch_queue: Some(PageFetchQueueConfig {
                redis_url: "redis://:redis-prod-secret-1234@redis:6379".into(),
                stream: "page-fetch-jobs".into(),
                ttl_seconds: 21_600,
            }),
        }
    }

    #[test]
    fn config_rejects_default_credentials_when_dev_mode_disabled() {
        std::env::remove_var("OD_ALLOW_INSECURE_DEV_SECRETS");
        let mut cfg = base_reclass_cfg();
        cfg.redis_url = "redis://redis:6379".into();
        let err = validate_config(&cfg).expect_err("missing redis auth should fail");
        assert!(format!("{err:#}").contains("reclass.redis_url"));
    }

    #[test]
    fn config_accepts_strong_credentials_when_dev_mode_disabled() {
        std::env::remove_var("OD_ALLOW_INSECURE_DEV_SECRETS");
        let cfg = base_reclass_cfg();
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn config_allows_defaults_only_with_explicit_dev_mode() {
        std::env::set_var("OD_ALLOW_INSECURE_DEV_SECRETS", "true");
        let mut cfg = base_reclass_cfg();
        cfg.redis_url = "redis://redis:6379".into();
        cfg.database_url = "postgres://defender:defender@postgres:5432/defender_admin".into();
        assert!(validate_config(&cfg).is_ok());
        std::env::remove_var("OD_ALLOW_INSECURE_DEV_SECRETS");
    }

    fn test_host() -> String {
        env::var("TEST_DOCKER_HOST").unwrap_or_else(|_| "127.0.0.1".into())
    }
}
