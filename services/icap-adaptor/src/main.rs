mod cache;
mod config;
mod icap;
mod jobs;
mod metrics;
mod pending_client;
mod policy_client;

use anyhow::Result;
use once_cell::sync::Lazy;
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, instrument, warn, Level};

use cache::CacheClient;
use common_types::{
    normalizer::normalize_target, ClassificationVerdict, EntityLevel, NormalizedTarget,
    PageFetchJob, PolicyAction, PolicyDecision, PolicyDecisionRequest,
};
use jobs::{ClassificationJob, JobPublisher, PageFetchPublisher};
use pending_client::PendingClient;
use policy_client::PolicyClient;
use url::Url;

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
        .map(|queue| {
            PageFetchPublisher::new(&queue.redis_url, queue.stream.clone(), queue.ttl_seconds)
        })
        .transpose()?;

    let pending_client = cfg
        .admin_api
        .as_ref()
        .map(|admin| PendingClient::new(admin.base_url.clone(), admin.admin_token.clone()))
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
        let pending_client = pending_client.clone();
        info!(target = "svc-icap", ?peer, "accepted connection");
        tokio::spawn(async move {
            if let Err(err) = handle_connection(
                socket,
                cfg,
                cache,
                policy_client,
                job_publisher,
                page_fetch_publisher,
                pending_client,
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
    pending_client: Option<PendingClient>,
) -> Result<()> {
    let roundtrip_start = tokio::time::Instant::now();
    let mut buf = vec![0u8; cfg.preview_size.max(1024)];
    let n = socket.read(&mut buf).await?;
    let raw = String::from_utf8_lossy(&buf[..n]);
    let icap_req = icap::IcapRequest::parse(&raw)?;
    if icap_req.method.eq_ignore_ascii_case("OPTIONS") {
        socket
            .write_all(icap_options_response(cfg.preview_size).as_bytes())
            .await?;
        socket.shutdown().await?;
        return Ok(());
    }

    let http_host = icap_req
        .http_host
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("HTTP host header missing"))?;
    let http_path = icap_req
        .http_path
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("HTTP path missing"))?;
    let normalized = normalize_target(http_host, http_path, icap_req.http_scheme.as_deref())?;
    if let Some(trace_id) = &icap_req.trace_id {
        tracing::Span::current().record("trace_id", &tracing::field::display(trace_id));
    }

    let identity = extract_identity(&icap_req.headers);

    if let Some((decision, source)) =
        resolve_cached_decision(&cache, &normalized.normalized_key, &normalized).await?
    {
        if !matches!(decision.action, PolicyAction::ContentPending) {
            if let Some(client) = &pending_client {
                if let Err(err) = client.clear_pending(&normalized.normalized_key).await {
                    warn!(
                        target = "svc-icap",
                        %err,
                        normalized_key = %normalized.normalized_key,
                        "failed to clear stale pending classification request on cache hit"
                    );
                }
            }
        }
        metrics::record_cache_hit();
        info!(
            target = "svc-icap",
            normalized_key = %normalized.normalized_key,
            action = ?decision.action,
            decision_source = source,
            "cache decision"
        );
        let response = icap_response(&decision.action);
        socket.write_all(response.as_bytes()).await?;
        socket.shutdown().await?;
        metrics::observe_squid_roundtrip(roundtrip_start.elapsed().as_secs_f64());
        return Ok(());
    }

    metrics::record_cache_miss();
    let request = PolicyDecisionRequest {
        normalized_key: normalized.normalized_key.clone(),
        entity_level: normalized.entity_level.clone(),
        source_ip: http_host.to_string(),
        user_id: identity.user_id.clone(),
        group_ids: identity.group_ids.clone(),
    };
    let start = tokio::time::Instant::now();
    let decision = policy_client.evaluate(&request).await.map_err(|err| {
        metrics::record_error();
        err
    })?;
    let latency = start.elapsed().as_secs_f64();
    metrics::observe_policy_latency(latency);
    info!(
        target = "svc-icap",
        normalized_key = %normalized.normalized_key,
        action = ?decision.action,
        "policy decision"
    );

    let classification_required = should_require_content_pending(
        cfg.require_content,
        &decision.action,
        decision.verdict.as_ref(),
    );
    let requires_pending = classification_required;
    let base_url = derive_base_url(&normalized.full_url)
        .or_else(|| Some(fallback_base_url(&normalized.hostname)));

    let mut response_decision = decision.clone();
    if requires_pending {
        response_decision.action = PolicyAction::ContentPending;
        response_decision.verdict = None;
        cache
            .set(
                normalized.normalized_key.clone(),
                response_decision.clone(),
                Duration::from_secs(cfg.pending_cache_ttl_seconds.max(30)),
            )
            .await?;
    } else {
        cache
            .set(
                normalized.normalized_key.clone(),
                response_decision.clone(),
                Duration::from_secs(300),
            )
            .await?;
    }

    if requires_pending {
        if let Some(client) = &pending_client {
            if let Err(err) = client
                .upsert_pending(&normalized.normalized_key, base_url.as_deref())
                .await
            {
                warn!(
                    target = "svc-icap",
                    %err,
                    normalized_key = %normalized.normalized_key,
                    "failed to upsert pending classification request"
                );
            }
        }
        if let Some(publisher) = &page_fetch_publisher {
            let job = PageFetchJob {
                normalized_key: normalized.normalized_key.clone(),
                url: base_url
                    .clone()
                    .unwrap_or_else(|| normalized.full_url.clone()),
                hostname: normalized.hostname.clone(),
                trace_id: icap_req.trace_id.clone(),
                ttl_seconds: None,
            };
            if let Err(err) = publisher.publish(job).await {
                error!(target = "svc-icap", %err, "failed to publish page fetch job");
            }
        }
    } else if let Some(client) = &pending_client {
        if let Err(err) = client.clear_pending(&normalized.normalized_key).await {
            warn!(
                target = "svc-icap",
                %err,
                normalized_key = %normalized.normalized_key,
                "failed to clear stale pending classification request"
            );
        }
    }

    if let Some(publisher) = &job_publisher {
        let enqueue = requires_pending
            || action_requires_follow_up(&decision.action, decision.verdict.is_none());
        if enqueue {
            if let Err(err) = publisher
                .publish(&ClassificationJob {
                    normalized_key: &normalized.normalized_key,
                    entity_level: entity_level_str(&normalized.entity_level),
                    hostname: &normalized.hostname,
                    full_url: &normalized.full_url,
                    trace_id: icap_req.trace_id.as_deref().unwrap_or(""),
                    requires_content: requires_pending,
                    base_url: base_url.as_deref(),
                    content_excerpt: None,
                    content_hash: None,
                    content_version: None,
                    content_language: None,
                })
                .await
            {
                error!(target = "svc-icap", %err, "failed to publish classification job");
            }
        }
    }

    let response = icap_response(&response_decision.action);
    socket.write_all(response.as_bytes()).await?;
    socket.shutdown().await?;
    metrics::observe_squid_roundtrip(roundtrip_start.elapsed().as_secs_f64());
    Ok(())
}

