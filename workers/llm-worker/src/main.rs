use anyhow::Result;
use redis::streams::{StreamReadOptions, StreamReadReply};
use redis::AsyncCommands;
use serde::Deserialize;
use tokio::signal;
use tokio_stream::StreamExt;
use tracing::{error, info, Level};

#[derive(Debug, Deserialize)]
struct WorkerConfig {
    pub queue_name: String,
    pub redis_url: String,
    pub cache_channel: String,
    #[serde(default = "default_stream")]
    pub stream: String,
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

    let cache_listener = CacheListener::new(&cfg.redis_url, &cfg.cache_channel).await?;
    tokio::spawn(cache_listener.run());

    let job_consumer = JobConsumer::new(&cfg.redis_url, &cfg.stream).await?;
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
}

impl JobConsumer {
    async fn new(redis_url: &str, stream: &str) -> Result<Self> {
        Ok(Self {
            redis_url: redis_url.to_string(),
            stream: stream.to_string(),
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
                        info!(
                            target = "svc-llm-worker",
                            id = %entry.id,
                            payload,
                            "processing classification job"
                        );
                    }
                }
            }
        }
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
