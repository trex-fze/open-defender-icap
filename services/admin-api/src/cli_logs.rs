use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::error;
use uuid::Uuid;

use crate::{
    auth::{require_roles, UserContext, ROLE_AUDIT_VIEW},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct CliLogQuery {
    pub operator_id: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct CliLogRecord {
    pub id: Uuid,
    pub operator_id: Option<String>,
    pub command: String,
    pub args_hash: Option<String>,
    pub result: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn list_cli_logs(
    Extension(user): Extension<UserContext>,
    State(state): State<AppState>,
    Query(query): Query<CliLogQuery>,
) -> Result<Json<Vec<CliLogRecord>>, StatusCode> {
    require_roles(&user, ROLE_AUDIT_VIEW)?;
    let limit = query.limit.unwrap_or(50).max(1).min(500);
    let rows = if let Some(operator) = query.operator_id.as_ref() {
        sqlx::query(
            "SELECT id, operator_id, command, args_hash, result, created_at
             FROM cli_operation_logs WHERE operator_id = $1
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(operator)
        .bind(limit as i64)
        .fetch_all(state.pool())
        .await
        .map_err(map_db_error)?
    } else {
        sqlx::query(
            "SELECT id, operator_id, command, args_hash, result, created_at
             FROM cli_operation_logs ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(state.pool())
        .await
        .map_err(map_db_error)?
    };

    let data = rows
        .into_iter()
        .map(|row| CliLogRecord {
            id: row.get("id"),
            operator_id: row.get("operator_id"),
            command: row.get("command"),
            args_hash: row.get("args_hash"),
            result: row.get("result"),
            created_at: row.get("created_at"),
        })
        .collect();
    Ok(Json(data))
}

fn map_db_error(err: sqlx::Error) -> StatusCode {
    error!(target = "svc-admin", %err, "cli log query failed");
    StatusCode::INTERNAL_SERVER_ERROR
}
