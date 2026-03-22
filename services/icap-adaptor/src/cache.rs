use anyhow::Result;
use common_types::PolicyDecision;
use futures::StreamExt;
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{sync::RwLock, time::sleep};
use tracing::{debug, info, warn};

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
    pub fn new(redis_url: Option<String>, channel: String) -> Result<Self> {
        let redis = if let Some(url) = redis_url {
            Some(redis::Client::open(url)?)
        } else {
            None
        };
        let client = Self {
            memory: Arc::new(RwLock::new(HashMap::new())),
            redis: redis.clone(),
        };

        if let Some(redis_client) = redis {
            let memory = Arc::clone(&client.memory);
            tokio::spawn(async move {
                if let Err(err) = Self::listen_for_invalidation(redis_client, memory, channel).await
                {
                    warn!(target = "svc-icap", %err, "cache invalidation subscriber exited");
                }
            });
        }

        Ok(client)
    }

    pub async fn get(&self, key: &str) -> Result<Option<PolicyDecision>> {
        if let Some(decision) = self.memory_get(key).await {
            return Ok(Some(decision));
        }

        if let Some(payload) = self.redis_get(key).await? {
            if let Ok(decision) = serde_json::from_str::<PolicyDecision>(&payload) {
                return Ok(Some(decision));
            }
        }

        Ok(None)
    }

    pub async fn set(&self, key: String, decision: PolicyDecision, ttl: Duration) -> Result<()> {
        self.memory_set(key.clone(), decision.clone(), ttl).await;

        if self.redis.is_some() {
            let payload = serde_json::to_string(&decision)?;
            self.redis_set(&key, &payload, ttl.as_secs().max(1)).await?;
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

    async fn redis_get(&self, key: &str) -> Result<Option<String>> {
        let client = match &self.redis {
            Some(client) => client,
            None => return Ok(None),
        };

        for attempt in 0..3 {
            match client.get_async_connection().await {
                Ok(mut conn) => match conn.get(key).await {
                    Ok(value) => return Ok(value),
                    Err(err) => warn!(target = "svc-icap", %err, attempt, "redis GET failed"),
                },
                Err(err) => warn!(target = "svc-icap", %err, attempt, "redis connection failed"),
            }
            sleep(Duration::from_millis(50 * (attempt + 1))).await;
        }

        Ok(None)
    }

    async fn redis_set(&self, key: &str, payload: &str, ttl_secs: u64) -> Result<()> {
        let client = match &self.redis {
            Some(client) => client,
            None => return Ok(()),
        };

        for attempt in 0..3 {
            match client.get_async_connection().await {
                Ok(mut conn) => match conn.set_ex::<&str, &str, ()>(key, payload, ttl_secs).await {
                    Ok(_) => {
                        debug!(target = "svc-icap", key, ttl_secs, "redis cache updated");
                        return Ok(());
                    }
                    Err(err) => warn!(target = "svc-icap", %err, attempt, "redis SET failed"),
                },
                Err(err) => warn!(target = "svc-icap", %err, attempt, "redis connection failed"),
            }
            sleep(Duration::from_millis(50 * (attempt + 1))).await;
        }

        Ok(())
    }

    async fn listen_for_invalidation(
        redis: redis::Client,
        memory: Arc<RwLock<HashMap<String, CacheEntry>>>,
        channel: String,
    ) -> Result<()> {
        loop {
            match redis.get_async_connection().await {
                Ok(conn) => {
                    let mut pubsub = conn.into_pubsub();
                    if let Err(err) = pubsub.subscribe(&channel).await {
                        warn!(target = "svc-icap", %err, channel, "failed to subscribe to cache invalidation channel");
                        sleep(Duration::from_millis(1500)).await;
                        continue;
                    }

                    if let Err(err) = Self::consume_messages(&mut pubsub, &memory).await {
                        warn!(target = "svc-icap", %err, "cache invalidation listener error");
                        sleep(Duration::from_millis(1000)).await;
                    }
                }
                Err(err) => {
                    warn!(target = "svc-icap", %err, "cache invalidation redis connection failed");
                    sleep(Duration::from_millis(1500)).await;
                }
            }
        }
        #[allow(unreachable_code)]
        Ok(())
    }

    async fn consume_messages(
        pubsub: &mut redis::aio::PubSub,
        memory: &Arc<RwLock<HashMap<String, CacheEntry>>>,
    ) -> redis::RedisResult<()> {
        let mut stream = pubsub.on_message();
        while let Some(msg) = stream.next().await {
            let payload: String = msg.get_payload()?;
            if let Err(err) = Self::apply_invalidation(memory, &payload).await {
                warn!(target = "svc-icap", %err, "cache invalidation payload failed");
            }
        }
        Ok(())
    }

    async fn apply_invalidation(
        memory: &Arc<RwLock<HashMap<String, CacheEntry>>>,
        payload: &str,
    ) -> Result<()> {
        let event: CacheInvalidationEvent = serde_json::from_str(payload)?;
        match event {
            CacheInvalidationEvent::Override {
                scope_type,
                scope_value,
            } => Self::invalidate_scope(memory, &scope_type, &scope_value).await,
            CacheInvalidationEvent::Review { normalized_key } => {
                Self::invalidate_key(memory, &normalized_key).await
            }
        }
    }

    async fn invalidate_scope(
        memory: &Arc<RwLock<HashMap<String, CacheEntry>>>,
        scope_type: &str,
        scope_value: &str,
    ) -> Result<()> {
        let mut store = memory.write().await;
        if scope_type != "domain" {
            let removed = store.len();
            store.clear();
            info!(
                target = "svc-icap",
                removed, scope_type, "cleared cache for non-domain scope override"
            );
            return Ok(());
        }

        let (wildcard, value) = match scope_value.strip_prefix("*.") {
            Some(rest) => (true, rest),
            None => (false, scope_value),
        };

        let before = store.len();
        store.retain(|key, _| !key_matches_domain_scope(key, value, wildcard));
        let removed = before.saturating_sub(store.len());
        info!(
            target = "svc-icap",
            removed,
            scope = scope_value,
            "invalidated domain scope cache entries"
        );
        Ok(())
    }

    async fn invalidate_key(
        memory: &Arc<RwLock<HashMap<String, CacheEntry>>>,
        normalized_key: &str,
    ) -> Result<()> {
        let mut store = memory.write().await;
        if store.remove(normalized_key).is_some() {
            info!(
                target = "svc-icap",
                normalized_key, "invalidated cache entry"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum CacheInvalidationEvent {
    Override {
        scope_type: String,
        scope_value: String,
    },
    Review {
        normalized_key: String,
    },
}

fn key_matches_domain_scope(key: &str, scope_value: &str, wildcard: bool) -> bool {
    if let Some(host) = key.strip_prefix("domain:") {
        return host == scope_value;
    }

    if let Some(host) = key.strip_prefix("subdomain:") {
        if wildcard {
            return host == scope_value || host.ends_with(scope_value);
        }
        if host == scope_value {
            return true;
        }
        let suffix = format!(".{}", scope_value);
        if host.ends_with(&suffix) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_types::PolicyAction;
    use tokio::time::Duration;

    fn sample_decision(action: PolicyAction) -> PolicyDecision {
        PolicyDecision {
            action,
            cache_hit: false,
            verdict: None,
        }
    }

    #[tokio::test]
    async fn memory_cache_expires_entries() {
        let cache = CacheClient::new(None, "test".into()).unwrap();
        cache
            .set(
                "key".into(),
                sample_decision(PolicyAction::Allow),
                Duration::from_millis(50),
            )
            .await
            .unwrap();
        assert!(cache.get("key").await.unwrap().is_some());
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(cache.get("key").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn review_invalidation_clears_entry() {
        let cache = CacheClient::new(None, "test".into()).unwrap();
        cache
            .set(
                "domain:example.com".into(),
                sample_decision(PolicyAction::Block),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        let payload = serde_json::json!({
            "kind": "review",
            "normalized_key": "domain:example.com"
        })
        .to_string();
        CacheClient::apply_invalidation(&cache.memory, &payload)
            .await
            .unwrap();
        assert!(cache.get("domain:example.com").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn domain_scope_invalidation_clears_subdomains() {
        let cache = CacheClient::new(None, "test".into()).unwrap();
        cache
            .set(
                "subdomain:foo.example.com".into(),
                sample_decision(PolicyAction::Review),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        let payload = serde_json::json!({
            "kind": "override",
            "scope_type": "domain",
            "scope_value": "example.com"
        })
        .to_string();
        CacheClient::apply_invalidation(&cache.memory, &payload)
            .await
            .unwrap();
        assert!(cache
            .get("subdomain:foo.example.com")
            .await
            .unwrap()
            .is_none());
    }
}
