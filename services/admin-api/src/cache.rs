use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use redis::AsyncCommands;
use serde::Serialize;
use serde_json::Value;
use tracing::warn;

const DEFAULT_CHANNEL: &str = "od:cache:invalidate";

#[derive(Clone)]
pub struct CacheInvalidator {
    client: redis::Client,
    channel: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum CacheEvent<'a> {
    Override {
        scope_type: &'a str,
        scope_value: &'a str,
    },
    Policy,
}

impl CacheInvalidator {
    pub fn new(redis_url: String, channel: Option<String>) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let channel = channel.unwrap_or_else(|| DEFAULT_CHANNEL.to_string());
        Ok(Self { client, channel })
    }

    pub async fn invalidate_override(&self, scope_type: &str, scope_value: &str) -> Result<()> {
        self.delete_scope(scope_type, scope_value).await?;
        self.publish(CacheEvent::Override {
            scope_type,
            scope_value,
        })
        .await
    }

    pub async fn invalidate_policy(&self) -> Result<()> {
        self.flush_domain_entries().await?;
        self.publish(CacheEvent::Policy).await
    }

    pub async fn invalidate_key(&self, key: &str) -> Result<()> {
        self.delete_key(key).await
    }

    pub async fn inspect_key(&self, key: &str) -> Result<Option<(Value, Option<DateTime<Utc>>)>> {
        if key.trim().is_empty() {
            return Ok(None);
        }

        let mut conn = self.client.get_async_connection().await?;
        let raw: Option<String> = conn.get(key).await?;
        let Some(raw) = raw else {
            return Ok(None);
        };

        let ttl_seconds: i64 = redis::cmd("TTL").arg(key).query_async(&mut conn).await?;
        let expires_at = if ttl_seconds > 0 {
            Some(Utc::now() + ChronoDuration::seconds(ttl_seconds))
        } else {
            None
        };
        let value = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
        Ok(Some((value, expires_at)))
    }

    async fn delete_scope(&self, scope_type: &str, scope_value: &str) -> Result<()> {
        match scope_type {
            "domain" => self.delete_domain_scope(scope_value).await?,
            other => {
                warn!(
                    target = "svc-admin",
                    scope_type = other,
                    "flushing entire cache set for non-domain scope"
                );
                self.flush_domain_entries().await?
            }
        }
        Ok(())
    }

    async fn delete_domain_scope(&self, scope_value: &str) -> Result<()> {
        let domain_value = match scope_value.strip_prefix("*.") {
            Some(rest) => rest,
            None => scope_value,
        };

        // Always drop the domain-level cache entry.
        self.delete_key(&format!("domain:{}", domain_value)).await?;

        // Delete direct subdomain entry matching the domain itself and any nested subdomains.
        self.delete_key(&format!("subdomain:{}", domain_value))
            .await?;
        self.delete_pattern(&format!("subdomain:*.{}", domain_value))
            .await?;

        Ok(())
    }

    async fn flush_domain_entries(&self) -> Result<()> {
        self.delete_pattern("domain:*").await?;
        self.delete_pattern("subdomain:*").await?;
        Ok(())
    }

    async fn delete_key(&self, key: &str) -> Result<()> {
        if key.trim().is_empty() {
            return Ok(());
        }
        let mut conn = self.client.get_async_connection().await?;
        let _: () = conn.del(key).await?;
        Ok(())
    }

    async fn delete_pattern(&self, pattern: &str) -> Result<()> {
        if pattern.trim().is_empty() {
            return Ok(());
        }

        let mut conn = self.client.get_async_connection().await?;
        let mut cursor: u64 = 0;
        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .cursor_arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await?;

            if !keys.is_empty() {
                let _: () = redis::cmd("DEL")
                    .arg(keys)
                    .query_async(&mut conn)
                    .await
                    .context("failed to delete cache keys")?;
            }

            if next_cursor == 0 {
                break;
            }
            cursor = next_cursor;
        }

        Ok(())
    }

    async fn publish(&self, event: CacheEvent<'_>) -> Result<()> {
        let mut conn = self.client.get_async_connection().await?;
        let payload = serde_json::to_string(&event)?;
        let _: () = redis::cmd("PUBLISH")
            .arg(&self.channel)
            .arg(payload)
            .query_async(&mut conn)
            .await
            .context("failed to publish cache invalidation event")?;
        Ok(())
    }
}
impl CacheInvalidator {
    pub fn channel_name(&self) -> &str {
        &self.channel
    }
}