fn action_requires_follow_up(action: &PolicyAction, missing_verdict: bool) -> bool {
    missing_verdict
        || matches!(
            action,
            PolicyAction::Review | PolicyAction::RequireApproval | PolicyAction::Warn
        )
}

#[derive(Default, Clone)]
struct IdentityContext {
    user_id: Option<String>,
    group_ids: Vec<String>,
}

fn extract_identity(headers: &std::collections::HashMap<String, String>) -> IdentityContext {
    const MAX_GROUPS: usize = 32;
    let mut ctx = IdentityContext::default();
    if let Some(value) = headers.get("x-user") {
        if let Some(clean) = sanitize_identity(value) {
            ctx.user_id = Some(clean);
        } else if !value.trim().is_empty() {
            warn!(
                target = "svc-icap",
                header = "X-User",
                value,
                "invalid user id header ignored"
            );
        }
    }
    if let Some(value) = headers.get("x-group") {
        for part in value.split(&[',', ';'][..]) {
            if ctx.group_ids.len() >= MAX_GROUPS {
                break;
            }
            if let Some(clean) = sanitize_identity(part) {
                ctx.group_ids.push(clean);
            } else if !part.trim().is_empty() {
                warn!(
                    target = "svc-icap",
                    header = "X-Group",
                    value = part,
                    "invalid group id ignored"
                );
            }
        }
    }
    ctx
}

