use anyhow::Result;
use serde::Deserialize;
use tracing::{info, Level};

#[derive(Debug, Deserialize)]
struct ReclassConfig {
    pub schedule: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: ReclassConfig = config_core::load_config("config/reclass-worker.json")?;
    info!(target = "svc-reclass", schedule = %cfg.schedule, "reclassification worker placeholder running");

    Ok(())
}
