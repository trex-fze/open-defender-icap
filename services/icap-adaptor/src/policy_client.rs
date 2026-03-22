use anyhow::{anyhow, Result};
use common_types::{PolicyDecision, PolicyDecisionRequest};
use reqwest::Client;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct PolicyClient {
    base_url: String,
    http: Client,
}

impl PolicyClient {
    pub fn new(base_url: Option<String>) -> Result<Self> {
        let base_url = base_url.ok_or_else(|| anyhow!("policy endpoint required"))?;
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self { base_url, http })
    }

    pub async fn evaluate(&self, req: &PolicyDecisionRequest) -> Result<PolicyDecision> {
        let url = format!("{}/api/v1/decision", self.base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(url)
            .json(req)
            .send()
            .await?
            .error_for_status()?;
        let decision = response.json::<PolicyDecision>().await?;
        Ok(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn client_errors_without_base_url() {
        let err = PolicyClient::new(None).unwrap_err();
        assert!(err.to_string().contains("policy endpoint"));
    }
}
