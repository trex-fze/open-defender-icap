use crate::models::DecisionRequest;
use common_types::{PolicyAction, PolicyDecision};
use parking_lot::RwLock;
use policy_dsl::{Conditions, PolicyDocument, PolicyRule};
use std::sync::Arc;

#[derive(Clone)]
pub struct PolicyStore {
    inner: Arc<RwLock<Vec<PolicyRule>>>,
    version: Arc<RwLock<String>>,
}

impl PolicyStore {
    pub fn load_from_file(path: &str) -> anyhow::Result<Self> {
        let doc = PolicyDocument::load_from_file(path)?;
        let mut rules = doc.rules;
        rules.sort_by_key(|r| r.priority);
        Ok(Self {
            inner: Arc::new(RwLock::new(rules)),
            version: Arc::new(RwLock::new(doc.version)),
        })
    }

    pub fn evaluate(&self, request: &DecisionRequest) -> PolicyDecision {
        let rules = self.inner.read();
        for rule in rules.iter() {
            if matches_conditions(&rule.conditions, request) {
                return PolicyDecision {
                    action: rule.action.clone(),
                    cache_hit: false,
                    verdict: request.to_verdict(rule),
                };
            }
        }
        PolicyDecision {
            action: PolicyAction::Allow,
            cache_hit: false,
            verdict: request.to_verdict_default(),
        }
    }

    pub fn list_rules(&self) -> Vec<PolicyRule> {
        self.inner.read().clone()
    }

    pub fn version(&self) -> String {
        self.version.read().clone()
    }

    pub fn reload(&self, path: &str) -> anyhow::Result<()> {
        let doc = PolicyDocument::load_from_file(path)?;
        let mut rules = doc.rules;
        rules.sort_by_key(|r| r.priority);
        *self.inner.write() = rules;
        *self.version.write() = doc.version;
        Ok(())
    }
}

fn matches_conditions(cond: &Conditions, request: &DecisionRequest) -> bool {
    if let Some(domains) = &cond.domains {
        if !domains.iter().any(|d| request.normalized_key.contains(d)) {
            return false;
        }
    }
    if let Some(categories) = &cond.categories {
        if let Some(hint) = &request.category_hint {
            if !categories.iter().any(|c| c.eq_ignore_ascii_case(hint)) {
                return false;
            }
        } else {
            return false;
        }
    }
    if let Some(users) = &cond.users {
        match &request.user_id {
            Some(user) if users.iter().any(|u| u.eq_ignore_ascii_case(user)) => {}
            _ => return false,
        }
    }
    if let Some(groups) = &cond.groups {
        match &request.group_ids {
            Some(req_groups) if req_groups.iter().any(|g| groups.contains(g)) => {}
            _ => return false,
        }
    }
    if let Some(ips) = &cond.source_ips {
        if !ips.iter().any(|ip| ip == &request.source_ip) {
            return false;
        }
    }
    if let Some(risk_levels) = &cond.risk_levels {
        match &request.risk_hint {
            Some(risk) if risk_levels.iter().any(|rl| rl.eq_ignore_ascii_case(risk)) => {}
            _ => return false,
        }
    }
    true
}

trait ToVerdict {
    fn to_verdict(&self, rule: &PolicyRule) -> Option<common_types::ClassificationVerdict>;
    fn to_verdict_default(&self) -> Option<common_types::ClassificationVerdict>;
}

impl ToVerdict for DecisionRequest {
    fn to_verdict(&self, rule: &PolicyRule) -> Option<common_types::ClassificationVerdict> {
        Some(common_types::ClassificationVerdict {
            primary_category: self
                .category_hint
                .clone()
                .unwrap_or_else(|| "Unknown".into()),
            subcategory: rule.description.clone().unwrap_or_else(|| "Rule".into()),
            risk_level: self.risk_hint.clone().unwrap_or_else(|| "medium".into()),
            confidence: self.confidence_hint.unwrap_or(0.5),
            recommended_action: rule.action.clone(),
        })
    }

    fn to_verdict_default(&self) -> Option<common_types::ClassificationVerdict> {
        Some(common_types::ClassificationVerdict {
            primary_category: self
                .category_hint
                .clone()
                .unwrap_or_else(|| "Unknown".into()),
            subcategory: "Default".into(),
            risk_level: self.risk_hint.clone().unwrap_or_else(|| "low".into()),
            confidence: self.confidence_hint.unwrap_or(0.5),
            recommended_action: PolicyAction::Allow,
        })
    }
}
