use anyhow::{Context, Result};
use common_types::PolicyAction;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PolicyDocument {
    pub version: String,
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PolicyRule {
    pub id: String,
    pub description: Option<String>,
    pub priority: u32,
    pub action: PolicyAction,
    #[serde(default)]
    pub conditions: Conditions,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    pub domains: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub users: Option<Vec<String>>,
    pub groups: Option<Vec<String>>,
    pub source_ips: Option<Vec<String>>,
    pub risk_levels: Option<Vec<String>>,
}

impl PolicyDocument {
    pub fn load_from_file(path: &str) -> Result<Self> {
        let data = fs::read_to_string(path).with_context(|| format!("failed to read {path}"))?;
        let doc: PolicyDocument = serde_json::from_str(&data)?;
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_policy_document() {
        let data = json!({
            "version": "v1",
            "rules": [
                {
                    "id": "block-social",
                    "priority": 10,
                    "action": "Block",
                    "conditions": {
                        "categories": ["Social Media"]
                    }
                }
            ]
        });
        let doc: PolicyDocument = serde_json::from_value(data).unwrap();
        assert_eq!(doc.rules.len(), 1);
        assert_eq!(doc.rules[0].action, PolicyAction::Block);
    }

    #[test]
    fn rejects_unknown_condition_keys() {
        let data = json!({
            "version": "v1",
            "rules": [
                {
                    "id": "bad-rule",
                    "priority": 10,
                    "action": "Block",
                    "conditions": {
                        "user_ids": ["alice"]
                    }
                }
            ]
        });
        let parsed: Result<PolicyDocument, _> = serde_json::from_value(data);
        assert!(parsed.is_err());
    }
}
