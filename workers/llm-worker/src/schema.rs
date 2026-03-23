use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct PromptPayload<'a> {
    pub normalized_key: &'a str,
    pub hostname: &'a str,
    pub full_url: &'a str,
    pub entity_level: &'a str,
    pub trace_id: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct LlmResponse {
    pub primary_category: String,
    pub subcategory: String,
    pub risk_level: String,
    pub confidence: f32,
    pub recommended_action: String,
}

impl LlmResponse {
    pub fn normalize(mut self) -> Result<Self> {
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(anyhow!("confidence must be between 0 and 1"));
        }

        self.risk_level = self.risk_level.to_lowercase();
        if !matches!(
            self.risk_level.as_str(),
            "low" | "medium" | "high" | "critical"
        ) {
            return Err(anyhow!("invalid risk_level"));
        }

        let normalized_action = match self.recommended_action.to_lowercase().as_str() {
            "allow" => "Allow",
            "block" => "Block",
            "warn" => "Warn",
            "monitor" => "Monitor",
            "review" => "Review",
            "requireapproval" | "require_approval" | "require-approval" => "RequireApproval",
            other => {
                return Err(anyhow!("invalid recommended_action: {other}"));
            }
        };
        self.recommended_action = normalized_action.to_string();

        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_response() {
        let resp = LlmResponse {
            primary_category: "Social".into(),
            subcategory: "General".into(),
            risk_level: "medium".into(),
            confidence: 0.8,
            recommended_action: "Review".into(),
        };
        assert!(resp.normalize().is_ok());
    }

    #[test]
    fn rejects_invalid_action() {
        let resp = LlmResponse {
            primary_category: "Malware".into(),
            subcategory: "C2".into(),
            risk_level: "HIGH".into(),
            confidence: 0.9,
            recommended_action: "DROP".into(),
        };
        assert!(resp.normalize().is_err());
    }
}
