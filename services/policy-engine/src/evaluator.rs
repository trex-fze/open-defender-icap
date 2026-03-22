use crate::{models::DecisionRequest, store::PolicyStore};
use common_types::PolicyDecision;

#[derive(Clone)]
pub struct PolicyEvaluator {
    store: PolicyStore,
    policy_file: String,
}

impl PolicyEvaluator {
    pub fn new(store: PolicyStore, policy_file: String) -> Self {
        Self { store, policy_file }
    }

    pub fn evaluate(&self, request: &DecisionRequest) -> PolicyDecision {
        self.store.evaluate(request)
    }

    pub fn reload(&self) -> anyhow::Result<()> {
        self.store.reload(&self.policy_file)
    }

    pub fn rules(&self) -> Vec<policy_dsl::PolicyRule> {
        self.store.list_rules()
    }

    pub fn version(&self) -> String {
        self.store.version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DecisionRequest;
    use policy_dsl::{PolicyDocument, PolicyRule};

    fn mock_store() -> PolicyStore {
        let doc = PolicyDocument {
            version: "test".into(),
            rules: vec![PolicyRule {
                id: "block-social".into(),
                description: None,
                priority: 10,
                action: common_types::PolicyAction::Block,
                conditions: policy_dsl::Conditions {
                    categories: Some(vec!["Social Media".into()]),
                    ..Default::default()
                },
            }],
        };
        let path = std::env::temp_dir().join("policy-test.json");
        std::fs::write(&path, serde_json::to_string(&doc).unwrap()).unwrap();
        PolicyStore::load_from_file(path.to_str().unwrap()).unwrap()
    }

    fn base_request() -> DecisionRequest {
        DecisionRequest {
            normalized_key: "domain:example.com".into(),
            entity_level: "domain".into(),
            source_ip: "10.0.0.1".into(),
            user_id: None,
            group_ids: None,
            category_hint: Some("Social Media".into()),
            risk_hint: None,
            confidence_hint: Some(0.8),
        }
    }

    #[test]
    fn matches_policy_rule() {
        let store = mock_store();
        let evaluator = PolicyEvaluator::new(store, "unused".into());
        let decision = evaluator.evaluate(&base_request());
        assert_eq!(decision.action, common_types::PolicyAction::Block);
    }
}
