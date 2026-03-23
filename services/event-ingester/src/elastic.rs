use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::{json, Value};
use tracing::{debug, warn};

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
            if let Some(key) = &self.api_key {
                req = req.header("Authorization", format!("ApiKey {}", key));
            } else if let (Some(user), Some(pass)) = (&self.username, &self.password) {
                req = req.basic_auth(user, Some(pass));
            }

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
        map.entry("ingested_at".to_string())
            .or_insert_with(|| json!(Utc::now()));
        if let Some(trace) = trace_value {
            map.entry("trace_id".to_string())
                .or_insert(Value::String(trace));
        }
    }

    (index, event)
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
}
