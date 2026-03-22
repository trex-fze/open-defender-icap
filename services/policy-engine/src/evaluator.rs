use crate::{
    models::{DecisionRequest, PolicyCreateRequest},
    store::{insert_policy_document, PolicyStore},
};
use anyhow::{anyhow, Result};
use common_types::PolicyDecision;
use policy_dsl::PolicyDocument;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct PolicyEvaluator {
    store: Arc<PolicyStore>,
    source: PolicySource,
}

#[derive(Clone)]
enum PolicySource {
    File {
        path: String,
    },
    Database {
        pool: PgPool,
        seed_path: Option<String>,
    },
}

impl PolicyEvaluator {
    pub fn from_file(store: PolicyStore, policy_file: String) -> Self {
        Self {
            store: Arc::new(store),
            source: PolicySource::File { path: policy_file },
        }
    }

    pub fn from_database(store: PolicyStore, pool: PgPool, seed_path: Option<String>) -> Self {
        Self {
            store: Arc::new(store),
            source: PolicySource::Database { pool, seed_path },
        }
    }

    pub fn evaluate(&self, request: &DecisionRequest) -> PolicyDecision {
        self.store.evaluate(request)
    }

    pub async fn reload(&self) -> Result<()> {
        match &self.source {
            PolicySource::File { path } => {
                let doc = PolicyDocument::load_from_file(path)?;
                self.store.update(doc);
            }
            PolicySource::Database { pool, seed_path } => {
                if let Some(new_store) = PolicyStore::load_from_db(pool).await? {
                    self.store
                        .update_from_rules(new_store.list_rules(), new_store.version());
                } else if let Some(seed) = seed_path {
                    let doc = PolicyDocument::load_from_file(seed)?;
                    PolicyStore::seed_db_from_document(pool, &doc, "default", Some("system"))
                        .await?;
                    if let Some(new_store) = PolicyStore::load_from_db(pool).await? {
                        self.store
                            .update_from_rules(new_store.list_rules(), new_store.version());
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn create_policy(&self, req: PolicyCreateRequest) -> Result<()> {
        match &self.source {
            PolicySource::File { .. } => Err(anyhow!("database backend not configured")),
            PolicySource::Database { pool, .. } => {
                let doc = PolicyDocument {
                    version: req.version.clone(),
                    rules: req.rules.clone(),
                };
                insert_policy_document(pool, &doc, &req.name, req.created_by.as_deref()).await?;
                self.reload().await?;
                Ok(())
            }
        }
    }

    pub fn rules(&self) -> Vec<policy_dsl::PolicyRule> {
        self.store.list_rules()
    }

    pub fn version(&self) -> String {
        self.store.version()
    }
}
