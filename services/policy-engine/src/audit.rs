use anyhow::Result;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct PolicyAuditLogger {
    pool: PgPool,
}

#[derive(Debug, Default)]
pub struct PolicyAuditEvent {
    pub action: String,
    pub actor: Option<String>,
    pub policy_id: Option<Uuid>,
    pub version: Option<String>,
    pub status: Option<String>,
    pub notes: Option<String>,
    pub diff: Option<Value>,
}

impl PolicyAuditLogger {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn log(&self, event: PolicyAuditEvent) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO policy_audit_events
                (id, policy_id, action, actor, version, status, notes, diff)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        )
        .bind(Uuid::new_v4())
        .bind(event.policy_id)
        .bind(event.action)
        .bind(event.actor)
        .bind(event.version)
        .bind(event.status)
        .bind(event.notes)
        .bind(event.diff)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
