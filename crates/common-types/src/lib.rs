use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntityLevel {
    Domain,
    Subdomain,
    Url,
    Page,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyAction {
    Allow,
    Block,
    Warn,
    Monitor,
    Review,
    RequireApproval,
}

impl fmt::Display for PolicyAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PolicyAction::Allow => "Allow",
            PolicyAction::Block => "Block",
            PolicyAction::Warn => "Warn",
            PolicyAction::Monitor => "Monitor",
            PolicyAction::Review => "Review",
            PolicyAction::RequireApproval => "RequireApproval",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedRequest {
    pub trace_id: String,
    pub entity_level: EntityLevel,
    pub normalized_key: String,
    pub source_ip: String,
    pub user_id: Option<String>,
    pub hostname: String,
    pub fqdn: String,
    pub url_path: String,
    pub full_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedTarget {
    pub entity_level: EntityLevel,
    pub normalized_key: String,
    pub hostname: String,
    pub registered_domain: String,
    pub full_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClassificationVerdict {
    pub primary_category: String,
    pub subcategory: String,
    pub risk_level: String,
    pub confidence: f32,
    pub recommended_action: PolicyAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyDecision {
    pub action: PolicyAction,
    pub cache_hit: bool,
    pub verdict: Option<ClassificationVerdict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecisionRequest {
    pub normalized_key: String,
    pub entity_level: EntityLevel,
    pub source_ip: String,
    pub user_id: Option<String>,
}
