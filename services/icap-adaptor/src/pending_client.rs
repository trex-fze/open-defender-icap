use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct PendingClient {
    base_url: String,
    admin_token: Option<String>,
    http: Client,
}

#[derive(Debug, Serialize)]
struct PendingUpsertRequest<'a> {
    status: &'a str,
    base_url: Option<&'a str>,
}

impl PendingClient {
    pub fn new(base_url: String, admin_token: Option<String>) -> Result<Self> {
        if base_url.trim().is_empty() {
            return Err(anyhow!("admin api endpoint required"));
        }
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self {
            base_url,
            admin_token,
            http,
        })
    }

    pub async fn upsert_pending(&self, normalized_key: &str, base_url: Option<&str>) -> Result<()> {
        let url = format!(
            "{}/api/v1/classifications/{}/pending",
            self.base_url.trim_end_matches('/'),
            urlencoding::encode(normalized_key)
        );
        let mut request = self.http.post(url).json(&PendingUpsertRequest {
            status: "waiting_content",
            base_url,
        });
        if let Some(token) = self.admin_token.as_deref() {
            request = request.header("X-Admin-Token", token);
        }
        request.send().await?.error_for_status()?;
        Ok(())
    }
}
