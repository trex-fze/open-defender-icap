use chrono::{DateTime, Utc};
use common_types::normalizer::normalize_target;
use reqwest::Client;
use serde_json::{json, Map, Value};
use std::net::IpAddr;
use tracing::{debug, warn};
use url::Url;

#[derive(Clone)]
pub struct ElasticWriter {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    username: Option<String>,
    password: Option<String>,
    retry_attempts: usize,
    trust_proxy_headers: bool,
    trusted_proxy_cidrs: Vec<TrustedCidr>,
}

#[derive(Clone, Debug)]
struct TrustedCidr {
    network: IpAddr,
    prefix_len: u8,
}

impl ElasticWriter {
    pub fn new(
        base_url: &str,
        api_key: Option<String>,
        username: Option<String>,
        password: Option<String>,
        retry_attempts: usize,
        trust_proxy_headers: bool,
        trusted_proxy_cidrs: Vec<String>,
    ) -> anyhow::Result<Self> {
        let client = Client::builder().build()?;
        let trusted_proxy_cidrs = trusted_proxy_cidrs
            .into_iter()
            .map(|raw| parse_cidr(&raw))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            username,
            password,
            retry_attempts: retry_attempts.max(1),
            trust_proxy_headers,
            trusted_proxy_cidrs,
        })
    }

    pub async fn bulk_index(&self, index_prefix: String, events: Vec<Value>) -> anyhow::Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let mut body = String::new();
        for event in events {
            let (index, normalized) = normalize_event(
                index_prefix.clone(),
                event,
                self.trust_proxy_headers,
                &self.trusted_proxy_cidrs,
            );
            let meta = json!({
                "index": {
                    "_index": index,
                }
            });
            body.push_str(&serde_json::to_string(&meta)?);
            body.push('\n');
            body.push_str(&serde_json::to_string(&normalized)?);
            body.push('\n');
        }

        let url = format!("{}/_bulk", self.base_url);
        let mut attempt = 0;
        loop {
            attempt += 1;
            let mut req = self
                .client
                .post(&url)
                .header("Content-Type", "application/x-ndjson")
                .body(body.clone());
            req = self.attach_auth(req);

            match req.send().await {
                Ok(response) => {
                    let status = response.status();
                    let text = response.text().await.unwrap_or_default();
                    if !status.is_success() {
                        if attempt >= self.retry_attempts {
                            return Err(anyhow::anyhow!(
                                "bulk index failed after {} attempts: {} {}",
                                attempt,
                                status,
                                text
                            ));
                        }
                        warn!(target = "svc-ingest", %status, attempt, "bulk request failed, retrying");
                        continue;
                    }

                    if text.contains("\"errors\":true") {
                        return Err(anyhow::anyhow!("bulk index reported errors: {}", text));
                    }
                    debug!(target = "svc-ingest", attempt, "bulk index ok");
                    return Ok(());
                }
                Err(err) => {
                    if attempt >= self.retry_attempts {
                        return Err(anyhow::anyhow!("failed to send bulk request: {err}"));
                    }
                    warn!(target = "svc-ingest", %err, attempt, "bulk request error, retrying");
                }
            }
        }
    }

    pub async fn put_index_template(&self, name: &str, template: &Value) -> anyhow::Result<()> {
        let url = format!("{}/_index_template/{}", self.base_url, name);
        self.put_json(&url, template).await
    }

    pub async fn put_ilm_policy(&self, name: &str, policy: &Value) -> anyhow::Result<()> {
        let url = format!("{}/_ilm/policy/{}", self.base_url, name);
        self.put_json(&url, policy).await
    }

    async fn put_json(&self, url: &str, body: &Value) -> anyhow::Result<()> {
        let mut req = self.client.put(url).json(body);
        req = self.attach_auth(req);
        let resp = req.send().await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(anyhow::anyhow!(
                "elastic request failed {}: {}",
                status,
                text
            ))
        }
    }

    fn attach_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(key) = &self.api_key {
            req.header("Authorization", format!("ApiKey {}", key))
        } else if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            req.basic_auth(user, Some(pass))
        } else {
            req
        }
    }
}

fn normalize_event(
    index_prefix: String,
    mut event: Value,
    trust_proxy_headers: bool,
    trusted_proxy_cidrs: &[TrustedCidr],
) -> (String, Value) {
    let timestamp = event
        .get("@timestamp")
        .and_then(Value::as_str)
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let index = format!("{}-{}", index_prefix, timestamp.format("%Y.%m.%d"));

    let trace_value = event
        .pointer("/od/trace_id")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    if let Value::Object(ref mut map) = event {
        enrich_squid_event(map, trust_proxy_headers, trusted_proxy_cidrs);
        map.entry("ingested_at".to_string())
            .or_insert_with(|| json!(Utc::now()));
        if let Some(trace) = trace_value {
            map.entry("trace_id".to_string())
                .or_insert(Value::String(trace));
        }
    }

    (index, event)
}

