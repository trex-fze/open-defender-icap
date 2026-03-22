use crate::models::DecisionRequest;
use anyhow::{anyhow, Result};
use common_types::{ClassificationVerdict, PolicyAction, PolicyDecision};
use parking_lot::RwLock;
use policy_dsl::{Conditions, PolicyDocument, PolicyRule};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct PolicyStore {
    inner: Arc<RwLock<Vec<PolicyRule>>>,
    version: Arc<RwLock<String>>,
    policy_id: Arc<RwLock<Option<Uuid>>>,
}

#[derive(Clone)]
pub struct SimulationResult {
    pub decision: PolicyDecision,
    pub matched_rule_id: Option<String>,
}

impl PolicyStore {
    pub fn from_document(doc: PolicyDocument) -> Self {
        let mut rules = doc.rules;
        rules.sort_by_key(|r| r.priority);
        Self {
            inner: Arc::new(RwLock::new(rules)),
            version: Arc::new(RwLock::new(doc.version)),
            policy_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn load_from_file(path: &str) -> Result<Self> {
        let doc = PolicyDocument::load_from_file(path)?;
        Ok(Self::from_document(doc))
    }

    pub async fn load_from_db(pool: &PgPool) -> Result<Option<Self>> {
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

        Ok(Some(Self {
            inner: Arc::new(RwLock::new(parsed_rules)),
            version: Arc::new(RwLock::new(version)),
            policy_id: Arc::new(RwLock::new(Some(policy_id))),
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

    pub fn update(&self, doc: PolicyDocument) {
        let mut rules = doc.rules;
        rules.sort_by_key(|r| r.priority);
        *self.inner.write() = rules;
        *self.version.write() = doc.version;
        *self.policy_id.write() = None;
    }

    pub fn update_from_rules(
        &self,
        rules: Vec<PolicyRule>,
        version: String,
        policy_id: Option<Uuid>,
    ) {
        *self.inner.write() = rules;
        *self.version.write() = version;
        *self.policy_id.write() = policy_id;
    }

    pub fn evaluate(&self, request: &DecisionRequest) -> PolicyDecision {
        self.simulate(request).decision
    }

    pub fn simulate(&self, request: &DecisionRequest) -> SimulationResult {
        let rules = self.inner.read();
        for rule in rules.iter() {
            if matches_conditions(&rule.conditions, request) {
                return SimulationResult {
                    decision: PolicyDecision {
                        action: rule.action.clone(),
                        cache_hit: false,
                        verdict: request.to_verdict(rule),
                    },
                    matched_rule_id: Some(rule.id.clone()),
                };
            }
        }
        SimulationResult {
            decision: PolicyDecision {
                action: PolicyAction::Allow,
                cache_hit: false,
                verdict: request.to_verdict_default(),
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
    fn to_verdict(&self, rule: &PolicyRule) -> Option<ClassificationVerdict>;
    fn to_verdict_default(&self) -> Option<ClassificationVerdict>;
}

impl ToVerdict for DecisionRequest {
    fn to_verdict(&self, rule: &PolicyRule) -> Option<ClassificationVerdict> {
        Some(ClassificationVerdict {
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

    fn to_verdict_default(&self) -> Option<ClassificationVerdict> {
        Some(ClassificationVerdict {
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
