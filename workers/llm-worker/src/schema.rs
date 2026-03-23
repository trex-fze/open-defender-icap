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
    pub fn validate(self) -> Result<Self> {
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(anyhow!("confidence must be between 0 and 1"));
        }
        if !matches!(
            self.risk_level.as_str(),
            "low" | "medium" | "high" | "critical"
        ) {
            return Err(anyhow!("invalid risk_level"));
        }
        if !matches!(
            self.recommended_action.as_str(),
            "Allow" | "Block" | "Warn" | "Monitor" | "Review" | "RequireApproval"
        ) {
            return Err(anyhow!("invalid recommended_action"));
        }
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
        assert!(resp.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_action() {
        let resp = LlmResponse {
            primary_category: "Malware".into(),
            subcategory: "C2".into(),
            risk_level: "high".into(),
            confidence: 0.9,
            recommended_action: "DROP".into(),
        };
        assert!(resp.validate().is_err());
    }
}
