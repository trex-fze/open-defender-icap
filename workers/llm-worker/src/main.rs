use anyhow::Result;
use serde::Deserialize;
use tracing::{info, Level};

#[derive(Debug, Deserialize)]
struct WorkerConfig {
    pub queue_name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: WorkerConfig = config_core::load_config("config/llm-worker.json")?;
    info!(target = "svc-llm-worker", queue = %cfg.queue_name, "LLM worker placeholder running");

    Ok(())
}
