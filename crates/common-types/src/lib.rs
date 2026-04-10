use serde::{Deserialize, Serialize};
use std::fmt;

pub mod normalizer;

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
    ContentPending,
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
            PolicyAction::ContentPending => "ContentPending",
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecisionRequest {
    pub normalized_key: String,
    pub entity_level: EntityLevel,
    pub source_ip: String,
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageFetchJob {
    pub normalized_key: String,
    pub url: String,
    pub hostname: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<i32>,
}
