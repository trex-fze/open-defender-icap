mod cache;
mod config;
mod icap;
mod normalizer;
mod policy_client;

use anyhow::Result;
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, instrument, Level};

use cache::CacheClient;
use common_types::{PolicyAction, PolicyDecisionRequest};
use policy_client::PolicyClient;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg = config::load()?;
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(target = "svc-icap", %addr, preview_size = cfg.preview_size, "ICAP adaptor listening");

    let cache = Arc::new(CacheClient::new(cfg.redis_url.clone())?);
    let policy_client = Arc::new(PolicyClient::new(cfg.policy_endpoint.clone())?);

    loop {
        let (socket, peer) = listener.accept().await?;
        let cfg = cfg.clone();
        let cache = Arc::clone(&cache);
        let policy_client = Arc::clone(&policy_client);
        info!(target = "svc-icap", ?peer, "accepted connection");
        tokio::spawn(async move {
            if let Err(err) = handle_connection(socket, cfg, cache, policy_client).await {
                error!(target = "svc-icap", error = %err, "connection handler failed");
            }
        });
    }
}

#[instrument(name = "icap_connection", skip_all)]
async fn handle_connection(
    mut socket: TcpStream,
    cfg: config::IcapConfig,
    cache: Arc<CacheClient>,
    policy_client: Arc<PolicyClient>,
) -> Result<()> {
    let mut buf = vec![0u8; cfg.preview_size.max(1024)];
    let n = socket.read(&mut buf).await?;
    let raw = String::from_utf8_lossy(&buf[..n]);
    let icap_req = icap::IcapRequest::parse(&raw)?;
    let normalized = normalizer::normalize_target(
        icap_req.http_host.as_str(),
        &icap_req.http_path,
        icap_req.http_scheme.as_deref(),
    )?;

    let decision = if let Some(decision) = cache.get(&normalized.normalized_key).await? {
        info!(target = "svc-icap", normalized_key = %normalized.normalized_key, action = ?decision.action, "cache decision");
        decision
    } else {
        let request = PolicyDecisionRequest {
            normalized_key: normalized.normalized_key.clone(),
            entity_level: normalized.entity_level.clone(),
            source_ip: icap_req.http_host.clone(),
            user_id: None,
        };
        let decision = policy_client.evaluate(&request).await?;
        info!(target = "svc-icap", normalized_key = %normalized.normalized_key, action = ?decision.action, "policy decision placeholder");
        cache
            .set(
                normalized.normalized_key.clone(),
                decision.clone(),
                Duration::from_secs(300),
            )
            .await?;
        decision
    };

    let response = icap_response(&decision.action);
    socket.write_all(response.as_bytes()).await?;
    socket.shutdown().await?;
    Ok(())
}

fn icap_response(action: &PolicyAction) -> String {
    use PolicyAction::*;
    match action {
        Allow | Monitor => "ICAP/1.0 204 No Content\r\n\r\n".to_string(),
        _ => {
            let body = "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nContent-Length: 18\r\n\r\nRequest blocked.";
            format!(
                "ICAP/1.0 200 OK\r\nEncapsulated: res-body=0\r\n\r\n{}",
                body
            )
        }
    }
}
