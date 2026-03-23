mod metrics;

use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: ReclassConfig = config_core::load_config("config/reclass-worker.json")?;
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

    let publisher = JobPublisher::new(&cfg.redis_url, &cfg.job_stream)?;

    let planner = Planner::new(
        pool.clone(),
        cfg.planner_batch_size,
        Duration::from_secs(cfg.planner_interval_seconds.max(5)),
    );
    let dispatcher = Dispatcher::new(pool.clone(), publisher.clone(), cfg.dispatcher_batch_size);

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

#[derive(Debug, Serialize)]
struct ClassificationJobMessage {
    normalized_key: String,
    entity_level: String,
    hostname: String,
    full_url: String,
    trace_id: String,
}

struct Planner {
    pool: PgPool,
    batch_size: i64,
    interval: Duration,
}

impl Planner {
    fn new(pool: PgPool, batch_size: i64, interval: Duration) -> Self {
        Self {
            pool,
            batch_size,
            interval,
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
            SELECT id, normalized_key, COALESCE(ttl_seconds, 3600) as ttl_seconds
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
            let ttl_seconds: i64 = row.try_get::<i64, _>("ttl_seconds")?;
            let ttl = ttl_seconds.max(300);
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
    batch_size: i64,
}

impl Dispatcher {
    fn new(pool: PgPool, publisher: JobPublisher, batch_size: i64) -> Self {
        Self {
            pool,
            publisher,
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
                    let message = ClassificationJobMessage {
                        normalized_key: normalized_key.clone(),
                        entity_level: target.entity_level,
                        hostname: target.hostname,
                        full_url: target.full_url,
                        trace_id: trace_id.clone(),
                    };

                    match self.publisher.publish(&message).await {
                        Ok(_) => {
                            sqlx::query("UPDATE reclassification_jobs SET status = 'completed', completed_at = NOW() WHERE id = $1")
                                .bind(job_id)
                                .execute(&self.pool)
                                .await?;
                            metrics::record_job_dispatched();
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
