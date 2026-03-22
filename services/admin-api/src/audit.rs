use chrono::Utc;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use tokio::task;
use tracing::error;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuditLogger {
    pool: PgPool,
    elastic: Option<ElasticExporter>,
}

impl AuditLogger {
    pub fn new(pool: PgPool, elastic: Option<ElasticExporter>) -> Self {
        Self { pool, elastic }
    }

    pub async fn log(&self, event: AuditEvent) {
        let pool = self.pool.clone();
        let elastic = self.elastic.clone();
        task::spawn(async move {
            match insert_event(pool, event).await {
                Ok(record) => {
                    if let Some(exporter) = elastic {
                        if let Err(err) = exporter.send(&record).await {
                            error!(target = "svc-admin", %err, "failed to export audit event to elasticsearch");
                        }
                    }
                }
                Err(err) => {
                    error!(target = "svc-admin", %err, "failed to write audit event");
                }
            }
        });
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct AuditEvent {
    pub actor: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub payload: Option<Value>,
}

#[derive(Debug, Serialize, Clone)]
struct AuditRecord {
    id: Uuid,
    actor: Option<String>,
    action: String,
    target_type: Option<String>,
    target_id: Option<String>,
    payload: Option<Value>,
    created_at: chrono::DateTime<Utc>,
}

async fn insert_event(pool: PgPool, event: AuditEvent) -> Result<AuditRecord, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO audit_events (id, actor, action, target_type, target_id, payload)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(id)
    .bind(&event.actor)
    .bind(&event.action)
    .bind(&event.target_type)
    .bind(&event.target_id)
    .bind(&event.payload)
    .execute(&pool)
    .await?;

    Ok(AuditRecord {
        id,
        actor: event.actor,
        action: event.action,
        target_type: event.target_type,
        target_id: event.target_id,
        payload: event.payload,
        created_at: Utc::now(),
    })
}

#[derive(Clone)]
pub struct ElasticExporter {
    client: Client,
    endpoint: String,
    index: String,
    api_key: Option<String>,
}

impl ElasticExporter {
    pub fn new(base_url: String, index: String, api_key: Option<String>) -> anyhow::Result<Self> {
        let client = Client::builder().build()?;
        Ok(Self {
            client,
            endpoint: base_url.trim_end_matches('/').to_string(),
            index,
            api_key,
        })
    }

    async fn send(&self, record: &AuditRecord) -> Result<(), reqwest::Error> {
        let url = format!("{}/{}/_doc", self.endpoint, self.index);
        let mut req = self.client.post(&url).json(record);
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("ApiKey {}", key));
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }
}
