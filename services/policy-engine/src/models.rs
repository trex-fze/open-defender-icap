use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct DecisionRequest {
    pub normalized_key: String,
    pub entity_level: String,
    pub source_ip: String,
    pub user_id: Option<String>,
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
