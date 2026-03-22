use common_types::PolicyAction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct DecisionRequest {
    pub normalized_key: String,
    pub entity_level: String,
    pub source_ip: String,
    pub user_id: Option<String>,
    #[serde(default)]
    pub group_ids: Option<Vec<String>>,
    #[serde(default)]
    pub category_hint: Option<String>,
    #[serde(default)]
    pub risk_hint: Option<String>,
    #[serde(default)]
    pub confidence_hint: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error_code: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct PolicyListResponse {
    pub version: String,
    pub rules: Vec<PolicySummary>,
}

#[derive(Debug, Serialize)]
pub struct PolicySummary {
    pub id: String,
    pub description: Option<String>,
    pub priority: u32,
    pub action: PolicyAction,
}

impl PolicyListResponse {
    pub fn from_rules(version: String, rules: Vec<policy_dsl::PolicyRule>) -> Self {
        let summaries = rules
            .into_iter()
            .map(|rule| PolicySummary {
                id: rule.id,
                description: rule.description,
                priority: rule.priority,
                action: rule.action,
            })
            .collect::<Vec<_>>();
        Self {
            version,
            rules: summaries,
        }
    }
}
