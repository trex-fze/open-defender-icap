use chrono::{DateTime, Utc};
use common_types::normalizer::normalize_target;
use reqwest::Client;
use serde_json::{json, Map, Value};
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
}

impl ElasticWriter {
    pub fn new(
        base_url: &str,
        api_key: Option<String>,
        username: Option<String>,
        password: Option<String>,
        retry_attempts: usize,
    ) -> anyhow::Result<Self> {
        let client = Client::builder().build()?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            username,
            password,
            retry_attempts: retry_attempts.max(1),
        })
    }

    pub async fn bulk_index(&self, index_prefix: String, events: Vec<Value>) -> anyhow::Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let mut body = String::new();
        for event in events {
            let (index, normalized) = normalize_event(index_prefix.clone(), event);
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

fn normalize_event(index_prefix: String, mut event: Value) -> (String, Value) {
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
        enrich_squid_event(map);
        map.entry("ingested_at".to_string())
            .or_insert_with(|| json!(Utc::now()));
        if let Some(trace) = trace_value {
            map.entry("trace_id".to_string())
                .or_insert(Value::String(trace));
        }
    }

    (index, event)
}

fn enrich_squid_event(map: &mut Map<String, Value>) {
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
                    Value::String(if inferred == "block" { "blocked" } else { "allowed" }.into()),
                );
            }
        }
    }

    if let Some(http_method) = method.as_deref() {
        ensure_path_str(map, &["http", "request", "method"], http_method);
    }

    if let Some(raw_target) = target.as_deref() {
        if let Some((domain, canonical_url)) = parse_target_domain_url(raw_target, method.as_deref()) {
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
        let (index, value) = normalize_event("traffic-events".into(), event);
        assert_eq!(index, "traffic-events-2026.03.23");
        assert!(value.get("ingested_at").is_some());
    }

    #[test]
    fn enriches_connect_message_with_domain_and_inferred_action() {
        let event = json!({
            "@timestamp": "2026-04-04T16:07:22.839Z",
            "message": "1775318877.918      7 172.66.0.227 NONE_NONE/403 1058 CONNECT dice.com:443 - HIER_NONE/- text/html"
        });
        let (_index, value) = normalize_event("traffic-events".into(), event);
        assert_eq!(
            value.pointer("/destination/domain").and_then(Value::as_str),
            Some("dice.com")
        );
        assert_eq!(
            value.get("recommended_action_inferred").and_then(Value::as_str),
            Some("block")
        );
        assert_eq!(
            value.pointer("/http/response/status_code").and_then(Value::as_i64),
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
        let (_index, value) = normalize_event("traffic-events".into(), event);
        assert_eq!(
            value.pointer("/destination/domain").and_then(Value::as_str),
            Some("preset.example")
        );
        assert_eq!(
            value.get("recommended_action_inferred").and_then(Value::as_str),
            Some("allow")
        );
    }
}
