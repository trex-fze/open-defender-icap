use anyhow::{Context, Result};
use common_types::PolicyAction;
use redis::streams::{StreamReadOptions, StreamReadReply};
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use tokio::signal;
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
}

fn default_stream() -> String {
    "classification-jobs".into()
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

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&cfg.database_url)
        .await?;

    let cache_listener = CacheListener::new(&cfg.redis_url, &cfg.cache_channel).await?;
    tokio::spawn(cache_listener.run());

    let job_consumer = JobConsumer::new(&cfg.redis_url, &cfg.stream, pool).await?;
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
}

impl JobConsumer {
    async fn new(redis_url: &str, stream: &str, pool: PgPool) -> Result<Self> {
        Ok(Self {
            redis_url: redis_url.to_string(),
            stream: stream.to_string(),
            pool,
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
        let job: ClassificationJobPayload = serde_json::from_str(payload)?;
        let verdict = simulate_classification(&job);
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

#[derive(Debug, Deserialize)]
struct ClassificationJobPayload {
    normalized_key: String,
    entity_level: String,
    hostname: String,
    full_url: String,
    trace_id: String,
}

#[derive(Clone)]
struct SimulatedVerdict {
    primary_category: String,
    subcategory: String,
    risk_level: String,
    confidence: f32,
    recommended_action: common_types::PolicyAction,
}

fn simulate_classification(job: &ClassificationJobPayload) -> SimulatedVerdict {
    if job.hostname.contains("social") {
        SimulatedVerdict {
            primary_category: "Social Media".into(),
            subcategory: "General".into(),
            risk_level: "medium".into(),
            confidence: 0.7,
            recommended_action: common_types::PolicyAction::Review,
        }
    } else if job.hostname.contains("malware") {
        SimulatedVerdict {
            primary_category: "Malware".into(),
            subcategory: "CommandAndControl".into(),
            risk_level: "high".into(),
            confidence: 0.95,
            recommended_action: common_types::PolicyAction::Block,
        }
    } else {
        SimulatedVerdict {
            primary_category: "Productivity".into(),
            subcategory: "General".into(),
            risk_level: "low".into(),
            confidence: 0.85,
            recommended_action: common_types::PolicyAction::Allow,
        }
    }
}

async fn store_classification(
    pool: &PgPool,
    job: &ClassificationJobPayload,
    verdict: &SimulatedVerdict,
) -> Result<()> {
    use common_types::PolicyAction;
    let action = verdict.recommended_action.to_string();
    let new_id = Uuid::new_v4();
    let ttl_seconds = 3600;
    let sfw = matches!(verdict.recommended_action, PolicyAction::Allow);
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
    .bind(&action)
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