fn enrich_squid_event(
    map: &mut Map<String, Value>,
    trust_proxy_headers: bool,
    trusted_proxy_cidrs: &[TrustedCidr],
) {
    let message = map
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if message.is_empty() {
        return;
    }

    let mut parts = message.split_whitespace();
    let _timestamp = parts.next();
    let _duration_ms = parts.next();
    let source_ip = parts.next().map(str::to_string);
    let status_token = parts.next().map(str::to_string);
    let _bytes = parts.next();
    let method = parts.next().map(str::to_string);
    let target = parts.next().map(str::to_string);

    if let Some(ip) = source_ip.as_deref() {
        ensure_path_str(map, &["source", "ip"], ip);
        if let Ok(peer_ip) = ip.parse::<IpAddr>() {
            let mut client_ip = peer_ip;
            let mut client_ip_source = "peer";
            let (forwarded_raw, xff_raw) = extract_forwarding_headers_from_log_line(&message);

            if let Some(forwarded) = forwarded_raw {
                ensure_path_str(map, &["od", "forwarded_raw"], forwarded);
            }
            if let Some(xff) = xff_raw {
                if !xff.is_empty() {
                    ensure_path_str(map, &["od", "forwarded_for_raw"], xff);
                }
            }

            if trust_proxy_headers && is_trusted_proxy(peer_ip, trusted_proxy_cidrs) {
                if let Some(forwarded) = forwarded_raw {
                    if let Some(original_ip) = first_valid_forwarded_header_ip(forwarded) {
                        client_ip = original_ip;
                        client_ip_source = "forwarded";
                    }
                }
                if client_ip_source == "peer" {
                    if let Some(xff) = xff_raw {
                        if let Some(original_ip) = first_valid_x_forwarded_for_ip(xff) {
                            client_ip = original_ip;
                            client_ip_source = "x-forwarded-for";
                        }
                    }
                }
            }

            ensure_path_str(map, &["client", "ip"], &client_ip.to_string());
            ensure_path_str(map, &["od", "client_ip_source"], client_ip_source);
        } else {
            ensure_path_str(map, &["od", "client_ip_source"], "unknown");
        }
    }

    if let Some(token) = status_token.as_deref() {
        if let Some((result_code, status_code)) = token.split_once('/') {
            ensure_path_str(map, &["proxy", "result_code"], result_code);
            if let Ok(code) = status_code.parse::<i64>() {
                ensure_path_number(map, &["http", "response", "status_code"], code);
                let inferred = infer_action_from_status(code);
                ensure_value(
                    map,
                    "recommended_action_inferred",
                    Value::String(inferred.to_string()),
                );
                ensure_value(
                    map,
                    "traffic_class",
                    Value::String(
                        if inferred == "block" {
                            "blocked"
                        } else {
                            "allowed"
                        }
                        .into(),
                    ),
                );
            }
        }
    }

    if let Some(http_method) = method.as_deref() {
        ensure_path_str(map, &["http", "request", "method"], http_method);
    }

    if let Some(raw_target) = target.as_deref() {
        if let Some((domain, canonical_url)) =
            parse_target_domain_url(raw_target, method.as_deref())
        {
            ensure_path_str(map, &["destination", "domain"], &domain);
            ensure_path_str(map, &["url", "full"], &canonical_url);
            if let Ok(normalized) = normalize_target(&domain, "/", Some("https")) {
                ensure_value(
                    map,
                    "normalized_key",
                    Value::String(normalized.normalized_key),
                );
            }
        }
    }
}

fn extract_forwarding_headers_from_log_line(message: &str) -> (Option<&str>, Option<&str>) {
    let quoted = extract_trailing_quoted_fields(message, 2);
    match quoted.as_slice() {
        [] => (None, None),
        [xff] => (None, Some(*xff)),
        [forwarded, xff] => (Some(*forwarded), Some(*xff)),
        _ => (None, None),
    }
}

fn extract_trailing_quoted_fields<'a>(message: &'a str, max_fields: usize) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut cursor = message.trim_end();
    for _ in 0..max_fields {
        let end = match cursor.rfind('"') {
            Some(idx) if idx == cursor.len() - 1 => idx,
            _ => break,
        };
        let before_end = &cursor[..end];
        let start = match before_end.rfind('"') {
            Some(idx) => idx,
            None => break,
        };
        out.push(before_end[start + 1..].trim());
        cursor = before_end[..start].trim_end();
    }
    out.reverse();
    out
}

