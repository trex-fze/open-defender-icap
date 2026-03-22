use anyhow::Result;
use common_types::PolicyDecision;
use redis::AsyncCommands;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct CacheClient {
    memory: Arc<RwLock<HashMap<String, CacheEntry>>>,
    redis: Option<redis::Client>,
}

#[derive(Clone)]
struct CacheEntry {
    decision: PolicyDecision,
    expires_at: tokio::time::Instant,
}

impl CacheClient {
    pub fn new(redis_url: Option<String>) -> Result<Self> {
        let redis = if let Some(url) = redis_url {
            Some(redis::Client::open(url)?)
        } else {
            None
        };
        Ok(Self {
            memory: Arc::new(RwLock::new(HashMap::new())),
            redis,
        })
    }

    pub async fn get(&self, key: &str) -> Result<Option<PolicyDecision>> {
        if let Some(decision) = self.memory_get(key).await {
            return Ok(Some(decision));
        }

        if let Some(client) = &self.redis {
            let mut conn = client.get_async_connection().await?;
            let payload: Option<String> = conn.get(key).await?;
            if let Some(payload) = payload {
                if let Ok(decision) = serde_json::from_str::<PolicyDecision>(&payload) {
                    return Ok(Some(decision));
                }
            }
        }

        Ok(None)
    }

    pub async fn set(&self, key: String, decision: PolicyDecision, ttl: Duration) -> Result<()> {
        self.memory_set(key.clone(), decision.clone(), ttl).await;

        if let Some(client) = &self.redis {
            let mut conn = client.get_async_connection().await?;
            let payload = serde_json::to_string(&decision)?;
            let ttl_secs = ttl.as_secs().max(1);
            conn.set_ex(key, payload, ttl_secs).await?;
        }

        Ok(())
    }

    async fn memory_get(&self, key: &str) -> Option<PolicyDecision> {
        let store = self.memory.read().await;
        store.get(key).and_then(|entry| {
            if tokio::time::Instant::now() <= entry.expires_at {
                Some(entry.decision.clone())
            } else {
                None
            }
        })
    }

    async fn memory_set(&self, key: String, decision: PolicyDecision, ttl: Duration) {
        let mut store = self.memory.write().await;
        store.insert(
            key,
            CacheEntry {
                decision,
                expires_at: tokio::time::Instant::now() + ttl,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_types::{ClassificationVerdict, PolicyAction};

    fn sample_decision(action: PolicyAction) -> PolicyDecision {
        PolicyDecision {
            action: action.clone(),
            cache_hit: action == PolicyAction::Allow,
            verdict: Some(ClassificationVerdict {
                primary_category: "Test".into(),
                subcategory: "Test".into(),
                risk_level: "low".into(),
                confidence: 0.9,
                recommended_action: action,
            }),
        }
    }

    #[tokio::test]
    async fn in_memory_cache_flow() {
        let cache = CacheClient::new(None).unwrap();
        assert!(cache.get("key").await.unwrap().is_none());
        cache
            .set(
                "key".into(),
                sample_decision(PolicyAction::Allow),
                Duration::from_secs(10),
            )
            .await
            .unwrap();
        let value = cache.get("key").await.unwrap().unwrap();
        assert_eq!(value.action, PolicyAction::Allow);
    }
}
