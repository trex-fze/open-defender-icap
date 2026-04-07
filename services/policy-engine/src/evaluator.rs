use crate::{
    models::{DecisionRequest, PolicyCreateRequest, PolicyUpdateRequest},
    store::{insert_policy_document, update_policy_document, PolicyStore, SimulationResult},
};
use anyhow::{anyhow, Result};
use common_types::{PolicyAction, PolicyDecision};
use policy_dsl::PolicyDocument;
use sqlx::PgPool;
use std::sync::Arc;
use taxonomy::ActivationState;
use tracing::warn;
use uuid::Uuid;

#[derive(Clone)]
pub struct PolicyEvaluator {
    store: Arc<PolicyStore>,
    source: PolicySource,
    activation: Arc<ActivationState>,
}

#[derive(Clone)]
enum PolicySource {
    File {
        path: String,
    },
    Database {
        pool: PgPool,
        activation_pool: PgPool,
        seed_path: Option<String>,
    },
}

impl PolicyEvaluator {
    pub fn from_file(
        store: PolicyStore,
        policy_file: String,
        activation: Arc<ActivationState>,
    ) -> Self {
        Self {
            store: Arc::new(store),
            source: PolicySource::File { path: policy_file },
            activation,
        }
    }

    pub fn from_database(
        store: PolicyStore,
        pool: PgPool,
        activation_pool: PgPool,
        seed_path: Option<String>,
        activation: Arc<ActivationState>,
    ) -> Self {
        Self {
            store: Arc::new(store),
            source: PolicySource::Database {
                pool,
                activation_pool,
                seed_path,
            },
            activation,
        }
    }

    pub fn evaluate(&self, request: &DecisionRequest) -> PolicyDecision {
        let mut result = self.store.simulate(request);
        self.apply_activation(&mut result.decision);
        result.decision
    }

    pub fn simulate(&self, request: &DecisionRequest) -> SimulationResult {
        let mut result = self.store.simulate(request);
        self.apply_activation(&mut result.decision);
        result
    }

    pub async fn reload(&self) -> Result<()> {
        let taxonomy = self.store.taxonomy();
        match &self.source {
            PolicySource::File { path } => {
                let doc = PolicyDocument::load_from_file(path)?;
                self.store.update(doc)?;
            }
            PolicySource::Database {
                pool,
                activation_pool,
                seed_path,
            } => {
                if let Some(new_store) =
                    PolicyStore::load_from_db(pool, Arc::clone(&taxonomy)).await?
                {
                    self.store.update_from_rules(
                        new_store.list_rules(),
                        new_store.version(),
                        new_store.policy_id(),
                    )?;
                } else if let Some(seed) = seed_path {
                    let doc = PolicyDocument::load_from_file(seed)?;
                    PolicyStore::seed_db_from_document(pool, &doc, "default", Some("system"))
                        .await?;
                    if let Some(new_store) =
                        PolicyStore::load_from_db(pool, Arc::clone(&taxonomy)).await?
                    {
                        self.store.update_from_rules(
                            new_store.list_rules(),
                            new_store.version(),
                            new_store.policy_id(),
                        )?;
                    }
                }
                self.activation.refresh_from_db(activation_pool).await?;
            }
        }
        Ok(())
    }

    pub async fn create_policy(&self, req: PolicyCreateRequest) -> Result<Uuid> {
        match &self.source {
            PolicySource::File { .. } => Err(anyhow!("database backend not configured")),
            PolicySource::Database { pool, .. } => {
                let doc = PolicyDocument {
                    version: req.version.clone(),
                    rules: req.rules.clone(),
                };
                let policy_id =
                    insert_policy_document(pool, &doc, &req.name, req.created_by.as_deref())
                        .await?;
                self.reload().await?;
                Ok(policy_id)
            }
        }
    }

    pub async fn update_policy(
        &self,
        policy_id: Uuid,
        req: PolicyUpdateRequest,
        actor: &str,
    ) -> Result<()> {
        match &self.source {
            PolicySource::File { .. } => Err(anyhow!("database backend not configured")),
            PolicySource::Database { pool, .. } => {
                update_policy_document(pool, policy_id, &req, Some(actor)).await?;
                self.reload().await?;
                Ok(())
            }
        }
    }

    pub fn rules(&self) -> Vec<policy_dsl::PolicyRule> {
        self.store.list_rules()
    }

    pub fn is_category_enabled(&self, category_id: &str) -> bool {
        self.activation.is_enabled(category_id, None)
    }

    pub fn is_verdict_enabled(&self, category_id: &str, subcategory_id: &str) -> bool {
        self.activation
            .is_enabled(category_id, Some(subcategory_id))
    }

    pub fn version(&self) -> String {
        self.store.version()
    }

    pub fn policy_id(&self) -> Option<Uuid> {
        self.store.policy_id()
    }

    fn apply_activation(&self, decision: &mut PolicyDecision) {
        if let Some(verdict) = decision.verdict.as_ref() {
            if !self
                .activation
                .is_enabled(&verdict.primary_category, Some(&verdict.subcategory))
            {
                warn!(
                    target = "svc-policy",
                    category = %verdict.primary_category,
                    subcategory = %verdict.subcategory,
                    "taxonomy activation blocked policy decision"
                );
                decision.action = PolicyAction::Block;
            }
        }
    }
}
