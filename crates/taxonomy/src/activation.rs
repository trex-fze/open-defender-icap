use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use sqlx::{
    types::chrono::{DateTime, Utc},
    PgPool, Row,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::time::sleep;
use tracing::{info, warn};
use uuid::Uuid;

pub const CATEGORY_SENTINEL_ID: &str = "__CATEGORY__";

#[derive(Debug)]
pub struct ActivationState {
    inner: RwLock<ActivationProfile>,
}

impl ActivationState {
    pub async fn load(pool: &PgPool) -> Result<Self> {
        let profile = ActivationProfile::load_from_db(pool).await?;
        Ok(Self {
            inner: RwLock::new(profile),
        })
    }

    pub fn spawn_refresh_task(self: Arc<Self>, pool: PgPool) {
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(30)).await;
                match ActivationProfile::load_from_db(&pool).await {
                    Ok(profile) => {
                        if let Some(version) = self.replace_if_new(profile) {
                            info!(
                                target = "taxonomy",
                                %version,
                                "activation profile reloaded"
                            );
                        }
                    }
                    Err(err) => {
                        warn!(
                            target = "taxonomy",
                            %err,
                            "failed to refresh activation profile"
                        );
                    }
                }
            }
        });
    }

    pub fn is_enabled(&self, category_id: &str, subcategory_id: Option<&str>) -> bool {
        let profile = self.inner.read();
        profile.is_enabled(category_id, subcategory_id)
    }

    pub fn allow_all() -> Self {
        Self {
            inner: RwLock::new(ActivationProfile::allow_all()),
        }
    }

    pub fn from_maps(
        category_states: HashMap<String, bool>,
        subcategory_states: HashMap<String, HashMap<String, bool>>,
        default_enabled: bool,
    ) -> Self {
        Self {
            inner: RwLock::new(ActivationProfile {
                id: Uuid::nil(),
                version: "static".into(),
                updated_at: Utc::now(),
                category_states,
                subcategory_states,
                default_enabled,
            }),
        }
    }

    #[cfg(test)]
    pub fn testing_from_maps(
        category_states: HashMap<String, bool>,
        subcategory_states: HashMap<String, HashMap<String, bool>>,
    ) -> Self {
        Self {
            inner: RwLock::new(ActivationProfile {
                id: Uuid::nil(),
                version: "test".into(),
                updated_at: Utc::now(),
                category_states,
                subcategory_states,
                default_enabled: false,
            }),
        }
    }

    fn replace_if_new(&self, profile: ActivationProfile) -> Option<String> {
        let mut guard = self.inner.write();
        if profile.updated_at > guard.updated_at || profile.id != guard.id {
            let version = profile.version.clone();
            *guard = profile;
            Some(version)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
struct ActivationProfile {
    id: Uuid,
    version: String,
    updated_at: DateTime<Utc>,
    category_states: HashMap<String, bool>,
    subcategory_states: HashMap<String, HashMap<String, bool>>,
    default_enabled: bool,
}

impl ActivationProfile {
    async fn load_from_db(pool: &PgPool) -> Result<Self> {
        let row = sqlx::query(
            "SELECT id, version, updated_at FROM taxonomy_activation_profiles ORDER BY updated_at DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;

        let Some(row) = row else {
            return Err(anyhow!("taxonomy activation profile not found"));
        };

        let profile_id: Uuid = row.get("id");
        let version: String = row.get("version");
        let updated_at: DateTime<Utc> = row.get("updated_at");

        let entries = sqlx::query(
            "SELECT category_id, subcategory_id, enabled FROM taxonomy_activation_entries WHERE profile_id = $1",
        )
        .bind(profile_id)
        .fetch_all(pool)
        .await?;

        let mut category_states = HashMap::new();
        let mut subcategory_states: HashMap<String, HashMap<String, bool>> = HashMap::new();
        for entry in entries {
            let category_id: String = entry.get("category_id");
            let subcategory_id: String = entry.get("subcategory_id");
            let enabled: bool = entry.get("enabled");
            if subcategory_id == CATEGORY_SENTINEL_ID {
                category_states.insert(category_id, enabled);
            } else {
                subcategory_states
                    .entry(category_id.clone())
                    .or_default()
                    .insert(subcategory_id, enabled);
            }
        }

        Ok(Self {
            id: profile_id,
            version,
            updated_at,
            category_states,
            subcategory_states,
            default_enabled: false,
        })
    }

    fn is_enabled(&self, category_id: &str, subcategory_id: Option<&str>) -> bool {
        if let Some(sub_id) = subcategory_id {
            if let Some(subs) = self.subcategory_states.get(category_id) {
                if let Some(state) = subs.get(sub_id) {
                    return *state;
                }
            }
        }
        if let Some(state) = self.category_states.get(category_id) {
            return *state;
        }
        self.default_enabled
    }

    fn allow_all() -> Self {
        Self {
            id: Uuid::nil(),
            version: "allow-all".into(),
            updated_at: Utc::now(),
            category_states: HashMap::new(),
            subcategory_states: HashMap::new(),
            default_enabled: true,
        }
    }
}
