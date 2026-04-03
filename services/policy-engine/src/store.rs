use crate::models::DecisionRequest;
use anyhow::{anyhow, Result};
use common_types::{ClassificationVerdict, PolicyAction, PolicyDecision};
use parking_lot::RwLock;
use policy_dsl::{Conditions, PolicyDocument, PolicyRule};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use taxonomy::{FallbackReason, TaxonomyStore, UNKNOWN_CATEGORY_ID};
use tracing::warn;
use uuid::Uuid;

#[derive(Clone)]
pub struct PolicyStore {
    inner: Arc<RwLock<Vec<PolicyRule>>>,
    version: Arc<RwLock<String>>,
    policy_id: Arc<RwLock<Option<Uuid>>>,
    taxonomy: Arc<TaxonomyStore>,
}

#[derive(Clone)]
pub struct SimulationResult {
    pub decision: PolicyDecision,
    pub matched_rule_id: Option<String>,
}

impl PolicyStore {
    pub fn from_document(doc: PolicyDocument, taxonomy: Arc<TaxonomyStore>) -> Result<Self> {
        let mut rules = doc.rules;
        canonicalize_rules(&taxonomy, &mut rules)?;
        rules.sort_by_key(|r| r.priority);
        Ok(Self {
            inner: Arc::new(RwLock::new(rules)),
            version: Arc::new(RwLock::new(doc.version)),
            policy_id: Arc::new(RwLock::new(None)),
            taxonomy,
        })
    }

    pub fn load_from_file(path: &str, taxonomy: Arc<TaxonomyStore>) -> Result<Self> {
        let doc = PolicyDocument::load_from_file(path)?;
        Self::from_document(doc, taxonomy)
    }

