mod metrics;
mod schema;

use anyhow::{Context, Result};
use redis::streams::{StreamReadOptions, StreamReadReply};
use redis::AsyncCommands;
use schema::{LlmResponse, PromptPayload};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use tokio::{signal, time::Instant};
use tokio_stream::StreamExt;
use tracing::{error, info, Level};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct WorkerConfig {
    pub queue_name: String,
    pub redis_url: String,
    pub cache_channel: String,
    #[serde(default = "default_stream")]
    pub stream: String,
    pub database_url: String,
    pub llm_endpoint: String,
    pub llm_api_key: String,
    #[serde(default = "default_metrics_host")]
    pub metrics_host: String,
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
}

fn default_stream() -> String {
    "classification-jobs".into()
}

fn default_metrics_host() -> String {
    "0.0.0.0".into()
}

fn default_metrics_port() -> u16 {
    19015
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: WorkerConfig = config_core::load_config("config/llm-worker.json")?;
    info!(
        target = "svc-llm-worker",
        queue = %cfg.queue_name,
        channel = %cfg.cache_channel,
        stream = %cfg.stream,
        "LLM worker initialized"
    );

    let metrics_host = cfg.metrics_host.clone();
    let metrics_port = cfg.metrics_port;
    tokio::spawn(async move {
        if let Err(err) = metrics::serve_metrics(&metrics_host, metrics_port).await {
            error!(target = "svc-llm-worker", %err, "metrics server exited");
        }
    });

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&cfg.database_url)
        .await?;

    let cache_listener = CacheListener::new(&cfg.redis_url, &cfg.cache_channel).await?;
    tokio::spawn(cache_listener.run());

    let job_consumer = JobConsumer::new(&cfg, pool).await?;
    tokio::spawn(job_consumer.run());

    signal::ctrl_c().await?;
    Ok(())
}

struct CacheListener {
    redis_url: String,
    channel: String,
}

struct JobConsumer {
    redis_url: String,
    stream: String,
    pool: PgPool,
    llm_endpoint: String,
    llm_api_key: String,
}

impl JobConsumer {
    async fn new(cfg: &WorkerConfig, pool: PgPool) -> Result<Self> {
        Ok(Self {
            redis_url: cfg.redis_url.clone(),
            stream: cfg.stream.clone(),
            pool,
            llm_endpoint: cfg.llm_endpoint.clone(),
            llm_api_key: cfg.llm_api_key.clone(),
        })
    }