fn first_valid_forwarded_header_ip(forwarded: &str) -> Option<IpAddr> {
    for element in forwarded.split(',') {
        for pair in element.split(';') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case("for") {
                if let Some(ip) = parse_ip_token(value) {
                    return Some(ip);
                }
            }
        }
    }
    None
}

fn first_valid_x_forwarded_for_ip(xff: &str) -> Option<IpAddr> {
    xff.split(',').find_map(parse_ip_token)
}

fn parse_ip_token(raw: &str) -> Option<IpAddr> {
    let token = raw.trim().trim_matches('"').trim();
    if token.is_empty() || token.eq_ignore_ascii_case("unknown") || token.starts_with('_') {
        return None;
    }

    if let Ok(ip) = token.parse::<IpAddr>() {
        return Some(ip);
    }

    if let Some(rest) = token.strip_prefix('[') {
        let end = rest.find(']')?;
        let ip_part = &rest[..end];
        if let Ok(ip) = ip_part.parse::<IpAddr>() {
            return Some(ip);
        }
    }

    if let Some((host, port)) = token.rsplit_once(':') {
        if port.chars().all(|ch| ch.is_ascii_digit()) {
            if let Ok(ip) = host.parse::<IpAddr>() {
                return Some(ip);
            }
        }
    }

    None
}

fn parse_cidr(raw: &str) -> anyhow::Result<TrustedCidr> {
    let (ip_raw, prefix_raw) = raw
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("invalid trusted proxy CIDR: {raw}"))?;
    let network = ip_raw
        .parse::<IpAddr>()
        .map_err(|_| anyhow::anyhow!("invalid trusted proxy CIDR ip: {raw}"))?;
    let prefix_len = prefix_raw
        .parse::<u8>()
        .map_err(|_| anyhow::anyhow!("invalid trusted proxy CIDR prefix: {raw}"))?;
    let max_bits = match network {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    };
    if prefix_len as u16 > max_bits {
        return Err(anyhow::anyhow!("trusted proxy CIDR out of range: {raw}"));
    }
    Ok(TrustedCidr {
        network,
        prefix_len,
    })
}

fn is_trusted_proxy(ip: IpAddr, trusted_proxy_cidrs: &[TrustedCidr]) -> bool {
    trusted_proxy_cidrs
        .iter()
        .any(|cidr| ip_in_cidr(ip, cidr.network, cidr.prefix_len))
}

fn ip_in_cidr(ip: IpAddr, network: IpAddr, prefix_len: u8) -> bool {
    match (ip, network) {
        (IpAddr::V4(ipv4), IpAddr::V4(netv4)) => {
            let ip_u = u32::from(ipv4);
            let net_u = u32::from(netv4);
            let mask = if prefix_len == 0 {
                0
            } else {
                u32::MAX << (32 - prefix_len)
            };
            (ip_u & mask) == (net_u & mask)
        }
        (IpAddr::V6(ipv6), IpAddr::V6(netv6)) => {
            let ip_u = u128::from(ipv6);
            let net_u = u128::from(netv6);
            let mask = if prefix_len == 0 {
                0
            } else {
                u128::MAX << (128 - prefix_len)
            };
            (ip_u & mask) == (net_u & mask)
        }
        _ => false,
    }
}

fn parse_target_domain_url(target: &str, method: Option<&str>) -> Option<(String, String)> {
    if target.starts_with("http://") || target.starts_with("https://") {
        let parsed = Url::parse(target).ok()?;
        let host = parsed.host_str()?.to_ascii_lowercase();
        return Some((host, target.to_string()));
    }

    if method
        .map(|m| m.eq_ignore_ascii_case("CONNECT"))
        .unwrap_or(false)
    {
        let host = target.split(':').next()?.trim().to_ascii_lowercase();
        if host.is_empty() {
            return None;
        }
        let url = format!("https://{host}/");
        return Some((host, url));
    }

    None
}

fn infer_action_from_status(status_code: i64) -> &'static str {
    match status_code {
        403 | 407 | 451 => "block",
        _ => "allow",
    }
}

fn ensure_value(map: &mut Map<String, Value>, key: &str, value: Value) {
    match map.get(key) {
        Some(existing) if !existing.is_null() => {}
        _ => {
            map.insert(key.to_string(), value);
        }
    }
}

fn ensure_path_str(map: &mut Map<String, Value>, path: &[&str], value: &str) {
    ensure_path_value(map, path, Value::String(value.to_string()));
}

fn ensure_path_number(map: &mut Map<String, Value>, path: &[&str], value: i64) {
    ensure_path_value(map, path, Value::Number(value.into()));
}

