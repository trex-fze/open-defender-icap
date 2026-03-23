mod bootstrap;
mod config;
mod elastic;
mod models;

use axum::{
    extract::State, http::HeaderMap, http::StatusCode, routing::get, routing::post, Json, Router,
};
use config::IngestConfig;
use elastic::ElasticWriter;
use models::{FilebeatEnvelope, HealthResponse};
use serde_json::Value;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    writer: ElasticWriter,
    shared_secret: Option<String>,
    index_prefix: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = IngestConfig::from_env()?;
    init_tracing(&config.log_level);
    let writer = ElasticWriter::new(
        &config.elastic_url,
        config.elastic_api_key.clone(),
        config.elastic_username.clone(),
        config.elastic_password.clone(),
        config.ingest_retry_attempts,
    )?;
    if config.apply_templates {
        bootstrap::ensure_assets(&writer, &config).await?;
    }

    let state = AppState {
        writer,
        shared_secret: config.filebeat_secret.clone(),
        index_prefix: config.elastic_index_prefix.clone(),
    };

    let app = Router::new()
        .route("/health/ready", get(ready))
        .route("/ingest/filebeat", post(filebeat_ingest))
        .with_state(state);

    let addr: SocketAddr = config.bind_addr.parse()?;
    info!(target = "svc-ingest", %addr, "starting event-ingester");
    let listener = TcpListener::bind(addr).await?;
    let server = axum::serve(listener, app.into_make_service());
    tokio::select! {
        result = server => {
            if let Err(err) = result {
                error!(target = "svc-ingest", %err, "server error");
            }
        }
        _ = shutdown_signal() => {
            info!(target = "svc-ingest", "shutdown signal received");
        }
    }
    Ok(())
}

fn init_tracing(level: &str) {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| level.to_string());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .json()
        .init();
}

async fn shutdown_signal() {
    let _ = signal::ctrl_c().await;
}

async fn ready() -> Json<HealthResponse> {
    Json(HealthResponse::ok())
}

async fn filebeat_ingest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(envelope): Json<FilebeatEnvelope>,
) -> Result<StatusCode, (StatusCode, String)> {
    if let Some(secret) = &state.shared_secret {
        let provided = headers
            .get("x-filebeat-secret")
            .and_then(|value| value.to_str().ok());
        if provided != Some(secret.as_str()) {
            return Err((StatusCode::UNAUTHORIZED, "invalid filebeat secret".into()));
        }
    }

    let docs: Vec<Value> = envelope.into_events();
    if docs.is_empty() {
        return Ok(StatusCode::ACCEPTED);
    }

    let index_prefix = state.index_prefix.clone();
    state
        .writer
        .bulk_index(index_prefix, docs)
        .await
        .map_err(|err| {
            error!(target = "svc-ingest", %err, "failed to index events");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to index events".into(),
            )
        })?;

    Ok(StatusCode::ACCEPTED)
}