    async fn run(self) {
        loop {
            if let Err(err) = self.consume().await {
                error!(target = "svc-llm-worker", %err, "job consumer error");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }

    async fn consume(&self) -> Result<(), redis::RedisError> {
        let client = redis::Client::open(self.redis_url.clone())?;
        let mut conn = client.get_async_connection().await?;
        let options = StreamReadOptions::default().block(5000).count(10);
        loop {
            let reply: StreamReadReply = conn
                .xread_options(&[&self.stream], &["$"], &options)
                .await?;
            for stream in reply.keys {
                for entry in stream.ids {
                    if let Some(payload) = entry.get::<String>("payload") {
                        if let Err(err) = self.process_job(&payload).await {
                            error!(target = "svc-llm-worker", %err, "failed to process job");
                        }
                    }
                }
            }
        }
    }

    async fn process_job(&self, payload: &str) -> Result<(), anyhow::Error> {
        metrics::record_job_started();
        let result = self.handle_job(payload).await;
        match result {
            Ok(_) => metrics::record_job_completed(),
            Err(_) => metrics::record_job_failed(),
        }
        result
    }

    async fn handle_job(&self, payload: &str) -> Result<(), anyhow::Error> {
        let job: ClassificationJobPayload = serde_json::from_str(payload)?;
        let verdict = invoke_llm(&self.llm_endpoint, &self.llm_api_key, &job).await?;
        store_classification(&self.pool, &job, &verdict)
            .await
            .context("failed to persist classification")?;
        info!(
            target = "svc-llm-worker",
            key = job.normalized_key,
            action = ?verdict.recommended_action,
            "classification stored"
        );
        Ok(())
    }
}

impl CacheListener {
    async fn new(redis_url: &str, channel: &str) -> Result<Self> {
        Ok(Self {
            redis_url: redis_url.to_string(),
            channel: channel.to_string(),
        })
    }

    async fn run(self) {
        loop {
            match redis::Client::open(self.redis_url.clone()) {
                Ok(client) => {
                    if let Err(err) = self.listen(client).await {
                        error!(target = "svc-llm-worker", %err, "cache listener error");
                    }
                }
                Err(err) => error!(target = "svc-llm-worker", %err, "failed to connect to redis"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    async fn listen(&self, client: redis::Client) -> Result<(), redis::RedisError> {
        let conn = client.get_async_connection().await?;
        let mut pubsub = conn.into_pubsub();
        pubsub.subscribe(&self.channel).await?;
        let mut stream = pubsub.on_message();
        while let Some(msg) = stream.next().await {
            let payload: String = msg.get_payload()?;
            info!(
                target = "svc-llm-worker",
                event = payload,
                "cache invalidation received"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ClassificationJobPayload {
    normalized_key: String,
    entity_level: String,
    hostname: String,
    full_url: String,
    trace_id: String,
}

async fn store_classification(
    pool: &PgPool,
    job: &ClassificationJobPayload,
    verdict: &schema::LlmResponse,
) -> Result<()> {
    use common_types::PolicyAction;
    let action = match verdict.recommended_action.as_str() {
        "Allow" => PolicyAction::Allow,
        "Block" => PolicyAction::Block,
        "Warn" => PolicyAction::Warn,
        "Monitor" => PolicyAction::Monitor,
        "Review" => PolicyAction::Review,
        "RequireApproval" => PolicyAction::RequireApproval,
        other => return Err(anyhow::anyhow!("invalid action {other}")),
    };
    let new_id = Uuid::new_v4();
    let ttl_seconds = 3600;
    let sfw = matches!(action, PolicyAction::Allow);
    let flags: Value = json!({"source": "llm-worker"});

    let row = sqlx::query(
        r#"INSERT INTO classifications
            (id, normalized_key, taxonomy_version, model_version, primary_category, subcategory,
             risk_level, recommended_action, confidence, sfw, flags, ttl_seconds, status, next_refresh_at)
            VALUES ($1, $2, 'v1', 'llm-sim', $3, $4, $5, $6, $7, $8, $9, $10, 'active', NOW() + INTERVAL '4 hours')
            ON CONFLICT (normalized_key)
            DO UPDATE SET
                primary_category = EXCLUDED.primary_category,
                subcategory = EXCLUDED.subcategory,
                risk_level = EXCLUDED.risk_level,
                recommended_action = EXCLUDED.recommended_action,
                confidence = EXCLUDED.confidence,
                sfw = EXCLUDED.sfw,
                flags = EXCLUDED.flags,
                ttl_seconds = EXCLUDED.ttl_seconds,
                updated_at = NOW()
            RETURNING id"#,
    )
    .bind(new_id)
    .bind(&job.normalized_key)
    .bind(&verdict.primary_category)
    .bind(&verdict.subcategory)
    .bind(&verdict.risk_level)
    .bind(action.to_string())
    .bind(verdict.confidence as f64)
    .bind(sfw)
    .bind(flags)
    .bind(ttl_seconds)
    .fetch_one(pool)
    .await?;

    let classification_id: Uuid = row.get("id");
    let current_version: i64 = sqlx::query(
        "SELECT COALESCE(MAX(version), 0) AS version FROM classification_versions WHERE classification_id = $1",
    )
    .bind(classification_id)
    .fetch_one(pool)
    .await?
    .get::<i64, _>("version");
    let next_version = current_version + 1;

    sqlx::query(
        "INSERT INTO classification_versions (id, classification_id, version, changed_by, reason, payload)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(classification_id)
    .bind(next_version)
    .bind(Some("llm-worker".to_string()))
    .bind(Some("automated".to_string()))
    .bind(json!({
        "normalized_key": job.normalized_key,
        "category": verdict.primary_category,
        "action": action,
    }))
    .execute(pool)
    .await?;

    Ok(())
}

async fn invoke_llm(
    endpoint: &str,
    api_key: &str,
    job: &ClassificationJobPayload,
) -> Result<LlmResponse> {
    let client = reqwest::Client::new();
    let payload = PromptPayload {
        normalized_key: &job.normalized_key,
        hostname: &job.hostname,
        full_url: &job.full_url,
        entity_level: &job.entity_level,
        trace_id: &job.trace_id,
    };

    metrics::record_llm_invocation();
    let start = Instant::now();
    let response = match client
        .post(endpoint)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            if err.is_timeout() {
                metrics::record_llm_timeout();
            }
            metrics::record_llm_failure();
            return Err(err.into());
        }
    };

    metrics::observe_llm_latency(start.elapsed().as_secs_f64());

    let response = response.error_for_status().map_err(|err| {
        metrics::record_llm_failure();
        err
    })?;

    let verdict = response.json::<LlmResponse>().await.map_err(|err| {
        metrics::record_invalid_response();
        err
    })?;

    let verdict = verdict.validate().map_err(|err| {
        metrics::record_invalid_response();
        err
    })?;
    Ok(verdict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Json, routing::post, Router};
    use portpicker::pick_unused_port;
    use serde_json::json;
    use std::{
        process::{Command, Stdio},
        sync::Arc,
    };
    use tokio::{
        net::TcpListener,
        task::JoinHandle,
        time::{sleep, timeout, Duration, Instant},
    };

    #[tokio::test]
    async fn processes_queue_job_and_persists_classification() -> Result<()> {
        let (redis_guard, redis_port) = start_redis_container()?;
        let redis_url = format!("redis://127.0.0.1:{}/", redis_port);
        wait_for_redis(&redis_url).await?;

        let (postgres_guard, pg_port) = start_postgres_container()?;
        let database_url = format!(
            "postgres://postgres:postgres@127.0.0.1:{}/postgres",
            pg_port
        );
        let pool = connect_postgres(&database_url).await?;
        apply_migrations(&pool).await;

        let (llm_endpoint, server_task) = spawn_mock_llm().await;

        let cfg = WorkerConfig {
            queue_name: "classification-jobs".into(),
            redis_url: redis_url.clone(),
            cache_channel: "od:cache:invalidate".into(),
            stream: "classification-jobs".into(),
            database_url: database_url.clone(),
            llm_endpoint,
            llm_api_key: "test-key".into(),
            metrics_host: "127.0.0.1".into(),
            metrics_port: 0,
        };

        let consumer = JobConsumer::new(&cfg, pool.clone()).await.unwrap();
        let consumer_handle = tokio::spawn(async move { consumer.run().await });

        let job = ClassificationJobPayload {
            normalized_key: "domain:integration.test".into(),
            entity_level: "domain".into(),
            hostname: "integration.test".into(),
            full_url: "https://integration.test/".into(),
            trace_id: "trace-123".into(),
        };

        sleep(Duration::from_millis(500)).await;
        enqueue_job(&redis_url, &job).await.expect("enqueue job");

        timeout(Duration::from_secs(30), async {
            loop {
                if classification_exists(&pool, &job.normalized_key)
                    .await
                    .expect("query classification")
                {
                    break;
                }
                sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .expect("classification persisted");

        consumer_handle.abort();
        server_task.abort();
        drop(redis_guard);
        drop(postgres_guard);
        Ok(())
    }

    #[tokio::test]
    async fn fails_on_invalid_llm_response() -> Result<()> {
        let (redis_guard, redis_port) = start_redis_container()?;
        let redis_url = format!("redis://127.0.0.1:{}/", redis_port);
        wait_for_redis(&redis_url).await?;

        let (postgres_guard, pg_port) = start_postgres_container()?;
        let database_url = format!(
            "postgres://postgres:postgres@127.0.0.1:{}/postgres",
            pg_port
        );
        let pool = connect_postgres(&database_url).await?;
        apply_migrations(&pool).await;

        let (llm_endpoint, server_task) = spawn_llm_with_payload(json!({
            "primary_category": "News / Media",
            "subcategory": "National",
            "risk_level": "low",
            "confidence": 0.88,
            "recommended_action": "DROP"
        }))
        .await;

        let cfg = WorkerConfig {
            queue_name: "classification-jobs".into(),
            redis_url: redis_url.clone(),
            cache_channel: "od:cache:invalidate".into(),
            stream: "classification-jobs".into(),
            database_url: database_url.clone(),
            llm_endpoint,
            llm_api_key: "test-key".into(),
            metrics_host: "127.0.0.1".into(),
            metrics_port: 0,
        };

        let consumer = JobConsumer::new(&cfg, pool.clone()).await.unwrap();
        let consumer_handle = tokio::spawn(async move { consumer.run().await });

        let job = ClassificationJobPayload {
            normalized_key: "domain:invalid-llm.test".into(),
            entity_level: "domain".into(),
            hostname: "invalid-llm.test".into(),
            full_url: "https://invalid-llm.test/".into(),
            trace_id: "trace-invalid".into(),
        };

        sleep(Duration::from_millis(500)).await;
        enqueue_job(&redis_url, &job).await.expect("enqueue job");

        sleep(Duration::from_secs(3)).await;
        let exists = classification_exists(&pool, &job.normalized_key)
            .await
            .expect("query classification");
        assert!(
            !exists,
            "invalid LLM response should not persist classification"
        );

        consumer_handle.abort();
        server_task.abort();
        drop(redis_guard);
        drop(postgres_guard);
        Ok(())
    }

    async fn spawn_mock_llm() -> (String, JoinHandle<()>) {
        spawn_llm_with_payload(json!({
            "primary_category": "News / Media",
            "subcategory": "National",
            "risk_level": "low",
            "confidence": 0.88,
            "recommended_action": "Allow"
        }))
        .await
    }

    async fn spawn_llm_with_payload(payload: serde_json::Value) -> (String, JoinHandle<()>) {
        let payload = Arc::new(payload);
        let route_payload = Arc::clone(&payload);
        let app = Router::new().route(
            "/classify",
            post(move |Json(_body): Json<serde_json::Value>| {
                let route_payload = Arc::clone(&route_payload);
                async move { Json((*route_payload).clone()) }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock llm");
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{}:{}/classify", addr.ip(), addr.port());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve mock llm");
        });
        (url, task)
    }

    async fn enqueue_job(
        redis_url: &str,
        job: &ClassificationJobPayload,
    ) -> Result<(), redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let mut conn = client.get_async_connection().await?;
        let payload = serde_json::to_string(job).expect("serialize job");
        let _: () = redis::cmd("XADD")
            .arg("classification-jobs")
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }

    async fn classification_exists(pool: &PgPool, key: &str) -> Result<bool> {
        let row = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM classifications WHERE normalized_key = $1",
        )
        .bind(key)
        .fetch_one(pool)
        .await?;
        Ok(row > 0)
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
                    continue;
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

    fn start_redis_container() -> Result<(DockerContainer, u16)> {
        let port = pick_unused_port().context("no free port for redis")?;
        let container = DockerContainer::run(
            "redis:7-alpine",
            vec!["-p".into(), format!("{}:6379", port)],
        )?;
        Ok((container, port))
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
}
