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
    pagination::{
        cursor_limit, decode_cursor_with_direction, encode_directional_cursor, CursorDirection,
        CursorPaged,
    },
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
        .map(decode_cursor_with_direction::<CliLogCursor>)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let (cursor_direction, cursor_anchor) = cursor
        .as_ref()
        .map(|(direction, anchor)| (*direction, Some(anchor)))
        .unwrap_or((CursorDirection::Next, None));

    let cursor_created_at = cursor_anchor.map(|c| c.created_at);
    let cursor_id = cursor_anchor.map(|c| c.id).unwrap_or_else(Uuid::nil);

    let rows = if cursor_direction == CursorDirection::Prev {
        sqlx::query(
            r#"SELECT id, operator_id, command, args_hash, result, created_at
           FROM cli_operation_logs
           WHERE ($1::text IS NULL OR operator_id = $1)
             AND ($2::timestamptz IS NULL OR (created_at, id) > ($2, $3))
           ORDER BY created_at ASC, id ASC
           LIMIT $4"#,
        )
        .bind(query.operator_id.as_deref())
        .bind(cursor_created_at)
        .bind(cursor_id)
        .bind((limit + 1) as i64)
        .fetch_all(state.pool())
        .await
        .map_err(map_db_error)?
    } else {
        sqlx::query(
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
        .map_err(map_db_error)?
    };

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
    if cursor_direction == CursorDirection::Prev {
        data.reverse();
    }
    let next_cursor = if has_more {
        data.last().and_then(|last| {
            encode_directional_cursor(
                CursorDirection::Next,
                &CliLogCursor {
                    created_at: last.created_at,
                    id: last.id,
                },
            )
            .ok()
        })
    } else {
        None
    };
    let prev_cursor = if query.cursor.is_some() && !data.is_empty() {
        data.first().and_then(|first| {
            encode_directional_cursor(
                CursorDirection::Prev,
                &CliLogCursor {
                    created_at: first.created_at,
                    id: first.id,
                },
            )
            .ok()
        })
    } else {
        None
    };

    Ok(Json(CursorPaged::new_with_prev(
        data,
        limit,
        has_more,
        next_cursor,
        prev_cursor,
    )))
}

fn map_db_error(err: sqlx::Error) -> StatusCode {
    error!(target = "svc-admin", %err, "cli log query failed");
    StatusCode::INTERNAL_SERVER_ERROR
}
