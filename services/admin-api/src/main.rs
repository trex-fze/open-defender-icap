mod auth;

use anyhow::{Context, Result};
use auth::{enforce_admin, AdminAuth};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    middleware,
    routing::{delete, get, post},
    Json, Router,
};
use sqlx::types::chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    PgPool, Row,
};
use std::env;
use tokio::net::TcpListener;
use tracing::{error, info, Level};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct AdminApiConfig {
    pub host: String,
    pub port: u16,
    pub database_url: Option<String>,
    pub admin_token: Option<String>,
}

#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let cfg: AdminApiConfig = config_core::load_config("config/admin-api.json")?;
    let db_url = cfg
        .database_url
        .clone()
        .or_else(|| env::var("OD_ADMIN_DATABASE_URL").ok())
        .or_else(|| env::var("DATABASE_URL").ok())
        .context("database_url required: set config/admin-api.json or OD_ADMIN_DATABASE_URL / DATABASE_URL")?;
    let admin_token = cfg
        .admin_token
        .clone()
        .or_else(|| env::var("OD_ADMIN_TOKEN").ok());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = AppState { pool: pool.clone() };

    let admin_routes = Router::new()
        .route(
            "/api/v1/overrides",
            get(list_overrides).post(create_override),
        )
        .route("/api/v1/overrides/:id", delete(delete_override))
        .route("/api/v1/review-queue", get(list_review_queue))
        .route(
            "/api/v1/review-queue/:id/resolve",
            post(resolve_review_item),
        )
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            AdminAuth {
                token: admin_token,
            },
            enforce_admin,
        ));

    let app = Router::new()
        .route("/health/ready", get(health))
        .route("/health/live", get(health))
        .with_state(state)
        .merge(admin_routes);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(target = "svc-admin", %addr, "admin api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "OK"
}

async fn list_overrides(
    State(state): State<AppState>,
) -> Result<Json<Vec<OverrideRecord>>, StatusCode> {
    let rows = sqlx::query(
        r#"SELECT id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at
        FROM overrides ORDER BY created_at DESC LIMIT 200"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|err| {
        error!(target = "svc-admin", %err, "list_overrides failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut overrides = Vec::with_capacity(rows.len());
    for row in rows {
        overrides.push(map_override_row(&row).map_err(|err| {
            error!(target = "svc-admin", %err, "failed to map override row");
            StatusCode::INTERNAL_SERVER_ERROR
        })?);
    }

    Ok(Json(overrides))
}

async fn create_override(
    State(state): State<AppState>,
    Json(payload): Json<OverrideCreateRequest>,
) -> Result<Json<OverrideRecord>, (StatusCode, Json<ApiError>)> {
    if payload.scope_type.trim().is_empty() || payload.scope_value.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "VALIDATION_ERROR",
                "scope_type and scope_value required",
            )),
        ));
    }

    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO overrides (id, scope_type, scope_value, action, reason, created_by, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(id)
    .bind(&payload.scope_type)
    .bind(&payload.scope_value)
    .bind(&payload.action)
    .bind(&payload.reason)
    .bind(&payload.created_by)
    .bind(payload.expires_at)
    .execute(&state.pool)
    .await
    .map_err(|err| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    ))?;

    let record = sqlx::query(
        r#"SELECT id, scope_type, scope_value, action, reason, created_by, expires_at, status, created_at, updated_at
        FROM overrides WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(|err| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    ))?;

    let mapped = map_override_row(&record).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    Ok(Json(mapped))
}

async fn delete_override(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let rows = sqlx::query("DELETE FROM overrides WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("DB_ERROR", err.to_string())),
            )
        })?;

    if rows.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "override not found")),
        ));
    }

    Ok(())
}

async fn list_review_queue(
    State(state): State<AppState>,
) -> Result<Json<Vec<ReviewRecord>>, StatusCode> {
    let rows = sqlx::query(
        r#"SELECT id, normalized_key, request_metadata, status, submitter, assigned_to, decided_by, decision_notes, decision_action, created_at, updated_at
        FROM review_queue ORDER BY created_at DESC LIMIT 200"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|err| {
        error!(target = "svc-admin", %err, "list_review_queue failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        records.push(map_review_row(&row).map_err(|err| {
            error!(target = "svc-admin", %err, "failed to map review row");
            StatusCode::INTERNAL_SERVER_ERROR
        })?);
    }

    Ok(Json(records))
}

async fn resolve_review_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ReviewResolveRequest>,
) -> Result<Json<ReviewRecord>, (StatusCode, Json<ApiError>)> {
    if payload.status.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new("VALIDATION_ERROR", "status required")),
        ));
    }

    let rows = sqlx::query(
        r#"UPDATE review_queue
            SET status = $1,
                decided_by = $2,
                decision_notes = $3,
                decision_action = $4,
                updated_at = NOW()
          WHERE id = $5
          RETURNING id, normalized_key, request_metadata, status, submitter, assigned_to, decided_by, decision_notes, decision_action, created_at, updated_at"#,
    )
    .bind(&payload.status)
    .bind(&payload.decided_by)
    .bind(&payload.decision_notes)
    .bind(&payload.decision_action)
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|err| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError::new("DB_ERROR", err.to_string())),
    ))?;

    let Some(row) = rows else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("NOT_FOUND", "review item not found")),
        ));
    };

    let mapped = map_review_row(&row).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("DB_ERROR", err.to_string())),
        )
    })?;

    Ok(Json(mapped))
}

#[derive(Debug, Serialize)]
struct ApiError {
    error_code: &'static str,
    message: String,
}

impl ApiError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            error_code: code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OverrideCreateRequest {
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct OverrideRecord {
    id: Uuid,
    scope_type: String,
    scope_value: String,
    action: String,
    reason: Option<String>,
    created_by: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ReviewRecord {
    id: Uuid,
    normalized_key: String,
    request_metadata: Value,
    status: String,
    submitter: Option<String>,
    assigned_to: Option<String>,
    decided_by: Option<String>,
    decision_notes: Option<String>,
    decision_action: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct ReviewResolveRequest {
    status: String,
    decided_by: Option<String>,
    decision_notes: Option<String>,
    decision_action: Option<String>,
}

fn map_override_row(row: &PgRow) -> sqlx::Result<OverrideRecord> {
    Ok(OverrideRecord {
        id: row.try_get("id")?,
        scope_type: row.try_get("scope_type")?,
        scope_value: row.try_get("scope_value")?,
        action: row.try_get("action")?,
        reason: row.try_get("reason")?,
        created_by: row.try_get("created_by")?,
        expires_at: row.try_get("expires_at")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_review_row(row: &PgRow) -> sqlx::Result<ReviewRecord> {
    Ok(ReviewRecord {
        id: row.try_get("id")?,
        normalized_key: row.try_get("normalized_key")?,
        request_metadata: row.try_get("request_metadata")?,
        status: row.try_get("status")?,
        submitter: row.try_get("submitter")?,
        assigned_to: row.try_get("assigned_to")?,
        decided_by: row.try_get("decided_by")?,
        decision_notes: row.try_get("decision_notes")?,
        decision_action: row.try_get("decision_action")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