fn sanitize_identity(value: &str) -> Option<String> {
    const MAX_LEN: usize = 128;
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_LEN {
        return None;
    }
    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '.' | '-' | '_' | ':' | '/'))
    {
        Some(trimmed.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn extracts_valid_identity() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("x-user".into(), "alice@example.com".into());
        headers.insert("x-group".into(), "team-ops, admins".into());
        let ctx = extract_identity(&headers);
        assert_eq!(ctx.user_id.as_deref(), Some("alice@example.com"));
        assert_eq!(ctx.group_ids, vec!["team-ops", "admins"]);
    }

    #[test]
    fn rejects_invalid_tokens() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("x-user".into(), "   ".into());
        headers.insert("x-group".into(), "valid, bad!!".into());
        let ctx = extract_identity(&headers);
        assert!(ctx.user_id.is_none());
        assert_eq!(ctx.group_ids, vec!["valid"]);
    }
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
        ContentPending => build_http_block_response(
            "403 Forbidden",
            "text/html; charset=utf-8",
            PENDING_HTML.as_bytes(),
        ),
        _ => build_http_block_response(
            "403 Forbidden",
            "text/plain; charset=utf-8",
            b"Request blocked.",
        ),
    }
}

fn icap_options_response(preview_size: usize) -> String {
    format!(
        concat!(
            "ICAP/1.0 200 OK\r\n",
            "Methods: REQMOD\r\n",
            "Service: OpenDefender-REQMOD\r\n",
            "ISTag: \"open-defender\"\r\n",
            "Allow: 204\r\n",
            "Preview: {}\r\n",
            "Encapsulated: null-body=0\r\n\r\n"
        ),
        preview_size
    )
}

fn build_http_block_response(status_line: &str, content_type: &str, body: &[u8]) -> String {
    let http_header = format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let header_len = http_header.as_bytes().len();
    let chunk_prefix = format!("{:X}\r\n", body.len());
    let mut icap = format!(
        "ICAP/1.0 200 OK\r\nEncapsulated: res-hdr=0, res-body={}\r\n\r\n{}",
        header_len, http_header
    );
    icap.push_str(&chunk_prefix);
    icap.push_str(&String::from_utf8_lossy(body));
    icap.push_str("\r\n0\r\n\r\n");
    icap
}

fn derive_base_url(full_url: &str) -> Option<String> {
    let parsed = Url::parse(full_url).ok()?;
    let host = parsed.host_str()?;
    let scheme = parsed.scheme();
    Some(format!("{scheme}://{host}/"))
}

fn fallback_base_url(hostname: &str) -> String {
    format!("https://{hostname}/")
}

async fn resolve_cached_decision(
    cache: &CacheClient,
    normalized_key: &str,
    normalized: &NormalizedTarget,
) -> Result<Option<(PolicyDecision, &'static str)>> {
    if let Some(decision) = cache.get(normalized_key).await? {
        return Ok(Some((decision, "exact")));
    }

    let Some(ancestor_key) = ancestor_domain_key(normalized) else {
        return Ok(None);
    };
    if let Some(decision) = cache.get(&ancestor_key).await? {
        if is_inheritable_ancestor_action(&decision.action) {
            return Ok(Some((decision, "ancestor")));
        }
    }

    Ok(None)
}

fn ancestor_domain_key(normalized: &NormalizedTarget) -> Option<String> {
    if normalized.entity_level != EntityLevel::Subdomain {
        return None;
    }
    if normalized.registered_domain == normalized.hostname {
        return None;
    }
    Some(format!("domain:{}", normalized.registered_domain))
}

fn is_inheritable_ancestor_action(action: &PolicyAction) -> bool {
    matches!(
        action,
        PolicyAction::Allow | PolicyAction::Monitor | PolicyAction::Block
    )
}

fn should_require_content_pending(
    require_content: bool,
    action: &PolicyAction,
    verdict: Option<&ClassificationVerdict>,
) -> bool {
    let has_unknown_fallback = verdict
        .map(|v| {
            v.primary_category
                .eq_ignore_ascii_case("unknown-unclassified")
        })
        .unwrap_or(false);
    require_content
        && (verdict.is_none() || has_unknown_fallback)
        && matches!(action, PolicyAction::Allow | PolicyAction::Monitor)
}

