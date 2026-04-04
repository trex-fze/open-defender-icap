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
    pagination::{cursor_limit, decode_cursor, encode_cursor, CursorPaged},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct CliLogQuery {
    pub operator_id: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CliLogCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
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
) -> Result<Json<CursorPaged<CliLogRecord>>, StatusCode> {
    require_roles(&user, ROLE_AUDIT_VIEW)?;
    let limit = cursor_limit(query.limit);
    let cursor = query
        .cursor
        .as_deref()
        .map(decode_cursor::<CliLogCursor>)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let cursor_created_at = cursor.as_ref().map(|c| c.created_at);
    let cursor_id = cursor
        .as_ref()
        .map(|c| c.id)
        .unwrap_or_else(Uuid::nil);

    let rows = sqlx::query(
        r#"SELECT id, operator_id, command, args_hash, result, created_at
           FROM cli_operation_logs
           WHERE ($1::text IS NULL OR operator_id = $1)
             AND ($2::timestamptz IS NULL OR (created_at, id) < ($2, $3))
           ORDER BY created_at DESC, id DESC
           LIMIT $4"#,
    )
    .bind(query.operator_id.as_deref())
    .bind(cursor_created_at)
    .bind(cursor_id)
    .bind((limit + 1) as i64)
    .fetch_all(state.pool())
    .await
    .map_err(map_db_error)?;

    let mut data: Vec<CliLogRecord> = rows
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

    let has_more = data.len() > limit as usize;
    if has_more {
        data.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        data.last().and_then(|last| {
            encode_cursor(&CliLogCursor {
                created_at: last.created_at,
                id: last.id,
            })
            .ok()
        })
    } else {
        None
    };

    Ok(Json(CursorPaged::new(data, limit, has_more, next_cursor)))
}

fn map_db_error(err: sqlx::Error) -> StatusCode {
    error!(target = "svc-admin", %err, "cli log query failed");
    StatusCode::INTERNAL_SERVER_ERROR
}
