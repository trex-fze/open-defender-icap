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
            let ttl_seconds: i32 = row.try_get::<i32, _>("ttl_seconds")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use portpicker::pick_unused_port;
    use serde_json::json;
    use std::process::{Command, Stdio};
    use tokio::time::{sleep, Duration, Instant};

    #[tokio::test]
    async fn planner_schedules_due_classifications() -> Result<()> {
        let (pg_guard, pg_port) = start_postgres_container()?;
        let database_url = format!(
            "postgres://postgres:postgres@127.0.0.1:{}/postgres",
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

        let planner = Planner::new(pool.clone(), 10, Duration::from_secs(1));
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
            "postgres://postgres:postgres@127.0.0.1:{}/postgres",
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
        let redis_url = format!("redis://127.0.0.1:{}/", redis_port);
        wait_for_redis(&redis_url).await?;

        let publisher = JobPublisher::new(&redis_url, "classification-jobs")?;
        let dispatcher = Dispatcher::new(pool.clone(), publisher, 10);
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
        for ddl in [
            include_str!("../../../services/admin-api/migrations/0003_classifications.sql"),
            include_str!("../../../services/admin-api/migrations/0004_spec20_artifacts.sql"),
        ] {
            apply_sql_batch(pool, ddl).await.expect("apply migration");
        }
    }

    async fn apply_sql_batch(pool: &PgPool, sql: &str) -> Result<()> {
        for statement in sql.split(';') {
            let trimmed = statement.trim();
            if trimmed.is_empty() {
                continue;
            }
            sqlx::query(trimmed).execute(pool).await?;
        }
        Ok(())
    }

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
            "postgres:15-alpine",
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
}
