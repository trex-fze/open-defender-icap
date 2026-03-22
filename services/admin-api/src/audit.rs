use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use tokio::task;
use tracing::error;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuditLogger {
    pool: PgPool,
}

impl AuditLogger {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn log(&self, event: AuditEvent) {
        let pool = self.pool.clone();
        task::spawn(async move {
            if let Err(err) = insert_event(pool, event).await {
                error!(target = "svc-admin", %err, "failed to write audit event");
            }
        });
    }
}

#[derive(Debug, Serialize)]
pub struct AuditEvent {
    pub actor: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub payload: Option<Value>,
}

async fn insert_event(pool: PgPool, event: AuditEvent) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO audit_events (id, actor, action, target_type, target_id, payload)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(Uuid::new_v4())
    .bind(event.actor)
    .bind(event.action)
    .bind(event.target_type)
    .bind(event.target_id)
    .bind(event.payload)
    .execute(&pool)
    .await?;

    Ok(())
}