fn ensure_path_value(map: &mut Map<String, Value>, path: &[&str], value: Value) {
    if path.is_empty() {
        return;
    }
    let head = path[0];
    if path.len() == 1 {
        match map.get(head) {
            Some(existing) if !existing.is_null() => {}
            _ => {
                map.insert(head.to_string(), value);
            }
        }
        return;
    }

    let entry = map
        .entry(head.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(child) = entry {
        ensure_path_value(child, &path[1..], value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_daily_index_from_timestamp() {
        let event = json!({
            "@timestamp": "2026-03-23T12:00:00Z",
            "message": "ok"
        });
        let (index, value) = normalize_event("traffic-events".into(), event, false, &[]);
        assert_eq!(index, "traffic-events-2026.03.23");
        assert!(value.get("ingested_at").is_some());
    }

    #[test]
    fn enriches_connect_message_with_domain_and_inferred_action() {
        let event = json!({
            "@timestamp": "2026-04-04T16:07:22.839Z",
            "message": "1775318877.918      7 172.66.0.227 NONE_NONE/403 1058 CONNECT dice.com:443 - HIER_NONE/- text/html"
        });
        let (_index, value) = normalize_event("traffic-events".into(), event, false, &[]);
        assert_eq!(
            value.pointer("/destination/domain").and_then(Value::as_str),
            Some("dice.com")
        );
        assert_eq!(
            value
                .get("recommended_action_inferred")
                .and_then(Value::as_str),
            Some("block")
        );
        assert_eq!(
            value
                .pointer("/http/response/status_code")
                .and_then(Value::as_i64),
            Some(403)
        );
    }

    #[test]
    fn keeps_existing_structured_fields() {
        let event = json!({
            "@timestamp": "2026-04-04T16:07:22.839Z",
            "message": "1775318877.918      7 172.66.0.227 NONE_NONE/403 1058 CONNECT dice.com:443 - HIER_NONE/- text/html",
            "recommended_action_inferred": "allow",
            "destination": { "domain": "preset.example" }
        });
        let (_index, value) = normalize_event("traffic-events".into(), event, false, &[]);
        assert_eq!(
            value.pointer("/destination/domain").and_then(Value::as_str),
            Some("preset.example")
        );
        assert_eq!(
            value
                .get("recommended_action_inferred")
                .and_then(Value::as_str),
            Some("allow")
        );
    }

    #[test]
    fn populates_client_ip_from_trusted_forwarded_header() {
        let trusted = vec![parse_cidr("192.168.1.0/24").expect("valid cidr")];
        let event = json!({
            "@timestamp": "2026-04-04T16:07:22.839Z",
            "message": "1775318877.918 7 192.168.1.44 TCP_TUNNEL/200 39 CONNECT www.bing.com:443 - HIER_DIRECT/150.171.28.16 - \"for=203.0.113.9;proto=https;host=www.bing.com\" \"198.51.100.4\""
        });
        let (_index, value) = normalize_event("traffic-events".into(), event, true, &trusted);
        assert_eq!(
            value.pointer("/source/ip").and_then(Value::as_str),
            Some("192.168.1.44")
        );
        assert_eq!(
            value.pointer("/client/ip").and_then(Value::as_str),
            Some("203.0.113.9")
        );
        assert_eq!(
            value.pointer("/od/client_ip_source").and_then(Value::as_str),
            Some("forwarded")
        );
    }

    #[test]
    fn ignores_xff_from_untrusted_peer() {
        let trusted = vec![parse_cidr("192.168.1.0/24").expect("valid cidr")];
        let event = json!({
            "@timestamp": "2026-04-04T16:07:22.839Z",
            "message": "1775318877.918 7 10.10.10.10 TCP_TUNNEL/200 39 CONNECT www.bing.com:443 - HIER_DIRECT/150.171.28.16 - \"for=203.0.113.9\" \"203.0.113.9\""
        });
        let (_index, value) = normalize_event("traffic-events".into(), event, true, &trusted);
        assert_eq!(
            value.pointer("/client/ip").and_then(Value::as_str),
            Some("10.10.10.10")
        );
        assert_eq!(
            value.pointer("/od/client_ip_source").and_then(Value::as_str),
            Some("peer")
        );
    }

    #[test]
    fn ignores_forwarding_headers_when_disabled() {
        let trusted = vec![parse_cidr("192.168.1.0/24").expect("valid cidr")];
        let event = json!({
            "@timestamp": "2026-04-04T16:07:22.839Z",
            "message": "1775318877.918 7 192.168.1.44 TCP_TUNNEL/200 39 CONNECT www.bing.com:443 - HIER_DIRECT/150.171.28.16 - \"for=203.0.113.9\" \"203.0.113.9\""
        });
        let (_index, value) = normalize_event("traffic-events".into(), event, false, &trusted);
        assert_eq!(
            value.pointer("/client/ip").and_then(Value::as_str),
            Some("192.168.1.44")
        );
        assert_eq!(
            value.pointer("/od/client_ip_source").and_then(Value::as_str),
            Some("peer")
        );
    }
}
