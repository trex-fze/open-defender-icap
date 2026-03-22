use common_types::{ClassificationVerdict, PolicyAction, PolicyDecision};

use crate::models::DecisionRequest;

#[derive(Debug, Clone, Default)]
pub struct PolicyEvaluator;

impl PolicyEvaluator {
    pub fn evaluate(&self, request: &DecisionRequest) -> PolicyDecision {
        let action = if request.normalized_key.contains("block") {
            PolicyAction::Block
        } else if request.risk_hint.as_deref() == Some("high") {
            PolicyAction::Review
        } else {
            PolicyAction::Allow
        };

        PolicyDecision {
            action: action.clone(),
            cache_hit: false,
            verdict: Some(ClassificationVerdict {
                primary_category: request
                    .category_hint
                    .clone()
                    .unwrap_or_else(|| "Unknown".into()),
                subcategory: "Unspecified".into(),
                risk_level: request.risk_hint.clone().unwrap_or_else(|| "medium".into()),
                confidence: request.confidence_hint.unwrap_or(0.5),
                recommended_action: action,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DecisionRequest;

    fn base_request() -> DecisionRequest {
        DecisionRequest {
            normalized_key: "domain:example.com".into(),
            entity_level: "domain".into(),
            source_ip: "10.0.0.1".into(),
            user_id: None,
            category_hint: None,
            risk_hint: None,
            confidence_hint: None,
        }
    }

    #[test]
    fn allow_default() {
        let evaluator = PolicyEvaluator::default();
        let decision = evaluator.evaluate(&base_request());
        assert_eq!(decision.action, PolicyAction::Allow);
    }

    #[test]
    fn block_contains_keyword() {
        let evaluator = PolicyEvaluator::default();
        let mut req = base_request();
        req.normalized_key = "domain:blocked_site.com".into();
        let decision = evaluator.evaluate(&req);
        assert_eq!(decision.action, PolicyAction::Block);
    }

    #[test]
    fn review_on_high_risk_hint() {
        let evaluator = PolicyEvaluator::default();
        let mut req = base_request();
        req.risk_hint = Some("high".into());
        let decision = evaluator.evaluate(&req);
        assert_eq!(decision.action, PolicyAction::Review);
    }
}
