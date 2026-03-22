use anyhow::Result;
use axum::{routing::get, Router};
use serde::Deserialize;
use tokio::net::TcpListener;
use tracing::{info, Level};

#[derive(Debug, Deserialize)]
struct AdminApiConfig {
    pub host: String,
    pub port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: AdminApiConfig = config_core::load_config("config/admin-api.json")?;

    let app = Router::new()
        .route("/health/ready", get(health))
        .route("/health/live", get(health));

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(target = "svc-admin", %addr, "admin api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "OK"
}
