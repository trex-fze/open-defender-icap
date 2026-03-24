mod cache;
mod config;
mod icap;
mod jobs;
mod metrics;
mod normalizer;
mod policy_client;

use anyhow::Result;
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, instrument, Level};

use cache::CacheClient;
use common_types::{EntityLevel, PolicyAction, PolicyDecisionRequest};
use jobs::{ClassificationJob, JobPublisher, PageFetchJob, PageFetchPublisher};
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

    let cache = Arc::new(CacheClient::new(
        cfg.redis_url.clone(),
        cfg.cache_channel.clone(),
    )?);
    let policy_client = Arc::new(PolicyClient::new(cfg.policy_endpoint.clone())?);
    let job_publisher = cfg
        .job_queue
        .as_ref()
        .map(|queue| JobPublisher::new(&queue.redis_url, queue.stream.clone()))
        .transpose()?;

    let page_fetch_publisher = cfg
        .page_fetch_queue
        .as_ref()
        .map(|queue| PageFetchPublisher::new(&queue.redis_url, queue.stream.clone()))
        .transpose()?;

    let metrics_host = cfg.metrics_host.clone();
    let metrics_port = cfg.metrics_port;
    tokio::spawn(async move {
        if let Err(err) = metrics::serve_metrics(&metrics_host, metrics_port).await {
            error!(target = "svc-icap", error = %err, "metrics server exited");
        }
    });

    loop {
        let (socket, peer) = listener.accept().await?;
        let cfg = cfg.clone();
        let cache = Arc::clone(&cache);
        let policy_client = Arc::clone(&policy_client);
        let job_publisher = job_publisher.clone();
        let page_fetch_publisher = page_fetch_publisher.clone();
        info!(target = "svc-icap", ?peer, "accepted connection");
        tokio::spawn(async move {
            if let Err(err) = handle_connection(
                socket,
                cfg,
                cache,
                policy_client,
                job_publisher,
                page_fetch_publisher,
            )
            .await
            {
                error!(target = "svc-icap", error = %err, "connection handler failed");
            }
        });
    }
}

#[instrument(name = "icap_connection", skip_all, fields(trace_id = tracing::field::Empty))]
async fn handle_connection(
    mut socket: TcpStream,
    cfg: config::IcapConfig,
    cache: Arc<CacheClient>,
    policy_client: Arc<PolicyClient>,
    job_publisher: Option<JobPublisher>,
    page_fetch_publisher: Option<PageFetchPublisher>,
) -> Result<()> {
    let roundtrip_start = tokio::time::Instant::now();
    let mut buf = vec![0u8; cfg.preview_size.max(1024)];
    let n = socket.read(&mut buf).await?;
    let raw = String::from_utf8_lossy(&buf[..n]);
    let icap_req = icap::IcapRequest::parse(&raw)?;
    let normalized = normalizer::normalize_target(
        icap_req.http_host.as_str(),
        &icap_req.http_path,
        icap_req.http_scheme.as_deref(),
    )?;

    if let Some(trace_id) = &icap_req.trace_id {
        tracing::Span::current().record("trace_id", &tracing::field::display(trace_id));
    }

    let decision = if let Some(decision) = cache.get(&normalized.normalized_key).await? {
        metrics::record_cache_hit();
        info!(target = "svc-icap", normalized_key = %normalized.normalized_key, action = ?decision.action, "cache decision");
        decision
    } else {
        metrics::record_cache_miss();
        let request = PolicyDecisionRequest {
            normalized_key: normalized.normalized_key.clone(),
            entity_level: normalized.entity_level.clone(),
            source_ip: icap_req.http_host.clone(),
            user_id: None,
        };
        let start = tokio::time::Instant::now();
        let decision = policy_client.evaluate(&request).await.map_err(|err| {
            metrics::record_error();
            err
        })?;
        let latency = start.elapsed().as_secs_f64();
        metrics::observe_policy_latency(latency);
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

    if let Some(publisher) = &job_publisher {
        if should_enqueue(&decision.action, decision.verdict.is_none()) {
            if let Err(err) = publisher
                .publish(&ClassificationJob {
                    normalized_key: &normalized.normalized_key,
                    entity_level: entity_level_str(&normalized.entity_level),
                    hostname: &normalized.hostname,
                    full_url: &normalized.full_url,
                    trace_id: icap_req.trace_id.as_deref().unwrap_or(""),
                })
                .await
            {
                error!(target = "svc-icap", %err, "failed to publish classification job");
            }
        }
    }

    if let Some(publisher) = &page_fetch_publisher {
        if should_fetch_page(&normalized.full_url) {
            if let Err(err) = publisher
                .publish(&PageFetchJob {
                    normalized_key: &normalized.normalized_key,
                    url: &normalized.full_url,
                    hostname: &normalized.hostname,
                    trace_id: icap_req.trace_id.as_deref().unwrap_or(""),
                    ttl_seconds: 21_600,
                })
                .await
            {
                error!(target = "svc-icap", %err, "failed to publish page fetch job");
            }
        }
    }

    let response = icap_response(&decision.action);
    socket.write_all(response.as_bytes()).await?;
    socket.shutdown().await?;
    metrics::observe_squid_roundtrip(roundtrip_start.elapsed().as_secs_f64());
    Ok(())
}

fn should_enqueue(action: &PolicyAction, missing_verdict: bool) -> bool {
    missing_verdict
        || matches!(
            action,
            PolicyAction::Review | PolicyAction::RequireApproval | PolicyAction::Warn
        )
}

fn should_fetch_page(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn entity_level_str(level: &EntityLevel) -> &'static str {
    match level {
        EntityLevel::Domain => "domain",
        EntityLevel::Subdomain => "subdomain",
        EntityLevel::Url => "url",
        EntityLevel::Page => "page",
    }
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
