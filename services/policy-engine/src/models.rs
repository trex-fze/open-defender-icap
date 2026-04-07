use common_types::{PolicyAction, PolicyDecision};
use policy_dsl::PolicyDocument;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, Clone)]
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
    pub subcategory_hint: Option<String>,
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
    pub policy_id: Option<String>,
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

#[derive(Debug, Serialize)]
pub struct SimulationResponse {
    pub decision: PolicyDecision,
    pub matched_rule_id: Option<String>,
    pub policy_version: String,
    pub mode: String,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SimulationMode {
    Runtime,
    PolicyOnly,
}

impl SimulationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SimulationMode::Runtime => "runtime",
            SimulationMode::PolicyOnly => "policy_only",
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SimulatePolicyRequest {
    #[serde(default = "default_simulation_mode")]
    pub mode: SimulationMode,
    #[serde(flatten)]
    pub request: DecisionRequest,
}

fn default_simulation_mode() -> SimulationMode {
    SimulationMode::Runtime
}

impl PolicyListResponse {
    pub fn from_store(
        version: String,
        policy_id: Option<Uuid>,
        rules: Vec<policy_dsl::PolicyRule>,
    ) -> Self {
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
            policy_id: policy_id.map(|id| id.to_string()),
            version,
            rules: summaries,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PolicyCreateRequest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub created_by: Option<String>,
    pub rules: Vec<policy_dsl::PolicyRule>,
}

impl PolicyCreateRequest {
    pub fn into_document(self) -> PolicyDocument {
        PolicyDocument {
            version: self.version,
            rules: self.rules,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PolicyUpdateRequest {
    pub version: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub rules: Option<Vec<policy_dsl::PolicyRule>>,
}

impl ErrorResponse {
    pub fn forbidden() -> Self {
        Self {
            error_code: "FORBIDDEN",
            message: "insufficient privileges".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulate_request_defaults_to_runtime_mode() {
        let payload = serde_json::json!({
            "normalized_key": "domain:example.com",
            "entity_level": "domain",
            "source_ip": "192.0.2.10"
        });
        let parsed: SimulatePolicyRequest = serde_json::from_value(payload).expect("parse request");
        assert_eq!(parsed.mode, SimulationMode::Runtime);
    }

    #[test]
    fn simulate_request_accepts_policy_only_mode() {
        let payload = serde_json::json!({
            "mode": "policy_only",
            "normalized_key": "domain:example.com",
            "entity_level": "domain",
            "source_ip": "192.0.2.10"
        });
        let parsed: SimulatePolicyRequest = serde_json::from_value(payload).expect("parse request");
        assert_eq!(parsed.mode, SimulationMode::PolicyOnly);
    }
}
