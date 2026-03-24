mod bootstrap;
mod config;
mod elastic;
mod metrics;
mod models;

use axum::{
    extract::State, http::HeaderMap, http::StatusCode, routing::get, routing::post, Json, Router,
};
use common_types::{normalizer::normalize_target, PageFetchJob};
use config::IngestConfig;
use elastic::ElasticWriter;
use models::{FilebeatEnvelope, HealthResponse};
use serde_json::Value;
use std::net::SocketAddr;
use tokio::{net::TcpListener, signal, time::Instant};
use tracing::{error, info, warn};
use url::Url;

#[derive(Clone)]
struct AppState {
    writer: ElasticWriter,
    shared_secret: Option<String>,
    index_prefix: String,
    page_fetch: Option<PageFetchPublisher>,
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

    let page_fetch = config
        .page_fetch_redis_url
        .as_ref()
        .map(|url| {
            PageFetchPublisher::new(
                url,
                config.page_fetch_stream.clone(),
                config.page_fetch_ttl_seconds,
            )
        })
        .transpose()?;

    let state = AppState {
        writer,
        shared_secret: config.filebeat_secret.clone(),
        index_prefix: config.elastic_index_prefix.clone(),
        page_fetch,
    };

    let app = Router::new()
        .route("/health/ready", get(ready))
        .route("/metrics", get(metrics::metrics_endpoint))
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

    if let Some(publisher) = &state.page_fetch {
        for event in &docs {
            maybe_publish_page_fetch(publisher, event).await;
        }
    }
    if docs.is_empty() {
        return Ok(StatusCode::ACCEPTED);
    }

    let index_prefix = state.index_prefix.clone();
    let start = Instant::now();
    let event_count = docs.len();
    state
        .writer
        .bulk_index(index_prefix, docs)
        .await
        .map_err(|err| {
            metrics::record_failure();
            error!(target = "svc-ingest", %err, "failed to index events");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to index events".into(),
            )
        })?;
    metrics::record_batch(event_count, start.elapsed().as_secs_f64());

    Ok(StatusCode::ACCEPTED)
}

async fn maybe_publish_page_fetch(publisher: &PageFetchPublisher, event: &Value) {
    match build_page_fetch_job(event) {
        Some(job) => {
            metrics::record_page_fetch_attempt();
            if let Err(err) = publisher.publish(job).await {
                metrics::record_page_fetch_failure();
                warn!(target = "svc-ingest", %err, "failed to enqueue page fetch job");
            } else {
                metrics::record_page_fetch_enqueued();
            }
        }
        None => metrics::record_page_fetch_skipped(),
    }
}

fn build_page_fetch_job(event: &Value) -> Option<PageFetchJob> {
    let raw_url = extract_candidate_url(event)?;
    let normalized_url = ensure_scheme(&raw_url);
    let parsed = Url::parse(&normalized_url).ok()?;
    let host = parsed.host_str()?;

    let mut path = parsed.path().to_string();
    if path.is_empty() {
        path = "/".into();
    }
    if let Some(query) = parsed.query() {
        path.push('?');
        path.push_str(query);
    }

    let normalized = normalize_target(host, &path, Some(parsed.scheme())).ok()?;
    let trace_id = pointer_str(event, "/od/trace_id")
        .or_else(|| pointer_str(event, "/trace_id"))
        .map(|s| s.to_string());
    let ttl_override = event
        .pointer("/od/page_fetch_ttl_seconds")
        .and_then(Value::as_i64)
        .map(|v| v as i32);

    Some(PageFetchJob {
        normalized_key: normalized.normalized_key,
        url: parsed.to_string(),
        hostname: normalized.hostname,
        trace_id,
        ttl_seconds: ttl_override,
    })
}

fn extract_candidate_url(event: &Value) -> Option<String> {
    for pointer in URL_POINTERS {
        if let Some(value) = pointer_str(event, pointer) {
            return Some(value.to_string());
        }
    }

    if let Some(host) = pointer_str(event, "/destination/domain")
        .or_else(|| pointer_str(event, "/server/domain"))
        .or_else(|| pointer_str(event, "/od/hostname"))
    {
        let scheme = pointer_str(event, "/url/scheme").unwrap_or("http");
        let path = pointer_str(event, "/url/path").unwrap_or("/");
        return Some(format!(
            "{}://{}{}",
            scheme,
            host,
            ensure_leading_slash(path)
        ));
    }

    None
}

fn pointer_str<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn ensure_leading_slash(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    }
}

fn ensure_scheme(url: &str) -> String {
    if url.contains("://") {
        url.to_string()
    } else {
        format!("http://{}", url.trim_start_matches("//"))
    }
}

const URL_POINTERS: &[&str] = &[
    "/url/full",
    "/url/original",
    "/url/next",
    "/http/request/url",
    "/http/request/target",
    "/http/request/referrer",
    "/od/url",
    "/destination/url",
];

#[derive(Clone)]
struct PageFetchPublisher {
    client: redis::Client,
    stream: String,
    default_ttl: i32,
}

impl PageFetchPublisher {
    fn new(redis_url: &str, stream: String, default_ttl: i32) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            stream,
            default_ttl: default_ttl.max(60),
        })
    }

    async fn publish(&self, mut job: PageFetchJob) -> anyhow::Result<()> {
        if job.ttl_seconds.unwrap_or(0) <= 0 {
            job.ttl_seconds = Some(self.default_ttl);
        }
        let mut conn = self.client.get_async_connection().await?;
        let payload = serde_json::to_string(&job)?;
        redis::cmd("XADD")
            .arg(&self.stream)
            .arg("*")
            .arg("payload")
            .arg(payload)
            .query_async::<_, ()>(&mut conn)
            .await?;
        Ok(())
    }
}