    pub async fn load_from_db(pool: &PgPool, taxonomy: Arc<TaxonomyStore>) -> Result<Option<Self>> {
        let policy = sqlx::query(
            "SELECT id, name, version FROM policies WHERE status = 'active' ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;

        let Some(policy) = policy else {
            return Ok(None);
        };

        let policy_id: Uuid = policy.get("id");
        let version: String = policy.get("version");

        let rules = sqlx::query(
            "SELECT priority, action, description, conditions FROM policy_rules WHERE policy_id = $1 ORDER BY priority ASC",
        )
        .bind(policy_id)
        .fetch_all(pool)
        .await?;

        let mut parsed_rules = Vec::with_capacity(rules.len());
        for row in rules {
            let priority: i32 = row.get("priority");
            let action: String = row.get("action");
            let description: Option<String> = row.get("description");
            let conditions: Value = row.get("conditions");
            parsed_rules.push(db_row_to_rule(&action, description, priority, conditions)?);
        }
        canonicalize_rules(&taxonomy, &mut parsed_rules)?;

        Ok(Some(Self {
            inner: Arc::new(RwLock::new(parsed_rules)),
            version: Arc::new(RwLock::new(version)),
            policy_id: Arc::new(RwLock::new(Some(policy_id))),
            taxonomy,
        }))
    }

    pub async fn seed_db_from_document(
        pool: &PgPool,
        doc: &PolicyDocument,
        name: &str,
        created_by: Option<&str>,
    ) -> Result<Uuid> {
        insert_policy_document(pool, doc, name, created_by).await
    }

    pub fn update(&self, doc: PolicyDocument) -> Result<()> {
        let mut rules = doc.rules;
        canonicalize_rules(&self.taxonomy, &mut rules)?;
        rules.sort_by_key(|r| r.priority);
        *self.inner.write() = rules;
        *self.version.write() = doc.version;
        *self.policy_id.write() = None;
        Ok(())
    }

    pub fn update_from_rules(
        &self,
        mut rules: Vec<PolicyRule>,
        version: String,
        policy_id: Option<Uuid>,
    ) -> Result<()> {
        canonicalize_rules(&self.taxonomy, &mut rules)?;
        *self.inner.write() = rules;
        *self.version.write() = version;
        *self.policy_id.write() = policy_id;
        Ok(())
    }

    pub fn simulate(&self, request: &DecisionRequest) -> SimulationResult {
        let canonical_category = request.category_hint.as_deref().and_then(|hint| {
            self.canonicalize_request_category(hint, request.subcategory_hint.as_deref())
        });
        let rules = self.inner.read();
        for rule in rules.iter() {
            if self.matches_conditions(&rule.conditions, canonical_category.as_deref(), request) {
                return SimulationResult {
                    decision: PolicyDecision {
                        action: rule.action.clone(),
                        cache_hit: false,
                        verdict: request.to_verdict(rule, canonical_category.as_deref()),
                        decision_source: None,
                    },
                    matched_rule_id: Some(rule.id.clone()),
                };
            }
        }
        SimulationResult {
            decision: PolicyDecision {
                action: PolicyAction::Allow,
                cache_hit: false,
                verdict: request.to_verdict_default(canonical_category.as_deref()),
                decision_source: None,
            },
            matched_rule_id: None,
        }
    }

    pub fn list_rules(&self) -> Vec<PolicyRule> {
        self.inner.read().clone()
    }

    pub fn version(&self) -> String {
        self.version.read().clone()
    }

    pub fn policy_id(&self) -> Option<Uuid> {
        *self.policy_id.read()
    }

    pub fn taxonomy(&self) -> Arc<TaxonomyStore> {
        Arc::clone(&self.taxonomy)
    }

    fn canonicalize_request_category(
        &self,
        hint: &str,
        subcategory_hint: Option<&str>,
    ) -> Option<String> {
        let validated = self.taxonomy.validate_labels(hint, subcategory_hint);
        if let Some(reason) = validated.fallback_reason {
            warn!(
                target = "svc-policy",
                reason = reason.as_str(),
                hint,
                subcategory_hint,
                "category hint normalized via taxonomy"
            );
        }
        Some(validated.category.id.clone())
    }

    fn matches_conditions(
        &self,
        cond: &Conditions,
        canonical_category: Option<&str>,
        request: &DecisionRequest,
    ) -> bool {
        if let Some(domains) = &cond.domains {
            if !domains.iter().any(|d| request.normalized_key.contains(d)) {
                return false;
            }
        }
        if let Some(categories) = &cond.categories {
            match canonical_category {
                Some(category) if categories.iter().any(|c| c == category) => {}
                _ => return false,
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
}

pub async fn insert_policy_document(
    pool: &PgPool,
    doc: &PolicyDocument,
    name: &str,
    created_by: Option<&str>,
) -> Result<Uuid> {
    let policy_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO policies (id, name, version, status, created_by) VALUES ($1, $2, $3, 'active', $4)",
    )
    .bind(policy_id)
    .bind(name)
    .bind(&doc.version)
    .bind(created_by)
    .execute(pool)
    .await?;

    for rule in &doc.rules {
        sqlx::query(
            "INSERT INTO policy_rules (id, policy_id, priority, action, description, conditions) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(policy_id)
        .bind(rule.priority as i32)
        .bind(rule.action.to_string())
        .bind(&rule.description)
        .bind(serde_json::to_value(&rule.conditions)?)
        .execute(pool)
        .await?;
    }

    record_policy_version(
        pool,
        policy_id,
        &doc.version,
        "active",
        &doc.rules,
        created_by,
        None,
    )
    .await?;

    Ok(policy_id)
}

fn canonicalize_rules(taxonomy: &TaxonomyStore, rules: &mut [PolicyRule]) -> Result<()> {
    for rule in rules.iter_mut() {
        if let Some(categories) = rule.conditions.categories.as_mut() {
            for category in categories.iter_mut() {
                let validated = taxonomy.validate_labels(category, None);
                if let Some(reason) = validated.fallback_reason {
                    if matches!(
                        reason,
                        FallbackReason::MissingCategory | FallbackReason::UnknownCategory
                    ) {
                        return Err(anyhow!(
                            "policy rule '{}' references invalid category '{}': {}",
                            rule.id,
                            category,
                            reason.as_str()
                        ));
                    }
                }
                *category = validated.category.id.clone();
            }
        }
    }
    Ok(())
}

fn db_row_to_rule(
    action: &str,
    description: Option<String>,
    priority: i32,
    conditions: Value,
) -> Result<PolicyRule> {
    let action_enum = match action {
        "Allow" => PolicyAction::Allow,
        "Block" => PolicyAction::Block,
        "Warn" => PolicyAction::Warn,
        "Monitor" => PolicyAction::Monitor,
        "Review" => PolicyAction::Review,
        "RequireApproval" => PolicyAction::RequireApproval,
        "ContentPending" => PolicyAction::ContentPending,
        other => return Err(anyhow!("unknown action {other}")),
    };

    let cond: Conditions = serde_json::from_value(conditions)?;
    Ok(PolicyRule {
        id: Uuid::new_v4().to_string(),
        description,
        priority: priority as u32,
        action: action_enum,
        conditions: cond,
    })
}

async fn record_policy_version(
    pool: &PgPool,
    policy_id: Uuid,
    version: &str,
    status: &str,
    rules: &[PolicyRule],
    created_by: Option<&str>,
    notes: Option<&str>,
) -> Result<()> {
    let payload = serde_json::to_value(rules)?;
    sqlx::query(
        r#"INSERT INTO policy_versions (id, policy_id, version, status, created_by, notes, rules)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(policy_id)
    .bind(version)
    .bind(status)
    .bind(created_by)
    .bind(notes)
    .bind(payload)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_policy_document(
    pool: &PgPool,
    policy_id: Uuid,
    req: &crate::models::PolicyUpdateRequest,
    actor: Option<&str>,
) -> Result<()> {
    let record = sqlx::query("SELECT version, status FROM policies WHERE id = $1")
        .bind(policy_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow!("policy not found"))?;

    let mut version: String = record.get("version");
    let mut status: String = record.get("status");
    if let Some(v) = &req.version {
        version = v.clone();
    }
    if let Some(s) = &req.status {
        status = s.clone();
    }

    if req.version.is_some() || req.status.is_some() {
        sqlx::query("UPDATE policies SET version = $1, status = $2 WHERE id = $3")
            .bind(&version)
            .bind(&status)
            .bind(policy_id)
            .execute(pool)
            .await?;
    }

    let rules = if let Some(rules) = &req.rules {
        sqlx::query("DELETE FROM policy_rules WHERE policy_id = $1")
            .bind(policy_id)
            .execute(pool)
            .await?;
        for rule in rules {
            sqlx::query(
                "INSERT INTO policy_rules (id, policy_id, priority, action, description, conditions) VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(Uuid::new_v4())
            .bind(policy_id)
            .bind(rule.priority as i32)
            .bind(rule.action.to_string())
            .bind(&rule.description)
            .bind(serde_json::to_value(&rule.conditions)?)
            .execute(pool)
            .await?;
        }
        rules.clone()
    } else {
        fetch_policy_rules(pool, policy_id).await?
    };

    record_policy_version(
        pool,
        policy_id,
        &version,
        &status,
        &rules,
        actor,
        req.notes.as_deref(),
    )
    .await?;

    Ok(())
}

async fn fetch_policy_rules(pool: &PgPool, policy_id: Uuid) -> Result<Vec<PolicyRule>> {
    let rows = sqlx::query(
        "SELECT priority, action, description, conditions FROM policy_rules WHERE policy_id = $1 ORDER BY priority ASC",
    )
    .bind(policy_id)
    .fetch_all(pool)
    .await?;

    let mut parsed = Vec::with_capacity(rows.len());
    for row in rows {
        let priority: i32 = row.get("priority");
        let action: String = row.get("action");
        let description: Option<String> = row.get("description");
        let conditions: Value = row.get("conditions");
        parsed.push(db_row_to_rule(&action, description, priority, conditions)?);
    }
    Ok(parsed)
}

trait ToVerdict {
    fn to_verdict(
        &self,
        rule: &PolicyRule,
        canonical_category: Option<&str>,
    ) -> Option<ClassificationVerdict>;
    fn to_verdict_default(&self, canonical_category: Option<&str>)
        -> Option<ClassificationVerdict>;
}

impl ToVerdict for DecisionRequest {
    fn to_verdict(
        &self,
        rule: &PolicyRule,
        canonical_category: Option<&str>,
    ) -> Option<ClassificationVerdict> {
        let category = canonical_category
            .unwrap_or(UNKNOWN_CATEGORY_ID)
            .to_string();
        Some(ClassificationVerdict {
            primary_category: category,
            subcategory: rule.description.clone().unwrap_or_else(|| "Rule".into()),
            risk_level: self.risk_hint.clone().unwrap_or_else(|| "medium".into()),
            confidence: self.confidence_hint.unwrap_or(0.5),
            recommended_action: rule.action.clone(),
        })
    }

    fn to_verdict_default(
        &self,
        canonical_category: Option<&str>,
    ) -> Option<ClassificationVerdict> {
        let category = canonical_category
            .unwrap_or(UNKNOWN_CATEGORY_ID)
            .to_string();
        Some(ClassificationVerdict {
            primary_category: category,
            subcategory: "Default".into(),
            risk_level: self.risk_hint.clone().unwrap_or_else(|| "low".into()),
            confidence: self.confidence_hint.unwrap_or(0.5),
            recommended_action: PolicyAction::Allow,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> DecisionRequest {
        DecisionRequest {
            normalized_key: "domain:example.com".into(),
            entity_level: "domain".into(),
            source_ip: "192.0.2.10".into(),
            user_id: Some("alice@example.com".into()),
            group_ids: Some(vec!["global-admins".into()]),
            category_hint: None,
            subcategory_hint: None,
            risk_hint: None,
            confidence_hint: None,
        }
    }

    #[test]
    fn matches_user_condition() {
        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let rule = PolicyRule {
            id: "test-user".into(),
            description: Some("User match".into()),
            priority: 10,
            action: PolicyAction::Block,
            conditions: Conditions {
                users: Some(vec!["alice@example.com".into()]),
                ..Default::default()
            },
        };
        let store = PolicyStore::from_document(
            PolicyDocument {
                version: "v1".into(),
                rules: vec![rule],
            },
            taxonomy,
        )
        .unwrap();

        let mut request = base_request();
        let result = store.simulate(&request);
        assert_eq!(result.decision.action, PolicyAction::Block);

        request.user_id = Some("bob@example.com".into());
        let result = store.simulate(&request);
        assert_eq!(result.decision.action, PolicyAction::Allow);
    }

    #[test]
    fn canonicalizes_rule_categories() {
        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let rule = PolicyRule {
            id: "social-block".into(),
            description: Some("Block social".into()),
            priority: 5,
            action: PolicyAction::Block,
            conditions: Conditions {
                categories: Some(vec!["Social".into()]),
                ..Default::default()
            },
        };
        let store = PolicyStore::from_document(
            PolicyDocument {
                version: "v1".into(),
                rules: vec![rule],
            },
            taxonomy,
        )
        .unwrap();

        let stored_rules = store.list_rules();
        assert_eq!(stored_rules.len(), 1);
        let categories = stored_rules[0]
            .conditions
            .categories
            .clone()
            .expect("categories present");
        assert_eq!(categories, vec![String::from("unknown-unclassified")]);
    }

    #[test]
    fn rejects_invalid_category_in_rule() {
        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let rule = PolicyRule {
            id: "invalid".into(),
            description: None,
            priority: 1,
            action: PolicyAction::Block,
            conditions: Conditions {
                categories: Some(vec!["NotARealCategory".into()]),
                ..Default::default()
            },
        };
        let result = PolicyStore::from_document(
            PolicyDocument {
                version: "v1".into(),
                rules: vec![rule],
            },
            taxonomy,
        );
        assert!(result.is_err());
    }

    #[test]
    fn matches_group_condition() {
        let taxonomy = Arc::new(TaxonomyStore::load_default().unwrap());
        let rule = PolicyRule {
            id: "test-group".into(),
            description: Some("Group match".into()),
            priority: 10,
            action: PolicyAction::Block,
            conditions: Conditions {
                groups: Some(vec!["global-admins".into()]),
                ..Default::default()
            },
        };
        let store = PolicyStore::from_document(
            PolicyDocument {
                version: "v1".into(),
                rules: vec![rule],
            },
            taxonomy,
        )
        .unwrap();

        let request = base_request();
        assert_eq!(
            store.simulate(&request).decision.action,
            PolicyAction::Block
        );

        let mut other = base_request();
        other.group_ids = Some(vec!["finance".into()]);
        assert_eq!(store.simulate(&other).decision.action, PolicyAction::Allow);
    }
}