static PENDING_HTML: Lazy<String> = Lazy::new(|| {
    let template = r#"<html><head><meta charset="utf-8" /><title>Site Under Classification</title>
<style>body{font-family:sans-serif;background:#0b1221;color:#f4f7ff;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;} .card{background:#141d33;border:1px solid #1f2a48;border-radius:12px;padding:32px;max-width:460px;text-align:center;box-shadow:0 20px 60px rgba(0,0,0,0.4);} h1{font-size:1.5rem;margin-bottom:12px;} p{line-height:1.5;color:#c6d4f5;} .hint{margin-top:20px;font-size:0.9rem;color:#8ea0ce;}</style>
</head><body><div class="card"><h1>Site Under Classification</h1><p>Security is verifying this destination with full page content. Access will be restored automatically once the scan completes.</p><p class="hint">Please retry in a moment or contact Security if this persists.</p></div></body></html>"#;
    template.to_string()
});

#[cfg(test)]
mod icap_response_tests {
    use super::*;

    #[test]
    fn content_pending_response_contains_http_block() {
        let response = icap_response(&PolicyAction::ContentPending);
        assert!(response.contains("ICAP/1.0 200 OK"));
        assert!(response.contains("Encapsulated: res-hdr=0, res-body="));
        assert!(response.contains("HTTP/1.1 403 Forbidden"));
        assert!(response.contains("Site Under Classification"));
    }

    #[test]
    fn block_response_contains_text_body() {
        let response = icap_response(&PolicyAction::Block);
        assert!(response.contains("HTTP/1.1 403 Forbidden"));
        assert!(response.contains("Request blocked."));
    }

    #[test]
    fn ancestor_key_generated_for_subdomain() {
        let normalized = NormalizedTarget {
            entity_level: EntityLevel::Subdomain,
            normalized_key: "subdomain:www.google.com".into(),
            hostname: "www.google.com".into(),
            registered_domain: "google.com".into(),
            full_url: "https://www.google.com/".into(),
        };
        assert_eq!(
            ancestor_domain_key(&normalized),
            Some("domain:google.com".into())
        );
    }

    #[test]
    fn ancestor_key_not_generated_for_domain_level() {
        let normalized = NormalizedTarget {
            entity_level: EntityLevel::Domain,
            normalized_key: "domain:google.com".into(),
            hostname: "google.com".into(),
            registered_domain: "google.com".into(),
            full_url: "https://google.com/".into(),
        };
        assert!(ancestor_domain_key(&normalized).is_none());
    }

    #[test]
    fn ancestor_inheritance_allows_monitor_and_blocks_block() {
        assert!(is_inheritable_ancestor_action(&PolicyAction::Allow));
        assert!(is_inheritable_ancestor_action(&PolicyAction::Monitor));
        assert!(is_inheritable_ancestor_action(&PolicyAction::Block));
        assert!(!is_inheritable_ancestor_action(&PolicyAction::Warn));
        assert!(!is_inheritable_ancestor_action(
            &PolicyAction::ContentPending
        ));
    }

    #[test]
    fn pending_required_only_when_verdict_missing() {
        let social_verdict = ClassificationVerdict {
            primary_category: "social-media".into(),
            subcategory: "photo-sharing".into(),
            risk_level: "medium".into(),
            confidence: 0.8,
            recommended_action: PolicyAction::Monitor,
        };
        let unknown_verdict = ClassificationVerdict {
            primary_category: "unknown-unclassified".into(),
            subcategory: "Allow everything else".into(),
            risk_level: "medium".into(),
            confidence: 0.5,
            recommended_action: PolicyAction::Allow,
        };

        assert!(should_require_content_pending(
            true,
            &PolicyAction::Allow,
            None
        ));
        assert!(should_require_content_pending(
            true,
            &PolicyAction::Allow,
            Some(&unknown_verdict)
        ));
        assert!(!should_require_content_pending(
            true,
            &PolicyAction::Allow,
            Some(&social_verdict)
        ));
        assert!(!should_require_content_pending(
            true,
            &PolicyAction::Block,
            None
        ));
        assert!(!should_require_content_pending(
            false,
            &PolicyAction::Allow,
            None
        ));
    }
}
